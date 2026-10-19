//! R18 offline population owner. One call is the post-production three-second
//! economic boundary, never elapsed wall time. Model/ledger/roster/trips must be
//! restored from the same generation. Public commands and legacy file conversion
//! remain outside this experimental boundary.
use crate::{
    building::Site,
    logistics::LogisticsTrips,
    progression::{self, Worker},
    resources::{Ledger, PendingOwner},
};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner},
    model::{ActorState, Cargo, ConstructionState, Task, Town},
    units::{Position, Quantity},
};
use std::collections::BTreeSet;
mod save;

const NAMES: [&str; 32] = [
    "Ada", "Rowan", "Wren", "Bram", "Hazel", "Finn", "Mira", "Jasper", "Elara", "Theo", "Ivy",
    "Silas", "Nora", "Felix", "Willow", "Owen", "Cora", "Alden", "Flora", "Hugo", "Lila", "Emmett",
    "Pearl", "Arthur", "Clara", "Tobin", "Rose", "Cedric", "Iris", "Elias", "Mae", "Oscar",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Ownership,
    MissingResident,
    MissingPlace,
    Unfinished,
    Full,
    Qualification,
    Release,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Resident {
    pub actor: ActorId,
    pub name: String,
    pub worker: Worker,
    pub assignment: Option<BuildingId>,
}
/// Current R12 facts, in saved building order. Pending upgrades never supply
/// their future level, equipment or staffing capacity.
#[derive(Clone, Debug)]
pub struct Place {
    pub id: BuildingId,
    pub site: Site,
}
impl Place {
    fn complete(&self, town: &Town) -> bool {
        town.building(&self.id)
            .is_ok_and(|b| b.construction == ConstructionState::Complete)
    }
    fn housed(&self, town: &Town) -> bool {
        self.complete(town) || self.site.upgrade.is_some()
    }
    fn capacity(&self) -> Result<usize, Error> {
        progression::worker_capacity(&self.site.kind, self.site.capacity_level)
            .map(|n| n as usize)
            .map_err(|_| Error::Invalid)
    }
    fn comfort_level(&self) -> usize {
        usize::from(self.site.level) + usize::from(self.site.equipment.contains("timber_frame"))
    }
    fn job(&self) -> &str {
        match self.site.kind.as_str() {
            "storehouse" => return "hauling",
            "woodcutter" => return "woodcutting",
            "farm" | "acre" => return "farming",
            "fishing_hut" => return "fishing",
            _ => (),
        }
        crate::resources::public_catalog()["buildings"][&self.site.kind]["job"]
            .as_str()
            .unwrap_or("idle")
    }
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub food_shortage_seconds: f64,
    pub housing_shortage_seconds: f64,
    pub arrival_delay_seconds: f64,
    pub departure_count: u64,
    #[serde(deserialize_with = "save::required")]
    pub last_departure: Option<Departure>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Departure {
    pub name: String,
    pub job: String,
    pub reason: String,
    pub practice: std::collections::BTreeMap<String, f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Population {
    owner: Owner,
    sandbox: bool,
    residents: Vec<Resident>,
    history: History,
    /// Tools returned by departures. R18 slice 3 must move these into the
    /// founder's tool inventory in the same transaction, exactly once.
    returned_tools: std::collections::BTreeMap<String, u64>,
}
pub enum Command {
    Assign {
        actor: ActorId,
        building: BuildingId,
    },
    Remove(ActorId),
    Job {
        actor: ActorId,
        job: String,
    },
}
impl Population {
    /// Explicit import admission preserves named residents and saved empty talent
    /// maps. Missing talents alone receive the legacy resident-order defaults.
    pub fn restore(
        town: &Town,
        sandbox: bool,
        residents: Vec<Resident>,
        history: History,
    ) -> Result<Self, Error> {
        let mut result = Self {
            owner: town.owner().clone(),
            sandbox,
            residents,
            history,
            returned_tools: Default::default(),
        };
        result.validate(town)?;
        result.ensure_names();
        for (i, r) in result.residents.iter_mut().enumerate() {
            r.worker.ensure_talents(i);
            if sandbox {
                r.worker.sandbox = true;
            }
        }
        Ok(result)
    }
    pub fn residents(&self) -> &[Resident] {
        &self.residents
    }
    pub fn history(&self) -> &History {
        &self.history
    }
    pub fn returned_tools(&self) -> &std::collections::BTreeMap<String, u64> {
        &self.returned_tools
    }
    /// Production owners update the same personal practice record used for
    /// qualification. Changing assignment/job still goes through `command`.
    pub fn worker_mut(&mut self, actor: &ActorId) -> Result<&mut Worker, Error> {
        let i = self.index(actor)?;
        Ok(&mut self.residents[i].worker)
    }
    /// Drain into the founder inventory in the caller's transaction; repeated
    /// drains/reloads cannot return the same departed resident's tool twice.
    pub fn take_returned_tools(
        &mut self,
        owner: &Owner,
    ) -> Result<std::collections::BTreeMap<String, u64>, Error> {
        if owner != &self.owner {
            return Err(Error::Ownership);
        }
        Ok(std::mem::take(&mut self.returned_tools))
    }
    /// Resolve omitted old building indexes once on explicit legacy admission.
    /// Python's historical fallback uses resident list order modulo eligible
    /// workplaces, with trees for woodcutters and water for all other jobs except
    /// farms. Keep that rule even for professions that do not consume water.
    /// Explicit assignments, including over-capacity old saves, remain unchanged.
    pub fn resolve_legacy_assignments(
        &mut self,
        town: &Town,
        places: &[Place],
        ledger: &Ledger,
        sources: &[lantern_core::spatial::Resource],
    ) -> Result<(), Error> {
        self.validate_places(town, places, ledger)?;
        let mut assignments = Vec::new();
        for (index, r) in self.residents.iter().enumerate() {
            if r.assignment.is_some() || matches!(r.worker.job(), "idle" | "building") {
                continue;
            }
            let choices: Vec<_> = places
                .iter()
                .filter(|p| {
                    if p.job() != r.worker.job() || p.site.kind == "acre" || !p.housed(town) {
                        return false;
                    }
                    if p.site.kind == "farm" {
                        return true;
                    }
                    let source = if p.site.kind == "woodcutter" {
                        "tree"
                    } else {
                        "water"
                    };
                    let boat = progression::fishing_boat_level(&p.site.equipment);
                    let reach = match p.site.kind.as_str() {
                        "mine" | "woodcutter" => 10,
                        "fishing_hut" => {
                            3 + i64::from(p.site.level) - 1
                                + if boat > 0 {
                                    3 + 2 * (i64::from(boat) - 1)
                                } else {
                                    0
                                }
                        }
                        _ => 3,
                    };
                    let origin = town.building(&p.id).unwrap().location;
                    sources.iter().any(|r| {
                        r.kind == source && {
                            let tile = r.footprint.origin();
                            (i64::from(tile.x) - i64::from(origin.x)).abs()
                                + (i64::from(tile.y) - i64::from(origin.y)).abs()
                                <= reach
                        }
                    })
                })
                .collect();
            if !choices.is_empty() {
                assignments.push((index, choices[index % choices.len()].id.clone()));
            }
        }
        for (index, b) in assignments {
            self.residents[index].assignment = Some(b);
        }
        Ok(())
    }
    fn validate(&self, town: &Town) -> Result<(), Error> {
        if self.owner != *town.owner() {
            return Err(Error::Ownership);
        }
        if [
            self.history.food_shortage_seconds,
            self.history.housing_shortage_seconds,
            self.history.arrival_delay_seconds,
        ]
        .iter()
        .any(|n| !n.is_finite() || *n < 0.)
        {
            return Err(Error::Invalid);
        }
        if self.history.last_departure.as_ref().is_some_and(|d| {
            d.name.trim().is_empty()
                || Worker::new(&d.job).is_err()
                || !matches!(d.reason.as_str(), "food" | "housing" | "food and housing")
                || d.practice
                    .iter()
                    .any(|(j, n)| Worker::new(j).is_err() || !n.is_finite() || *n < 0.)
        }) {
            return Err(Error::Invalid);
        }
        let mut seen = BTreeSet::new();
        for r in &self.residents {
            town.actor(&r.actor).map_err(|_| Error::Ownership)?;
            if !seen.insert(r.actor.clone()) {
                return Err(Error::Invalid);
            }
            if let Some(b) = &r.assignment {
                town.building(b).map_err(|_| Error::MissingPlace)?;
            }
        }
        Ok(())
    }
    fn validate_places(&self, town: &Town, places: &[Place], ledger: &Ledger) -> Result<(), Error> {
        self.validate(town)?;
        if places.len() != town.buildings().len() {
            return Err(Error::Invalid);
        }
        for (p, b) in places.iter().zip(town.buildings()) {
            if p.id != b.id
                || p.site.level == 0
                || p.site.capacity_level == 0
                || ledger
                    .store(&p.id)
                    .map_err(|_| Error::MissingPlace)?
                    .policy
                    .kind
                    != p.site.kind
                || (p.site.upgrade.is_some() && p.complete(town))
            {
                return Err(Error::Invalid);
            }
            p.capacity()?;
        }
        Ok(())
    }
    fn next_name(&self) -> String {
        for generation in 1.. {
            for base in NAMES {
                let name = if generation == 1 {
                    base.to_string()
                } else {
                    format!("{base} {generation}")
                };
                if !self.residents.iter().any(|r| r.name == name)
                    && self
                        .history
                        .last_departure
                        .as_ref()
                        .is_none_or(|d| d.name != name)
                {
                    return name;
                }
            }
        }
        unreachable!()
    }
    fn ensure_names(&mut self) {
        for i in 0..self.residents.len() {
            let name = &self.residents[i].name;
            if name.trim().is_empty()
                || name
                    .strip_prefix("Settler ")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(name_digit))
            {
                self.residents[i].name = self.next_name();
            }
        }
    }
    pub fn housing(&self, town: &Town, places: &[Place]) -> Result<usize, Error> {
        places
            .iter()
            .filter(|p| p.site.kind == "home" && p.housed(town))
            .try_fold(5usize, |n, p| {
                n.checked_add((2 * p.comfort_level()).max(if self.sandbox {
                    p.capacity()?
                } else {
                    0
                }))
                .ok_or(Error::Invalid)
            })
    }
    pub fn loyalty(&self, town: &Town, places: &[Place]) -> f64 {
        if self.residents.is_empty() {
            return 0.;
        }
        let mut comfort = Vec::new();
        for p in places
            .iter()
            .filter(|p| p.site.kind == "home" && p.housed(town))
        {
            let level = p.comfort_level();
            comfort.extend(std::iter::repeat_n(
                (0.25 * (level - 1) as f64).min(0.75),
                2 * level,
            ));
        }
        comfort.sort_by(|a, b| b.total_cmp(a));
        comfort.into_iter().take(self.residents.len()).sum::<f64>() / self.residents.len() as f64
    }
    fn index(&self, a: &ActorId) -> Result<usize, Error> {
        a.identity()
            .validate_owner(&self.owner)
            .map_err(|_| Error::Ownership)?;
        self.residents
            .iter()
            .position(|r| r.actor == *a)
            .ok_or(Error::MissingResident)
    }
    fn release(
        &self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        actor: &ActorId,
        hall: &BuildingId,
    ) -> Result<(), Error> {
        trips
            .cancel(town, ledger, actor, hall)
            .map_err(|_| Error::Release)?;
        let pending: Vec<_> = ledger
            .stores()
            .iter()
            .filter(|s| s.pending.contains_key(&PendingOwner::Worker(actor.clone())))
            .map(|s| s.id.clone())
            .collect();
        for b in pending {
            ledger
                .orphan_pending(&b, actor)
                .map_err(|_| Error::Release)?;
        }
        if ledger.held().contains_key(actor) {
            ledger
                .return_cargo(actor, hall)
                .map_err(|_| Error::Release)?;
        }
        // Ledger owns physical goods. Model cargo is never an additional award.
        town.set_cargo(actor, Cargo::default())
            .map_err(|_| Error::Release)?;
        town.set_task(actor, Task::Idle).map_err(|_| Error::Release)
    }
    #[allow(clippy::too_many_arguments)]
    fn assign(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        places: &[Place],
        actor: &ActorId,
        building: &BuildingId,
        hall: &BuildingId,
    ) -> Result<(), Error> {
        let i = self.index(actor)?;
        let p = places
            .iter()
            .find(|p| p.id == *building)
            .ok_or(Error::MissingPlace)?;
        if p.site.kind == "hall" {
            return Err(Error::Full);
        }
        if !p.complete(town) {
            return Err(Error::Unfinished);
        }
        if self
            .residents
            .iter()
            .filter(|r| r.actor != *actor && r.assignment.as_ref() == Some(building))
            .count()
            >= p.capacity()?
        {
            return Err(Error::Full);
        }
        self.residents[i]
            .worker
            .qualification(p.job())
            .map_err(|_| Error::Qualification)?;
        self.release(town, ledger, trips, actor, hall)?;
        self.residents[i].assignment = Some(building.clone());
        self.residents[i]
            .worker
            .set_job(p.job())
            .map_err(|_| Error::Qualification)
    }
    /// Authorization uses the authenticated owner, never a client-supplied town.
    /// All components commit together; rejected commands preserve their originals.
    pub fn command(
        &mut self,
        owner: &Owner,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        places: &[Place],
        command: Command,
    ) -> Result<(), Error> {
        if owner != &self.owner {
            return Err(Error::Ownership);
        }
        self.transact(
            town,
            ledger,
            trips,
            places,
            |s, t, l, tr, h| match command {
                Command::Assign { actor, building } => {
                    s.assign(t, l, tr, places, &actor, &building, h)
                }
                Command::Remove(actor) => {
                    let i = s.index(&actor)?;
                    s.release(t, l, tr, &actor, h)?;
                    s.residents[i].assignment = None;
                    s.residents[i]
                        .worker
                        .set_job("idle")
                        .map_err(|_| Error::Invalid)
                }
                Command::Job { actor, job } => {
                    let i = s.index(&actor)?;
                    s.residents[i]
                        .worker
                        .qualification(&job)
                        .map_err(|_| Error::Qualification)?;
                    if matches!(job.as_str(), "idle" | "building") {
                        s.release(t, l, tr, &actor, h)?;
                        s.residents[i].assignment = None;
                        s.residents[i]
                            .worker
                            .set_job(&job)
                            .map_err(|_| Error::Qualification)
                    } else {
                        let choices: Vec<_> = places
                            .iter()
                            .filter(|p| {
                                p.site.kind != "acre"
                                    && p.job() == job
                                    && p.complete(t)
                                    && s.residents
                                        .iter()
                                        .filter(|r| {
                                            r.actor != actor && r.assignment.as_ref() == Some(&p.id)
                                        })
                                        .count()
                                        < p.capacity().unwrap_or(0)
                            })
                            .collect();
                        let p = choices
                            .iter()
                            .find(|p| s.residents[i].assignment.as_ref() == Some(&p.id))
                            .or_else(|| choices.first())
                            .ok_or(Error::Full)?;
                        s.assign(t, l, tr, places, &actor, &p.id, h)
                    }
                }
            },
        )
    }
    fn transact(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        places: &[Place],
        edit: impl FnOnce(
            &mut Self,
            &mut Town,
            &mut Ledger,
            &mut LogisticsTrips,
            &BuildingId,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.validate_places(town, places, ledger)?;
        let hall = places
            .iter()
            .find(|p| p.site.kind == "hall")
            .ok_or(Error::MissingPlace)?
            .id
            .clone();
        let (mut s, mut t, mut l, mut tr) =
            (self.clone(), town.clone(), ledger.clone(), trips.clone());
        edit(&mut s, &mut t, &mut l, &mut tr, &hall)?;
        s.validate(&t)?;
        *self = s;
        *town = t;
        *ledger = l;
        *trips = tr;
        Ok(())
    }
    /// Inject only the random draws actually requested by Python's rules. Spawn
    /// IDs come from the world's monotonic allocator (unused IDs may be burned on
    /// failure). No tick, random draw, arrival or autofill occurs during decode.
    pub fn economic_tick(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        places: &[Place],
        draw: &mut impl FnMut(usize) -> usize,
        spawn: &mut impl FnMut() -> ActorId,
    ) -> Result<(), Error> {
        self.transact(town, ledger, trips, places, |s, t, l, tr, h| {
            let working = s
                .residents
                .iter()
                .filter(|r| {
                    r.worker.job() != "idle"
                        || matches!(
                            t.actor(&r.actor).unwrap().state().task,
                            Task::Construction { .. } | Task::RoadConstruction { .. }
                        )
                })
                .count();
            let needed = progression::food_demand(s.residents.len(), working)
                .map_err(|_| Error::Invalid)?
                .get()
                * 3.
                / 60.;
            let shortage = l.food().map_err(|_| Error::Invalid)?.get() + 1e-9 < needed;
            l.eat(Quantity::new(needed).map_err(|_| Error::Invalid)?)
                .map_err(|_| Error::Invalid)?;
            s.history.food_shortage_seconds = if shortage {
                s.history.food_shortage_seconds + 3.
            } else {
                0.
            };
            s.history.housing_shortage_seconds = if s.residents.len() > s.housing(t, places)? {
                s.history.housing_shortage_seconds + 3.
            } else {
                0.
            };
            s.history.arrival_delay_seconds = (s.history.arrival_delay_seconds - 3.).max(0.);
            let food = s.history.food_shortage_seconds >= 60.;
            let housing = s.history.housing_shortage_seconds >= 60.;
            if !s.sandbox && (food || housing) && s.residents.len() > 1 {
                let loyalty = if housing { 0. } else { s.loyalty(t, places) };
                let chance = (5. * (1. - loyalty) * 100.).round_ties_even() as usize;
                let roll = draw(10000);
                if roll >= 10000 {
                    return Err(Error::Invalid);
                }
                if roll < chance {
                    let index = draw(s.residents.len());
                    if index >= s.residents.len() {
                        return Err(Error::Invalid);
                    }
                    let r = s.residents[index].clone();
                    s.release(t, l, tr, &r.actor, h)?;
                    t.remove_actor(&r.actor).map_err(|_| Error::Release)?;
                    s.residents.remove(index);
                    if let Some(tool) = &r.worker.tool
                        && known_tool(tool)
                    {
                        let n = s.returned_tools.entry(tool.clone()).or_default();
                        *n = n.checked_add(1).ok_or(Error::Invalid)?;
                    }
                    s.history.departure_count = s
                        .history
                        .departure_count
                        .checked_add(1)
                        .ok_or(Error::Invalid)?;
                    s.history.last_departure = Some(Departure {
                        name: r.name,
                        job: r.worker.job().into(),
                        reason: if food && housing {
                            "food and housing"
                        } else if food {
                            "food"
                        } else {
                            "housing"
                        }
                        .into(),
                        practice: r
                            .worker
                            .skills()
                            .iter()
                            .filter(|(job, n)| {
                                !matches!(job.as_str(), "idle" | "building") && n.get() != 0.
                            })
                            .map(|(j, n)| (j.clone(), n.get()))
                            .collect(),
                    });
                    s.history.food_shortage_seconds = 0.;
                    s.history.housing_shortage_seconds = 0.;
                    s.history.arrival_delay_seconds = 60.;
                }
            }
            if !s.sandbox
                && s.residents.len() < s.housing(t, places)?
                && l.food().map_err(|_| Error::Invalid)?.get() >= 20.
                && s.history.arrival_delay_seconds == 0.
            {
                l.eat(Quantity::new(10.).unwrap())
                    .map_err(|_| Error::Invalid)?;
                s.arrive(t, h, spawn())?;
            }
            if s.sandbox {
                while s.residents.len() < s.housing(t, places)? {
                    s.arrive(t, h, spawn())?;
                }
            }
            for p in places {
                if p.site.kind != "home" || !p.complete(t) {
                    continue;
                }
                let available = p.capacity()?.saturating_sub(
                    s.residents
                        .iter()
                        .filter(|r| r.assignment.as_ref() == Some(&p.id))
                        .count(),
                );
                let waiting: Vec<_> = s
                    .residents
                    .iter()
                    .filter(|r| r.worker.job() == "idle" && r.assignment.is_none())
                    .take(available)
                    .map(|r| r.actor.clone())
                    .collect();
                for a in waiting {
                    s.assign(t, l, tr, places, &a, &p.id, h)?;
                }
            }
            Ok(())
        })
    }
    fn arrive(&mut self, town: &mut Town, hall: &BuildingId, actor: ActorId) -> Result<(), Error> {
        let location = town
            .building(hall)
            .map_err(|_| Error::MissingPlace)?
            .location;
        town.insert_actor(ActorState::new(
            actor.clone(),
            Position::new(f64::from(location.x), f64::from(location.y)).unwrap(),
            Cargo::default(),
        ))
        .map_err(|_| Error::Invalid)?;
        let mut worker = Worker::new("idle").unwrap();
        worker.sandbox = self.sandbox;
        worker.ensure_talents(self.residents.len());
        self.residents.push(Resident {
            actor,
            name: self.next_name(),
            worker,
            assignment: None,
        });
        Ok(())
    }
}

fn known_tool(tool: &str) -> bool {
    crate::resources::public_catalog()["crafted_worker_tools"]
        .get(tool)
        .is_some()
        || matches!(
            tool,
            "fine_axe"
                | "fine_fishing_rod"
                | "fine_sickle"
                | "steel_fishing_rod"
                | "masterwork_fishing_rod"
                | "steel_axe"
                | "masterwork_axe"
                | "steel_sickle"
                | "masterwork_sickle"
        )
}

// Python str.isdigit includes superscripts/circled digits and excludes numeric
// fractions. Rust char::is_numeric has a different identity-migration contract.
fn name_digit(c: char) -> bool {
    static RANGES: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    RANGES
        .get_or_init(|| serde_json::from_str(include_str!("population/name_digits.json")).unwrap())
        .iter()
        .any(|[a, b]| (*a..=*b).contains(&(c as u32)))
}

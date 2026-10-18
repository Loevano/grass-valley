//! Experimental fishing owners. Movement clocks and three-second production batches
//! are separate, as in Python. The caller owns roster admission and R07 renewal;
//! this owner never renews supply or changes shared navigation geometry.
mod angler;
mod boats;
mod exploration;
use crate::{
    gathering::Error,
    progression::Worker,
    resources::{Ledger, PendingOwner, ResourceKey, Stock},
};
pub use angler::{LandAngler, Status as AnglerStatus};
pub use boats::{BoatOutcome, Boats, VoyagePhase};
pub use exploration::{BoatPurchase, ExplorationBoat};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity},
    model::{
        ConstructionState, Destination, NavigationRevision, RouteState, Task, Town, TravelIntent,
        WorkPhase,
    },
    search::{Access, Query, Target, Terrain},
    spatial::{self, AccessProfile, CARDINAL, LandAccess, RoadMode, SpatialIndex},
    travel::Speed,
    units::{Position, Quantity, Seconds, Tile},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};
fn sec(n: f64) -> Seconds {
    Seconds::new(n).expect("bounded fishing clock")
}
fn q(n: f64) -> Result<Quantity, Error> {
    Quantity::new(n).map_err(|_| Error::Invalid)
}
fn fish() -> ResourceKey {
    ResourceKey::parse("fish").unwrap()
}
fn pos(t: Tile) -> Position {
    Position::new(f64::from(t.x), f64::from(t.y)).unwrap()
}
fn pair(t: Tile) -> [i32; 2] {
    [t.x, t.y]
}
fn tile(t: [i32; 2]) -> Tile {
    Tile { x: t[0], y: t[1] }
}
fn distance(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs()
}
fn neighbors(t: Tile) -> impl Iterator<Item = Tile> {
    CARDINAL.into_iter().filter_map(move |(x, y)| {
        Some(Tile {
            x: t.x.checked_add(x)?,
            y: t.y.checked_add(y)?,
        })
    })
}
/// Map-order water facts admitted from the authoritative landscape. Connected
/// water shares its minimum supply, including inaccessible/deep/occupied members.
/// Commit these facts with the ledger after a successful attempt. R07 alone renews.
#[derive(Clone, Debug, PartialEq)]
pub struct Water {
    pub tile: Tile,
    pub supply: Quantity,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Waters {
    water: Vec<Water>,
}
impl Waters {
    pub fn new(water: Vec<Water>) -> Result<Self, Error> {
        let mut seen = BTreeSet::new();
        if water.iter().any(|w| !seen.insert(w.tile)) {
            return Err(Error::Invalid);
        }
        Ok(Self { water })
    }
    pub fn water(&self) -> &[Water] {
        &self.water
    }
    pub fn tiles(&self) -> BTreeSet<Tile> {
        self.water.iter().map(|w| w.tile).collect()
    }
    fn component(&self, source: Tile) -> BTreeSet<Tile> {
        let all = self.tiles();
        if !all.contains(&source) {
            return BTreeSet::new();
        }
        let mut seen = BTreeSet::from([source]);
        let mut queue = VecDeque::from([source]);
        while let Some(t) = queue.pop_front() {
            for p in neighbors(t) {
                if all.contains(&p) && seen.insert(p) {
                    queue.push_back(p);
                }
            }
        }
        seen
    }
    fn take(&mut self, source: Tile, amount: f64) -> f64 {
        let members = self.component(source);
        let available = self
            .water
            .iter()
            .filter(|w| members.contains(&w.tile))
            .map(|w| w.supply.get())
            .reduce(f64::min)
            .unwrap_or(0.);
        let taken = amount.min(available);
        for w in &mut self.water {
            if members.contains(&w.tile) {
                w.supply = q(available - taken).unwrap();
            }
        }
        taken
    }
    /// Shoreline depth 1, cardinal distance inward, independent of occupancy.
    pub fn depths(&self) -> BTreeMap<Tile, u32> {
        let water = self.tiles();
        let mut depths: BTreeMap<_, _> = water
            .iter()
            .filter(|&&t| neighbors(t).any(|p| !water.contains(&p)))
            .map(|&t| (t, 1))
            .collect();
        let mut queue: VecDeque<_> = depths.keys().copied().collect();
        while let Some(t) = queue.pop_front() {
            let d = depths[&t] + 1;
            for p in neighbors(t) {
                if water.contains(&p) && !depths.contains_key(&p) {
                    depths.insert(p, d);
                    queue.push_back(p);
                }
            }
        }
        depths
    }
    /// Same nearest-bank, bounded, cardinal component and depth filtering used by
    /// Python fishing_water. Boat actors using it belong to R17 slice 2.
    pub fn connected(
        &self,
        hut: Tile,
        reach: u32,
        boat_tier: u8,
        mut allowed: impl FnMut(Tile) -> bool,
    ) -> Vec<Tile> {
        let depths = self.depths();
        let tiles: BTreeSet<_> = self
            .water
            .iter()
            .filter(|w| {
                distance(hut, w.tile) <= i64::from(reach)
                    && depths[&w.tile] <= u32::from(boat_tier) + 1
                    && allowed(w.tile)
            })
            .map(|w| w.tile)
            .collect();
        let Some(start) = tiles
            .iter()
            .min_by_key(|&&t| (distance(hut, t), t.y, t.x))
            .copied()
        else {
            return vec![];
        };
        let mut seen = BTreeSet::from([start]);
        let mut result = vec![start];
        let mut i = 0;
        while i < result.len() {
            for p in neighbors(result[i]) {
                if tiles.contains(&p) && seen.insert(p) {
                    result.push(p);
                }
            }
            i += 1;
        }
        result
    }
    /// Land workers choose live water first, then nearest; map order breaks ties.
    /// Land fishing intentionally does not use boat depth/component filtering.
    fn source(&self, hut: Tile, level: u8) -> Option<Tile> {
        self.water
            .iter()
            .filter(|w| distance(hut, w.tile) <= i64::from(level) + 2)
            .min_by_key(|w| (w.supply.get() <= 0., distance(hut, w.tile)))
            .map(|w| w.tile)
    }
}
/// Immutable shared obstacles/terrain plus explicit occupied building/NPC tiles.
/// Water exemptions are private to a pier/query and never edit SpatialIndex.
pub struct Geometry {
    pub index: Rc<SpatialIndex>,
    pub terrain: Terrain,
    pub occupied: BTreeSet<Tile>,
}
impl Geometry {
    fn open(&self, t: Tile, exemptions: &BTreeSet<Tile>) -> bool {
        spatial::search_terrain_open(
            t,
            self.index.size(),
            self.terrain.bounds,
            self.terrain.cells.as_ref().map(|m| m.get(&t).copied()),
        ) && !self.index.occupied_for(
            t,
            &AccessProfile {
                land: LandAccess::Ordinary,
                exemptions,
                only: None,
            },
        )
    }
    fn valid_pier(
        &self,
        pier: &[[i32; 2]],
        hut: Tile,
        source: Tile,
        water: &BTreeSet<Tile>,
    ) -> bool {
        if !water.contains(&source)
            || pier.len() < 2
            || pier.len() > 4
            || tile(pier[0]) != hut
            || tile(*pier.last().unwrap()) != source
        {
            return false;
        }
        let exemptions = water.union(&BTreeSet::from([hut])).copied().collect();
        let mut seen = BTreeSet::new();
        pier.iter().all(|&p| seen.insert(p))
            && pier.windows(2).all(|p| {
                let (a, b) = (tile(p[0]), tile(p[1]));
                !self.occupied.contains(&b)
                    && self
                        .index
                        .edge_open(a, b, false, |t| self.open(t, &exemptions))
            })
    }
    fn pier(&self, hut: Tile, source: Tile, water: &BTreeSet<Tile>) -> Vec<[i32; 2]> {
        if !water.contains(&source) || distance(hut, source) > 3 {
            return vec![];
        }
        let mut exemptions: BTreeSet<_> = water
            .iter()
            .filter(|&&t| distance(hut, t) <= 3)
            .copied()
            .collect();
        exemptions.insert(hut);
        let mut queue = VecDeque::from([vec![pair(hut)]]);
        let mut seen = BTreeSet::from([hut]);
        while let Some(path) = queue.pop_front() {
            let at = tile(*path.last().unwrap());
            if at == source {
                return path;
            }
            if path.len() == 4 {
                continue;
            }
            for next in neighbors(at) {
                if !seen.contains(&next)
                    && !self.occupied.contains(&next)
                    && self
                        .index
                        .edge_open(at, next, false, |t| self.open(t, &exemptions))
                {
                    seen.insert(next);
                    let mut p = path.clone();
                    p.push(pair(next));
                    queue.push_back(p);
                }
            }
        }
        vec![]
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    hut: u64,
    origin: [i32; 2],
    source: [i32; 2],
    pier: Vec<[i32; 2]>,
}
impl Job {
    fn id(&self, town: &Town) -> Result<BuildingId, Error> {
        Ok(BuildingId::from_saved(
            EntityIdentity::from_saved(town.owner().clone(), self.hut)
                .map_err(|_| Error::Invalid)?,
        ))
    }
    fn target(&self) -> Destination {
        Destination::Position(pos(tile(self.source)))
    }
    fn task(&self, town: &Town, phase: WorkPhase, clock: f64) -> Result<Task, Error> {
        Ok(Task::Work {
            workplace: self.id(town)?,
            target: self.target(),
            phase,
            elapsed: sec(clock),
        })
    }
    fn matches(&self, town: &Town, a: &ActorId) -> bool {
        matches!(town.actor(a).map(|a|&a.state().task),Ok(Task::Work { workplace,target,.. }) if self.id(town).is_ok_and(|id| id==*workplace) && *target==self.target())
    }
}
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatchHistory {
    pub last_batch: u64,
    pub attempts: u64,
    pub caught: bool,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fishers {
    jobs: BTreeMap<ActorId, Job>,
    history: BTreeMap<ActorId, CatchHistory>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting,
    Advanced,
    Unloaded(Seconds),
    Cancelled,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Catch {
    Skipped,
    Attempt {
        caught: bool,
        amount: Quantity,
        consumed: Quantity,
    },
}
fn pending(ledger: &Ledger, hut: &BuildingId, a: &ActorId) -> Result<bool, Error> {
    Ok(ledger
        .store(hut)?
        .pending
        .contains_key(&PendingOwner::Worker(a.clone())))
}
impl Fishers {
    fn validate_owner(&self, town: &Town) -> Result<(), Error> {
        for actor in self.jobs.keys().chain(self.history.keys()) {
            actor
                .identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
        }
        Ok(())
    }
    pub fn pier(&self, a: &ActorId) -> Option<Vec<Tile>> {
        self.jobs
            .get(a)
            .map(|j| j.pier.iter().copied().map(tile).collect())
    }
    pub fn source(&self, a: &ActorId) -> Option<Tile> {
        self.jobs.get(a).map(|j| tile(j.source))
    }
    pub fn history(&self, a: &ActorId) -> Option<&CatchHistory> {
        self.history.get(a)
    }
    /// Caller admits a qualified, capacity-checked land worker to a stable hut ID.
    /// Crew workers must use the separate slice-2 owner, never this owner as well.
    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        water: &Waters,
        geometry: &Geometry,
        a: &ActorId,
        hut: &BuildingId,
        worker: &Worker,
        nav: NavigationRevision,
    ) -> Result<Option<Tile>, Error> {
        let (mut owner, mut model, mut goods) = (self.clone(), town.clone(), ledger.clone());
        let result =
            owner.assign_inner(&mut model, &mut goods, water, geometry, a, hut, worker, nav)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn assign_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        water: &Waters,
        g: &Geometry,
        a: &ActorId,
        hut: &BuildingId,
        worker: &Worker,
        nav: NavigationRevision,
    ) -> Result<Option<Tile>, Error> {
        self.validate_owner(town)?;
        town.actor(a)?;
        let b = town.building(hut)?;
        let s = ledger.store(hut)?;
        if worker.job() != "fishing"
            || b.construction != ConstructionState::Complete
            || s.policy.kind != "fishing_hut"
            || s.policy.construction
        {
            return Err(Error::Invalid);
        }
        worker.qualification("fishing")?;
        let origin = b.location;
        let source = water.source(origin, s.policy.level);
        if let Some(j) = self.jobs.get(a).cloned() {
            if j.id(town)? == *hut && j.matches(town, a) {
                let carrying = pending(ledger, hut, a)?;
                if tile(j.origin) == origin && (carrying || source == Some(tile(j.source))) {
                    let deck = water.tiles();
                    if (j.pier.is_empty() && g.pier(origin, tile(j.source), &deck).is_empty())
                        || g.valid_pier(&j.pier, origin, tile(j.source), &deck)
                    {
                        return Ok(Some(tile(j.source)));
                    }
                }
                // Relocation/invalidated deck keeps cargo at the original stable
                // workplace and resumes from the actor's actual saved position.
                let Some(source) = (if carrying {
                    Some(tile(j.source))
                } else {
                    source
                }) else {
                    self.cancel(town, ledger, a)?;
                    return Ok(None);
                };
                let pier = g.pier(origin, source, &water.tiles());
                let job = Job {
                    hut: j.hut,
                    origin: pair(origin),
                    source: pair(source),
                    pier,
                };
                let phase = if carrying {
                    WorkPhase::Returning
                } else {
                    WorkPhase::Outbound
                };
                town.set_task(a, job.task(town, phase, 0.)?)?;
                town.request_route(
                    a,
                    if carrying {
                        Destination::Building(hut.clone())
                    } else {
                        job.target()
                    },
                    nav,
                )?;
                self.jobs.insert(a.clone(), job);
                return Ok(Some(source));
            }
            self.cancel(town, ledger, a)?;
        }
        if town.actor(a)?.state().task != Task::Idle
            || ledger.held().contains_key(a)
            || !town.actor(a)?.state().cargo.goods().is_empty()
        {
            return Err(Error::Busy);
        }
        let Some(source) = source else {
            return Ok(None);
        };
        let job = Job {
            hut: hut.identity().serial(),
            origin: pair(origin),
            source: pair(source),
            pier: g.pier(origin, source, &water.tiles()),
        };
        town.set_task(a, job.task(town, WorkPhase::Outbound, 0.)?)?;
        town.request_route(a, job.target(), nav)?;
        self.jobs.insert(a.clone(), job);
        Ok(Some(source))
    }
    pub fn cancel(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        a: &ActorId,
    ) -> Result<(), Error> {
        self.validate_owner(town)?;
        let Some(j) = self.jobs.get(a) else {
            return Ok(());
        };
        let (mut model, mut goods) = (town.clone(), ledger.clone());
        let id = j.id(town)?;
        if pending(&goods, &id, a)? {
            goods.orphan_pending(&id, a)?;
        }
        if j.matches(town, a) {
            model.set_task(a, Task::Idle)?;
        }
        *town = model;
        *ledger = goods;
        self.jobs.remove(a);
        Ok(())
    }
    /// Query for the R10 service. Deck endpoint is exact; without a deck the
    /// worker fishes from a cardinal bank. Return targets the hut perimeter.
    pub fn query(&self, town: &Town, g: &Geometry, a: &ActorId) -> Result<Query, Error> {
        self.validate_owner(town)?;
        let j = self.jobs.get(a).ok_or(Error::Invalid)?;
        let actor = town.actor(a)?;
        if !j.matches(town, a)
            || town.building(&j.id(town)?)?.location != tile(j.origin)
            || (!j.pier.is_empty()
                && !g.valid_pier(&j.pier, tile(j.origin), tile(j.source), g.index.water()))
        {
            return Err(Error::Invalid);
        }
        let destination = actor.state().travel.destination().ok_or(Error::Invalid)?;
        let mut access = Access::default();
        if !j.pier.is_empty() {
            access.exemptions.extend(j.pier.iter().copied().map(tile));
        }
        let target = if *destination == Destination::Building(j.id(town)?) {
            Target::AnchorAdjacent(town.building(&j.id(town)?)?.location)
        } else if j.pier.is_empty() {
            Target::AnchorAdjacent(tile(j.source))
        } else {
            Target::Exact(tile(j.source))
        };
        let p = actor.state().position;
        let start = Tile {
            x: p.x.get().round_ties_even() as i32,
            y: p.y.get().round_ties_even() as i32,
        };
        Query::new(
            g.index.clone(),
            g.terrain.clone(),
            access,
            start,
            target,
            true,
            RoadMode::Smooth,
        )
        .map_err(|_| Error::Invalid)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        a: &ActorId,
        elapsed: Seconds,
        nav: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut owner, mut model, mut goods) = (self.clone(), town.clone(), ledger.clone());
        let result = owner.advance_inner(&mut model, &mut goods, a, elapsed, nav, &mut segment)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        Ok(result)
    }
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        a: &ActorId,
        elapsed: Seconds,
        nav: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        self.validate_owner(town)?;
        let Some(j) = self.jobs.get(a).cloned() else {
            return Ok(Outcome::Waiting);
        };
        let id = j.id(town)?;
        if !j.matches(town, a)
            || !town.building(&id).is_ok_and(|b| {
                b.location == tile(j.origin) && b.construction == ConstructionState::Complete
            })
            || ledger.store(&id)?.policy.construction
        {
            self.cancel(town, ledger, a)?;
            return Ok(Outcome::Cancelled);
        }
        let Task::Work {
            mut phase,
            elapsed: clock,
            ..
        } = town.actor(a)?.state().task
        else {
            return Err(Error::Invalid);
        };
        let mut clock = clock.get();
        let mut remaining = elapsed.get();
        // Each call stops at unload so its caller can commit the drop-off before
        // spending the remaining time. No completed route/arrival is replayed.
        loop {
            if matches!(phase, WorkPhase::Outbound | WorkPhase::Returning) {
                town.resume_travel(a, nav)?;
                let movement = town.advance_travel(a, sec(remaining), 4096, |x, y| {
                    segment(x, y, Speed::new(1.8).unwrap())
                })?;
                if !matches!(town.actor(a)?.state().travel, TravelIntent::Arrived(_)) {
                    return Ok(Outcome::Waiting);
                }
                remaining = movement.remaining.get();
                phase = if phase == WorkPhase::Outbound {
                    WorkPhase::Working
                } else {
                    WorkPhase::Depositing
                };
                clock = 0.;
                town.set_task(a, j.task(town, phase, clock)?)?;
            }
            let duration = match phase {
                WorkPhase::Working => 3.2,
                WorkPhase::Depositing => 0.65,
                _ => return Err(Error::Invalid),
            };
            let take = remaining.min((duration - clock).max(0.));
            clock += take;
            remaining -= take;
            if clock + 1e-9 < duration {
                town.set_task(a, j.task(town, phase, clock)?)?;
                return Ok(Outcome::Advanced);
            }
            if phase == WorkPhase::Depositing {
                if pending(ledger, &id, a)? {
                    ledger.drop_off(&id, a)?;
                }
                town.set_task(a, j.task(town, WorkPhase::Outbound, 0.)?)?;
                town.request_route(a, j.target(), nav)?;
                return Ok(Outcome::Unloaded(sec(remaining)));
            }
            clock = 0.;
            if pending(ledger, &id, a)? {
                phase = WorkPhase::Returning;
                town.set_task(a, j.task(town, phase, 0.)?)?;
                town.request_route(a, Destination::Building(id.clone()), nav)?;
            } else {
                town.set_task(a, j.task(town, phase, remaining % 3.2)?)?;
                return Ok(Outcome::Advanced);
            }
            if remaining <= 1e-9 {
                return Ok(Outcome::Advanced);
            }
        }
    }
    /// One admitted Python production batch (3 simulation seconds). Invoke after
    /// movement/logistics, once per monotonically increasing batch ID. As Python
    /// does for land workers, a catch can happen during travel; pending/blocked
    /// routes, caps and buffers prevent attempts. Roll is uniform 0..10, 0 fails.
    /// Supply is consumed and practice earned even on a failed roll. No wall time.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        water: &mut Waters,
        a: &ActorId,
        worker: &mut Worker,
        batch: u64,
        roll: u8,
    ) -> Result<Catch, Error> {
        self.validate_owner(town)?;
        if batch == 0 || roll >= 10 {
            return Err(Error::Invalid);
        }
        if self.history.get(a).is_some_and(|h| batch <= h.last_batch) {
            return Ok(Catch::Skipped);
        }
        let Some(j) = self.jobs.get(a) else {
            return Ok(Catch::Skipped);
        };
        let id = j.id(town)?;
        if !j.matches(town, a) || worker.job() != "fishing" {
            return Ok(Catch::Skipped);
        }
        worker.qualification("fishing")?;
        let b = town.building(&id)?;
        let store = ledger.store(&id)?;
        if b.location != tile(j.origin)
            || b.construction != ConstructionState::Complete
            || store.policy.construction
        {
            return Ok(Catch::Skipped);
        }
        // A batch is consumed even if blocked: reopening a route in the same
        // production batch must not mint an extra attempt.
        let (mut owner, mut goods, mut waters, mut person) =
            (self.clone(), ledger.clone(), water.clone(), worker.clone());
        owner.history.entry(a.clone()).or_default().last_batch = batch;
        let room = store
            .local_room("fish")?
            .min(ledger.production_room(&fish())?);
        if room <= 1e-9
            || matches!(
                town.actor(a)?.route(),
                RouteState::Pending(_)
                    | RouteState::Unreachable(_)
                    | RouteState::Blocked(_)
                    | RouteState::Cancelled
            )
        {
            *self = owner;
            return Ok(Catch::Skipped);
        }
        let tool = match worker.tool.as_deref() {
            Some("fine_fishing_rod") => 0.25,
            Some("steel_fishing_rod") => 0.5,
            Some("masterwork_fishing_rod") => 0.75,
            _ => 0.,
        };
        let nets = ["nets", "wool_nets"]
            .iter()
            .filter(|k| store.policy.equipment.contains(**k))
            .count() as f64
            * 0.25;
        let amount = room.min(
            (1. + 0.25 * f64::from(store.policy.level - 1) + tool + nets)
                * person.efficiency("fishing")?,
        );
        let consumed = waters.take(tile(j.source), amount);
        let caught = roll != 0;
        if caught && consumed > 0. {
            goods.produce(
                &id,
                Some(a),
                &Stock::default(),
                &Stock::from_entries([(fish(), q(consumed)?)])?,
            )?;
            goods.adopt_orphan_pending(&id, a)?;
        }
        if consumed > 0. {
            person.practice("fishing", sec(3.))?;
        }
        let h = owner.history.get_mut(a).unwrap();
        h.attempts = h.attempts.checked_add(1).ok_or(Error::Invalid)?;
        h.caught = caught;
        *self = owner;
        *ledger = goods;
        *water = waters;
        *worker = person;
        Ok(Catch::Attempt {
            caught,
            amount: q(if caught { consumed } else { 0. })?,
            consumed: q(consumed)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    jobs: Vec<(u64, Job)>,
    history: Vec<(u64, CatchHistory)>,
}
impl Fishers {
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        for a in self.jobs.keys().chain(self.history.keys()) {
            town.actor(a)?;
        }
        serde_json::to_string(&Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            jobs: self
                .jobs
                .iter()
                .map(|(a, j)| (a.identity().serial(), j.clone()))
                .collect(),
            history: self
                .history
                .iter()
                .map(|(a, h)| (a.identity().serial(), h.clone()))
                .collect(),
        })
        .map_err(|_| Error::Invalid)
    }
    /// Restore alongside model, ledger, worker and R07 supply facts in the same
    /// generation. Paths/searches are rebuilt by R11; no catch runs during load.
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if saved.version != 1
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        let mut result = Self::default();
        let actor = |serial| -> Result<ActorId, Error> {
            Ok(ActorId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), serial)
                    .map_err(|_| Error::Invalid)?,
            ))
        };
        for (serial, h) in saved.history {
            let a = actor(serial)?;
            town.actor(&a)?;
            if h.last_batch == 0
                || h.attempts > h.last_batch
                || (h.attempts == 0 && h.caught)
                || result.history.insert(a, h).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        for (serial, j) in saved.jobs {
            let a = actor(serial)?;
            let id = j.id(town)?;
            let s = ledger.store(&id)?;
            let b = town.building(&id)?;
            let state = town.actor(&a)?.state();
            let Task::Work { phase, elapsed, .. } = state.task else {
                return Err(Error::Invalid);
            };
            let limit = match phase {
                WorkPhase::Outbound | WorkPhase::Returning => 0.,
                WorkPhase::Working => 3.2,
                WorkPhase::Depositing => 0.65,
                _ => return Err(Error::Invalid),
            };
            let expected = match phase {
                WorkPhase::Outbound => Some(j.target()),
                WorkPhase::Returning => Some(Destination::Building(id.clone())),
                _ => None,
            };
            let mut seen = BTreeSet::new();
            let valid_pier = j.pier.is_empty()
                || (j.pier.len() >= 2
                    && j.pier.len() <= 4
                    && j.pier[0] == j.origin
                    && *j.pier.last().unwrap() == j.source
                    && j.pier.iter().all(|&p| seen.insert(p))
                    && j.pier
                        .windows(2)
                        .all(|p| distance(tile(p[0]), tile(p[1])) == 1));
            if s.policy.kind != "fishing_hut"
                || b.location != tile(j.origin)
                || !j.matches(town, &a)
                || elapsed.get() >= limit && limit > 0.
                || elapsed.get() > limit
                || !valid_pier
                || expected.as_ref() != state.travel.destination()
                || (!pending(ledger, &id, &a)?
                    && distance(tile(j.origin), tile(j.source)) > i64::from(s.policy.level) + 2)
                || ledger.held().contains_key(&a)
                || !state.cargo.goods().is_empty()
                || s.pending
                    .get(&PendingOwner::Worker(a.clone()))
                    .is_some_and(|p| p.ordered_entries().any(|(key, _)| key.as_str() != "fish"))
                || result.jobs.insert(a, j).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        Ok(result)
    }
}

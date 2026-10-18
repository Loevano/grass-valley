//! R16 slice 1: offline shared fields and exclusive tenders. Navigation is supplied
//! by R11; growth is called once per town update, never once per farmer. Water trips,
//! harvest return scheduling and ordinary-save adapters belong to later slices.
use crate::{
    progression::{self, Worker},
    resources::{self, Ledger, ResourceKey, Stock},
};
use lantern_core::{
    ids::{ActorId, BuildingId},
    model::{
        ConstructionState, Destination, ModelError, NavigationRevision, Task, Town, TravelIntent,
        WorkPhase,
    },
    travel::Speed,
    units::{Position, Quantity, Seconds},
};
use std::collections::{BTreeMap, BTreeSet};

pub const GROWTH_SECONDS: f64 = 15.;
pub const WORK_SECONDS: f64 = 2.;
pub const MAX_ACRES: usize = 48;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Unplowed,
    Plowed,
    Sprouting,
    Thirsty,
    Growing,
    Ripe,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldState {
    pub stage: Stage,
    pub elapsed: Seconds,
    pub progress: Seconds,
    pub cycle: u64,
    /// Crop locked when sown; changing the selected crop cannot change a harvest.
    pub crop: Option<ResourceKey>,
    pub yield_remaining: Option<Quantity>,
}
impl Default for FieldState {
    fn default() -> Self {
        Self {
            stage: Stage::Unplowed,
            elapsed: secs(0.),
            progress: secs(0.),
            cycle: 0,
            crop: None,
            yield_remaining: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub id: BuildingId,
    /// None for a farm's fallback field or a standalone acre.
    pub farm: Option<BuildingId>,
    pub selected_crop: ResourceKey,
    pub state: FieldState,
}
#[derive(Debug, PartialEq)]
pub enum Error {
    Invalid,
    Busy,
    Model(ModelError),
    Resource(resources::ResourceError),
    Progression(progression::Error),
}
impl From<ModelError> for Error {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl From<resources::ResourceError> for Error {
    fn from(e: resources::ResourceError) -> Self {
        Self::Resource(e)
    }
}
impl From<progression::Error> for Error {
    fn from(e: progression::Error) -> Self {
        Self::Progression(e)
    }
}
fn secs(n: f64) -> Seconds {
    Seconds::new(n).expect("bounded field clock")
}
fn crop_valid(k: &ResourceKey) -> bool {
    resources::public_catalog()["crops"][k.as_str()].is_number()
}
#[derive(Clone, Debug, PartialEq)]
struct Claim {
    workplace: BuildingId,
    field: BuildingId,
    destination: Destination,
    practice_start: Option<f64>,
    practice_seconds: f64,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fields {
    fields: Vec<Field>,
    claims: BTreeMap<ActorId, Claim>,
    previous: BTreeMap<ActorId, Claim>,
    assignments: BTreeMap<ActorId, BuildingId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Plow,
    Sow,
    Harvest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    NoClaim,
    Growth,
    Pass,
    Water,
    Storage,
    Travel,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting(Wait),
    Progress,
    /// Emitted only after the field and resource transaction commits. A partial
    /// harvest reports its actual remaining yield, never a visually emptied acre.
    Completed {
        field: BuildingId,
        action: Action,
        state: FieldState,
        produced: Quantity,
        remaining: Seconds,
    },
}
impl Fields {
    /// Admit the complete farming assignment roster before scheduling workers.
    /// Direct acre assignments reserve priority even before that actor is stepped.
    pub fn new(
        town: &Town,
        ledger: &Ledger,
        fields: Vec<Field>,
        assignments: BTreeMap<ActorId, BuildingId>,
    ) -> Result<Self, Error> {
        let result = Self {
            fields,
            claims: BTreeMap::new(),
            previous: BTreeMap::new(),
            assignments,
        };
        let mut ids = BTreeSet::new();
        for f in &result.fields {
            town.building(&f.id)?;
            let kind = &ledger.store(&f.id)?.policy.kind;
            let s = &f.state;
            if !ids.insert(f.id.clone())
                || !matches!(kind.as_str(), "farm" | "acre")
                || !crop_valid(&f.selected_crop)
                || s.crop.as_ref().is_some_and(|k| !crop_valid(k))
                || s.progress.get() > WORK_SECONDS
                || s.elapsed.get() >= GROWTH_SECONDS
                || (!matches!(s.stage, Stage::Sprouting | Stage::Growing) && s.elapsed.get() != 0.)
                || (matches!(s.stage, Stage::Sprouting | Stage::Growing) && s.progress.get() != 0.)
                || (matches!(
                    s.stage,
                    Stage::Sprouting | Stage::Thirsty | Stage::Growing | Stage::Ripe
                ) && s.crop.is_none())
                || (s.yield_remaining.is_some()
                    && (s.stage != Stage::Ripe || s.progress.get() != WORK_SECONDS))
            {
                return Err(Error::Invalid);
            }
            if let Some(parent) = &f.farm {
                town.building(parent)?;
                if kind != "acre"
                    || parent == &f.id
                    || ledger.store(parent)?.policy.kind != "farm"
                    || !result
                        .fields
                        .iter()
                        .any(|p| &p.id == parent && p.farm.is_none())
                {
                    return Err(Error::Invalid);
                }
            }
            if result
                .fields
                .iter()
                .filter(|a| a.farm.as_ref() == Some(&f.id))
                .count()
                > MAX_ACRES
            {
                return Err(Error::Invalid);
            }
        }
        for (actor, workplace) in &result.assignments {
            town.actor(actor)?;
            result.field(workplace)?;
        }
        Ok(result)
    }
    /// Update the authoritative farming roster without rebuilding field progress
    /// or transferring any pending crop owner. Direct assignment changes revoke
    /// conflicting approaches before the next worker can complete obsolete work.
    pub fn set_assignment(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        workplace: Option<&BuildingId>,
    ) -> Result<(), Error> {
        town.actor(actor)?;
        if let Some(id) = workplace {
            self.field(id)?;
            town.building(id)?;
            if !matches!(ledger.store(id)?.policy.kind.as_str(), "farm" | "acre") {
                return Err(Error::Invalid);
            }
        }
        if self.assignments.get(actor) == workplace {
            return Ok(());
        }
        let (mut owner, mut model) = (self.clone(), town.clone());
        owner.cancel(&mut model, actor)?;
        owner.assignments.remove(actor);
        if let Some(id) = workplace {
            owner.assignments.insert(actor.clone(), id.clone());
            let conflicts: Vec<_> = owner
                .claims
                .iter()
                .filter(|(other, c)| {
                    ledger.store(id).is_ok_and(|s| s.policy.kind == "acre")
                        && &c.field == id
                        && (&c.workplace != id
                            || model.actors().iter().position(|a| &a.state().id == *other)
                                > model.actors().iter().position(|a| &a.state().id == actor))
                })
                .map(|(a, _)| a.clone())
                .collect();
            for other in conflicts {
                owner.cancel(&mut model, &other)?;
            }
        }
        *self = owner;
        *town = model;
        Ok(())
    }
    pub fn field(&self, id: &BuildingId) -> Result<&Field, Error> {
        self.fields
            .iter()
            .find(|f| &f.id == id)
            .ok_or(Error::Invalid)
    }
    fn index(&self, id: &BuildingId) -> Result<usize, Error> {
        self.fields
            .iter()
            .position(|f| &f.id == id)
            .ok_or(Error::Invalid)
    }
    fn complete(town: &Town, ledger: &Ledger, id: &BuildingId) -> bool {
        town.building(id)
            .is_ok_and(|b| b.construction == ConstructionState::Complete)
            && ledger.store(id).is_ok_and(|s| !s.policy.construction)
    }
    fn working_fields(&self, town: &Town, ledger: &Ledger, farm: &BuildingId) -> Vec<BuildingId> {
        let acres: Vec<_> = self
            .fields
            .iter()
            .filter(|f| f.farm.as_ref() == Some(farm) && Self::complete(town, ledger, &f.id))
            .map(|f| f.id.clone())
            .collect();
        if acres.is_empty() {
            vec![farm.clone()]
        } else {
            acres
        }
    }
    pub fn work_pass(
        &self,
        town: &Town,
        ledger: &Ledger,
        farm: &BuildingId,
    ) -> Result<(u64, Stage), Error> {
        let fields = self.working_fields(town, ledger, farm);
        let cycle = fields
            .iter()
            .map(|id| self.field(id).map(|f| f.state.cycle))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(Error::Invalid)?;
        let stages: Vec<_> = fields
            .iter()
            .filter_map(|id| self.field(id).ok())
            .filter(|f| f.state.cycle == cycle)
            .map(|f| f.state.stage)
            .collect();
        let stage = if stages.contains(&Stage::Unplowed) {
            Stage::Unplowed
        } else if stages.contains(&Stage::Plowed) {
            Stage::Plowed
        } else if stages
            .iter()
            .any(|s| matches!(s, Stage::Sprouting | Stage::Thirsty))
        {
            Stage::Thirsty
        } else {
            Stage::Ripe
        };
        Ok((cycle, stage))
    }
    fn can_sow(&self, farm: &BuildingId, cycle: u64) -> bool {
        // Python includes unfinished linked acres in this barrier.
        self.fields
            .iter()
            .filter(|f| f.farm.as_ref() == Some(farm))
            .all(|f| f.state.stage != Stage::Unplowed || f.state.cycle > cycle)
    }
    /// Growth pauses at the next worker barrier and never banks surplus time.
    pub fn grow(&mut self, town: &Town, ledger: &Ledger, elapsed: Seconds) {
        for f in &mut self.fields {
            if Self::complete(town, ledger, &f.id)
                && matches!(f.state.stage, Stage::Sprouting | Stage::Growing)
            {
                let s = &mut f.state;
                s.elapsed =
                    secs((GROWTH_SECONDS - s.elapsed.get()).min(elapsed.get()) + s.elapsed.get());
                if s.elapsed.get() >= GROWTH_SECONDS {
                    s.stage = if s.stage == Stage::Sprouting {
                        Stage::Thirsty
                    } else {
                        Stage::Ripe
                    };
                    s.elapsed = secs(0.);
                    s.progress = secs(0.);
                }
            }
        }
    }
    fn matches(town: &Town, actor: &ActorId, c: &Claim) -> bool {
        matches!(town.actor(actor).map(|a| &a.state().task), Ok(Task::Work { workplace, target, phase: WorkPhase::Outbound | WorkPhase::Working, elapsed }) if workplace == &c.workplace && target == &c.destination && elapsed.get() == 0.)
    }
    pub fn target(&self, actor: &ActorId) -> Option<&BuildingId> {
        self.claims.get(actor).map(|c| &c.field)
    }
    pub fn cancel(&mut self, town: &mut Town, actor: &ActorId) -> Result<(), Error> {
        if let Some(c) = self.claims.get(actor)
            && Self::matches(town, actor, c)
        {
            town.set_task(actor, Task::Idle)?;
        }
        if let Some(c) = self.claims.remove(actor) {
            self.previous.insert(actor.clone(), c);
        }
        // Finished output stays with its existing ledger worker owner.
        Ok(())
    }
    /// Pending routes hold claims. Selection uses separate rows and their nearest
    /// endpoint; an existing active claim remains stable until completed/cancelled.
    pub fn assign(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        workplace: &BuildingId,
        navigation: NavigationRevision,
    ) -> Result<Option<BuildingId>, Error> {
        let (mut owner, mut model) = (self.clone(), town.clone());
        let answer = owner.assign_inner(&mut model, ledger, actor, workplace, navigation)?;
        *self = owner;
        *town = model;
        Ok(answer)
    }
    fn assign_inner(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        workplace: &BuildingId,
        navigation: NavigationRevision,
    ) -> Result<Option<BuildingId>, Error> {
        town.actor(actor)?;
        if self.assignments.get(actor) != Some(workplace) {
            return Err(Error::Invalid);
        }
        self.field(workplace)?;
        if !Self::complete(town, ledger, workplace) {
            return Err(Error::Invalid);
        }
        self.claims
            .retain(|a, c| Self::matches(town, a, c) && Self::complete(town, ledger, &c.field));
        if let Some(c) = self.claims.get(actor) {
            if &c.workplace == workplace {
                return Ok(Some(c.field.clone()));
            }
            self.cancel(town, actor)?;
        }
        if town.actor(actor)?.state().task != Task::Idle
            || ledger.held().contains_key(actor)
            || !town.actor(actor)?.state().cargo.goods().is_empty()
        {
            return Err(Error::Busy);
        }
        let linked = ledger.store(workplace)?.policy.kind == "farm";
        let fields = if linked {
            self.working_fields(town, ledger, workplace)
        } else {
            vec![workplace.clone()]
        };
        let farm = workplace;
        let pass = self.work_pass(town, ledger, farm)?;
        let mut rows: BTreeMap<i32, Vec<BuildingId>> = BTreeMap::new();
        for id in fields {
            let s = &self.field(&id)?.state;
            if (s.cycle, s.stage) != pass
                || (s.stage == Stage::Plowed && !self.can_sow(farm, s.cycle))
                || self.claims.values().any(|c| c.field == id)
                || self.assignments.iter().any(|(other, assigned)| {
                    other != actor
                        && assigned == &id
                        && (workplace != &id
                            || town.actors().iter().position(|a| &a.state().id == other)
                                < town.actors().iter().position(|a| &a.state().id == actor))
                })
            {
                continue;
            }
            rows.entry(town.building(&id)?.location.y)
                .or_default()
                .push(id);
        }
        let occupied: BTreeSet<_> = self
            .claims
            .iter()
            .chain(
                self.previous
                    .iter()
                    .filter(|(a, _)| !self.claims.contains_key(*a)),
            )
            .filter(|(a, c)| {
                *a != actor
                    && &c.workplace == workplace
                    && self.assignments.get(*a) == Some(workplace)
            })
            .map(|(_, c)| c)
            .filter_map(|c| town.building(&c.field).ok().map(|b| b.location.y))
            .collect();
        let pos = town.actor(actor)?.state().position;
        let available: Vec<_> = rows
            .keys()
            .filter(|y| !occupied.contains(y))
            .copied()
            .collect();
        let available = if available.is_empty() {
            rows.keys().copied().collect()
        } else {
            available
        };
        let previous_row = self
            .previous
            .get(actor)
            .filter(|c| &c.workplace == workplace)
            .and_then(|c| town.building(&c.field).ok())
            .map(|b| b.location.y);
        let row = previous_row.filter(|y| available.contains(y)).or_else(|| {
            available.into_iter().min_by(|a, b| {
                (f64::from(*a) - pos.y.get())
                    .abs()
                    .total_cmp(&(f64::from(*b) - pos.y.get()).abs())
                    .then(a.cmp(b))
            })
        });
        let Some(row) = row else {
            return Ok(None);
        };
        let mut ends = rows[&row].clone();
        ends.sort_by_key(|id| town.building(id).unwrap().location.x);
        let id = [ends.first().unwrap(), ends.last().unwrap()]
            .into_iter()
            .min_by(|a, b| {
                let ax = town.building(a).unwrap().location.x;
                let bx = town.building(b).unwrap().location.x;
                (f64::from(ax) - pos.x.get())
                    .abs()
                    .total_cmp(&(f64::from(bx) - pos.x.get()).abs())
                    .then(ax.cmp(&bx))
            })
            .unwrap()
            .clone();
        let tile = town.building(&id)?.location;
        let stage = self.field(&id)?.state.stage;
        let mut x = f64::from(tile.x);
        let mut y = f64::from(tile.y);
        if ledger.store(&id)?.policy.kind == "acre"
            && matches!(stage, Stage::Unplowed | Stage::Plowed | Stage::Thirsty)
        {
            if tile.x > 0 {
                x -= 0.45;
                y += 0.1;
            } else if tile.y > 0 {
                x += 0.1;
                y -= 0.45;
            } else {
                x += 0.45;
                y += 0.1;
            }
        }
        let c = Claim {
            workplace: workplace.clone(),
            field: id.clone(),
            destination: Destination::Position(Position::new(x, y).map_err(|_| Error::Invalid)?),
            practice_start: None,
            practice_seconds: 0.,
        };
        town.set_task(actor, task(&c, WorkPhase::Outbound))?;
        town.request_route(actor, c.destination.clone(), navigation)?;
        self.claims.insert(actor.clone(), c);
        Ok(Some(id))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        navigation: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut owner, mut model, mut goods, mut person) =
            (self.clone(), town.clone(), ledger.clone(), worker.clone());
        let result = owner.advance_inner(
            &mut model,
            &mut goods,
            actor,
            &mut person,
            elapsed,
            navigation,
            &mut segment,
        )?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *worker = person;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let Some(c) = self.claims.get(actor).cloned() else {
            return Ok(Outcome::Waiting(Wait::NoClaim));
        };
        if !Self::matches(town, actor, &c)
            || !Self::complete(town, ledger, &c.field)
            || !Self::complete(town, ledger, &c.workplace)
            || worker.job() != "farming"
        {
            self.cancel(town, actor)?;
            return Ok(Outcome::Waiting(Wait::Cancelled));
        }
        worker.qualification("farming")?;
        let i = self.index(&c.field)?;
        let farm = &c.workplace;
        let s = &self.fields[i].state;
        if matches!(s.stage, Stage::Sprouting | Stage::Growing) {
            return Ok(Outcome::Waiting(Wait::Growth));
        }
        if (s.cycle, s.stage) != self.work_pass(town, ledger, farm)?
            || (s.stage == Stage::Plowed && !self.can_sow(farm, s.cycle))
        {
            return Ok(Outcome::Waiting(Wait::Pass));
        }
        if s.stage == Stage::Thirsty {
            return Ok(Outcome::Waiting(Wait::Water));
        }
        let crop = if s.stage == Stage::Plowed {
            self.fields[i].selected_crop.clone()
        } else {
            s.crop
                .clone()
                .unwrap_or_else(|| self.fields[i].selected_crop.clone())
        };
        let room = ledger
            .store(&c.workplace)?
            .local_room(crop.as_str())?
            .min(ledger.production_room(&crop)?);
        if s.stage == Stage::Ripe && room <= 1e-9 {
            return Ok(Outcome::Waiting(Wait::Storage));
        }
        let mut remaining = elapsed.get();
        if matches!(
            town.actor(actor)?.state().task,
            Task::Work {
                phase: WorkPhase::Outbound,
                ..
            }
        ) {
            town.resume_travel(actor, navigation)?;
            let movement = town.advance_travel(actor, elapsed, 4096, |a, b| {
                segment(a, b, Speed::new(1.8).unwrap())
            })?;
            if town.actor(actor)?.state().travel != TravelIntent::Arrived(c.destination.clone()) {
                return Ok(Outcome::Waiting(Wait::Travel));
            }
            remaining = movement.remaining.get();
            town.set_task(actor, task(&c, WorkPhase::Working))?;
        }
        let s = &mut self.fields[i].state;
        let take = remaining.min((WORK_SECONDS - s.progress.get()).max(0.));
        s.progress = secs(s.progress.get() + take);
        remaining -= take;
        if take > 0. {
            // Round the cumulative action contribution, so caller subdivisions
            // cannot move a harvest across a 20-point efficiency threshold.
            let claim = self.claims.get_mut(actor).ok_or(Error::Invalid)?;
            let start = *claim
                .practice_start
                .get_or_insert(worker.practice_value("farming"));
            claim.practice_seconds += take;
            worker.set_practice(
                "farming",
                lantern_core::units::Practice::new(start).map_err(|_| Error::Invalid)?,
            )?;
            worker.practice("farming", secs(claim.practice_seconds))?;
        }
        if s.progress.get() + 1e-9 < WORK_SECONDS {
            return Ok(Outcome::Progress);
        }
        s.progress = secs(WORK_SECONDS);
        let mut produced = 0.;
        let action = match s.stage {
            Stage::Unplowed => {
                s.stage = Stage::Plowed;
                s.progress = secs(0.);
                Action::Plow
            }
            Stage::Plowed => {
                s.stage = Stage::Sprouting;
                s.crop = Some(crop);
                s.elapsed = secs(0.);
                s.progress = secs(0.);
                Action::Sow
            }
            Stage::Ripe => {
                let yield_left = if let Some(n) = s.yield_remaining {
                    n.get()
                } else {
                    let level = ledger.store(&c.workplace)?.policy.level;
                    let upgraded_storage = resources::public_catalog()["storage_upgrade_kinds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| {
                            v.as_str()
                                == Some(ledger.store(&c.workplace).unwrap().policy.kind.as_str())
                        });
                    let bonus =
                        if upgraded_storage || ledger.store(&c.workplace)?.policy.kind == "acre" {
                            0.
                        } else {
                            0.25 * f64::from(level - 1)
                        };
                    let tool = match worker.tool.as_deref() {
                        Some("fine_sickle") => 0.25,
                        Some("steel_sickle") => 0.5,
                        Some("masterwork_sickle") => 0.75,
                        _ => 0.,
                    };
                    let mut amount = resources::public_catalog()["farming_harvest_yields"]
                        [crop.as_str()]
                    .as_f64()
                    .ok_or(Error::Invalid)?
                        * (1. + bonus + tool)
                        * worker.efficiency("farming")?;
                    let manure = ResourceKey::parse("manure")?;
                    if ledger.free(&manure)?.get() >= 0.1 {
                        ledger.spend(&Stock::from_entries([(
                            manure,
                            Quantity::new(0.1).unwrap(),
                        )])?)?;
                        amount *= 1.25;
                    }
                    Quantity::new(amount).map_err(|_| Error::Invalid)?.get()
                };
                produced = room.min(yield_left);
                ledger.produce(
                    &c.workplace,
                    Some(actor),
                    &Stock::default(),
                    &Stock::from_entries([(
                        crop,
                        Quantity::new(produced).map_err(|_| Error::Invalid)?,
                    )])?,
                )?;
                if yield_left - produced <= 1e-9 {
                    *s = FieldState {
                        cycle: s.cycle.checked_add(1).ok_or(Error::Invalid)?,
                        ..FieldState::default()
                    };
                } else {
                    s.yield_remaining =
                        Some(Quantity::new(yield_left - produced).map_err(|_| Error::Invalid)?);
                }
                Action::Harvest
            }
            _ => return Err(Error::Invalid),
        };
        let state = self.fields[i].state.clone();
        if state.yield_remaining.is_none() {
            self.cancel(town, actor)?;
        }
        Ok(Outcome::Completed {
            field: c.field,
            action,
            state,
            produced: Quantity::new(produced).map_err(|_| Error::Invalid)?,
            remaining: secs(remaining),
        })
    }
}
fn task(c: &Claim, phase: WorkPhase) -> Task {
    Task::Work {
        workplace: c.workplace.clone(),
        target: c.destination.clone(),
        phase,
        elapsed: secs(0.),
    }
}

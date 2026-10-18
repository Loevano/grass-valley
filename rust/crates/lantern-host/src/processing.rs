//! Experimental processor actors and owned workshop orders. Town owns movement;
//! Ledger owns every input reservation and finished good. No live host or terrain
//! renewal clock.
use crate::logistics::{self, LogisticsTrips};
use crate::{
    progression::{self, Batch, Worker, Workplace},
    resources::{self, Ledger, PendingOwner, Stock},
};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity},
    model::{
        ConstructionState, Destination, ModelError, NavigationRevision, Task, Town, TravelIntent,
        WorkPhase,
    },
    travel::Speed,
    units::{Position, Quantity, Seconds, Tile},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod workshop;
pub use workshop::WorkshopOrder;

#[derive(Debug, PartialEq)]
pub enum Error {
    Model(ModelError),
    Resource(resources::ResourceError),
    Progression(progression::Error),
    Invalid,
    Busy,
    Unsupported,
    TripsRequired,
    QueueFull,
    MissingOrder,
    Exhausted,
    Logistics(logistics::Error),
}
impl From<logistics::Error> for Error {
    fn from(e: logistics::Error) -> Self {
        Self::Logistics(e)
    }
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
fn sec(n: f64) -> Seconds {
    Seconds::new(n).expect("bounded timer")
}
fn kind_supported(kind: &str) -> bool {
    matches!(
        kind,
        "tannery"
            | "mine"
            | "smelter"
            | "saddler"
            | "steelworks"
            | "tradehouse"
            | "bakery"
            | "butcher"
            | "sawpit"
            | "mill"
            | "smokehouse"
            | "fish_pond"
            | "workshop"
    )
}
fn recipe_supported(workplace: &Workplace) -> Result<progression::Recipe, Error> {
    if !kind_supported(&workplace.kind) {
        return Err(Error::Unsupported);
    }
    Ok(workplace.selected_recipe()?)
}
fn serial_building(town: &Town, serial: u64) -> Result<BuildingId, Error> {
    Ok(BuildingId::from_saved(
        EntityIdentity::from_saved(town.owner().clone(), serial).map_err(|_| Error::Invalid)?,
    ))
}
fn distance(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs()
}

/// Iron-only facts admitted from R07 in authoritative resource order. A depleted
/// vein stays present, with zero supply, for the existing terrain renewal owner.
#[derive(Clone, Debug, PartialEq)]
pub struct IronVein {
    pub tile: Tile,
    pub supply: Quantity,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IronDeposits {
    veins: Vec<IronVein>,
}
impl IronDeposits {
    pub fn new(veins: Vec<IronVein>) -> Result<Self, Error> {
        let mut seen = BTreeSet::new();
        if veins.iter().any(|v| !seen.insert(v.tile)) {
            return Err(Error::Invalid);
        }
        Ok(Self { veins })
    }
    pub fn veins(&self) -> &[IronVein] {
        &self.veins
    }
    fn richest(&self, camp: Tile) -> Option<usize> {
        self.veins
            .iter()
            .enumerate()
            .filter(|(_, v)| distance(camp, v.tile) <= 10)
            .max_by(|(i, a), (j, b)| {
                a.supply
                    .get()
                    .total_cmp(&b.supply.get())
                    .then_with(|| j.cmp(i))
            })
            .map(|(i, _)| i)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    building: u64,
    x: i32,
    y: i32,
    kind: String,
    output: String,
    elapsed: f64,
    active: bool,
}
impl Job {
    fn id(&self, town: &Town) -> Result<BuildingId, Error> {
        serial_building(town, self.building)
    }
    fn available(&self, town: &Town, ledger: &Ledger) -> bool {
        self.id(town).is_ok_and(|id| {
            town.building(&id).is_ok_and(|b| {
                b.location
                    == Tile {
                        x: self.x,
                        y: self.y,
                    }
                    && b.construction == ConstructionState::Complete
            }) && ledger
                .store(&id)
                .is_ok_and(|s| s.policy.kind == self.kind && !s.policy.construction)
        })
    }
    fn matches(&self, town: &Town, actor: &ActorId) -> bool {
        matches!(
            town.actor(actor).map(|a| &a.state().task),
            Ok(Task::Work { workplace, target, .. })
                if self.id(town).is_ok_and(|id| *workplace == id
                    && *target == Destination::Building(id))
        )
    }
    fn task(&self, town: &Town, phase: WorkPhase, elapsed: f64) -> Result<Task, Error> {
        let id = self.id(town)?;
        Ok(Task::Work {
            workplace: id.clone(),
            target: Destination::Building(id),
            phase,
            elapsed: sec(elapsed),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialWork {
    output: String,
    elapsed: f64,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Processors {
    jobs: BTreeMap<ActorId, Job>,
    grain: BTreeMap<BuildingId, Vec<PartialWork>>,
    workshops: workshop::Workshops,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    Travel,
    Inputs,
    Capacity,
    Cap,
    IronVein,
    Ore,
    Qualification,
    Requests,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting(Wait),
    Paused,
    Cancelled,
    Working {
        elapsed: Seconds,
        duration: Seconds,
    },
    /// Exactly one batch per invocation, matching Python; surplus work time is
    /// discarded rather than banked or converted into extra batches.
    Completed(Batch),
    Unloading,
    /// Delivery consumes only its remaining 0.65-second interval. Continue with
    /// this remainder after the authoritative drop-off, without a tick-sized gap.
    Unloaded(Seconds),
}
impl Processors {
    fn check_owner(&self, town: &Town) -> Result<(), Error> {
        if self
            .jobs
            .keys()
            .any(|a| a.identity().validate_owner(town.owner()).is_err())
            || self
                .grain
                .keys()
                .any(|b| b.identity().validate_owner(town.owner()).is_err())
        {
            return Err(Error::Invalid);
        }
        self.workshops.check_owner(town)?;
        Ok(())
    }
    pub fn elapsed(&self, actor: &ActorId) -> Option<Seconds> {
        self.jobs.get(actor).map(|j| sec(j.elapsed))
    }
    /// Attach or resume at the current position. R14 supply/output trips may
    /// suspend a processor without discarding its partial batch.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        id: &BuildingId,
        worker: &Worker,
        workplace: &Workplace,
        navigation: NavigationRevision,
    ) -> Result<(), Error> {
        let (mut owner, mut model, mut goods) = (self.clone(), town.clone(), ledger.clone());
        self.check_owner(town)?;
        let effective = self.workplace(id, workplace)?;
        let workplace = &effective;
        let recipe = recipe_supported(workplace)?;
        worker.qualification(worker.job())?;
        if workplace.kind != "workshop" || workplace.request.is_some() {
            workplace.recipe_requirement(worker)?;
        }
        if workplace.job()? != worker.job() {
            return Err(Error::Invalid);
        }
        let b = model.building(id)?;
        let store = goods.store(id)?;
        if b.construction != ConstructionState::Complete
            || store.policy.construction
            || store.policy.kind != workplace.kind
            || store.policy.equipment != workplace.equipment
        {
            return Err(Error::Invalid);
        }
        let endpoint = (b.location.x, b.location.y);
        if let Some(job) = owner.jobs.get(actor) {
            if job.building != id.identity().serial()
                || job.kind != workplace.kind
                || !job.available(town, ledger)
            {
                owner.cancel(&mut model, &mut goods, actor)?;
            } else if job.matches(&model, actor) {
                return Ok(());
            }
        }
        if model.actor(actor)?.state().task != Task::Idle
            || !model.actor(actor)?.state().cargo.goods().is_empty()
            || goods.held().contains_key(actor)
        {
            return Err(Error::Busy);
        }
        let job = owner.jobs.entry(actor.clone()).or_insert(Job {
            building: id.identity().serial(),
            x: endpoint.0,
            y: endpoint.1,
            kind: workplace.kind.clone(),
            output: recipe.output.as_str().into(),
            elapsed: 0.,
            active: false,
        });
        if job.output != recipe.output.as_str() {
            job.output = recipe.output.as_str().into();
            job.elapsed = 0.;
            job.active = false;
        }
        model.set_task(actor, job.task(&model, WorkPhase::Outbound, 0.)?)?;
        model.request_route(actor, Destination::Building(id.clone()), navigation)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        Ok(())
    }
    /// This is an explicit delivery event, independent of the recipe duration.
    /// The host's work-cycle owner chooses when to unload completed goods.
    pub fn begin_unload(&self, town: &mut Town, actor: &ActorId) -> Result<(), Error> {
        let job = self.jobs.get(actor).ok_or(Error::Invalid)?;
        if !job.matches(town, actor)
            || !matches!(
                town.actor(actor)?.state().task,
                Task::Work {
                    phase: WorkPhase::Working,
                    ..
                }
            )
        {
            return Err(Error::Busy);
        }
        town.set_task(actor, job.task(town, WorkPhase::Depositing, 0.)?)?;
        Ok(())
    }
    pub fn cancel(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
    ) -> Result<(), Error> {
        let Some(job) = self.jobs.get(actor) else {
            return Ok(());
        };
        let id = job.id(town)?;
        let (mut model, mut goods) = (town.clone(), ledger.clone());
        if goods
            .store(&id)?
            .pending
            .contains_key(&PendingOwner::Worker(actor.clone()))
        {
            goods.orphan_pending(&id, actor)?;
        }
        if job.matches(town, actor) {
            model.set_task(actor, Task::Idle)?;
        }
        *town = model;
        *ledger = goods;
        self.jobs.remove(actor);
        Ok(())
    }
    /// Admit legacy mill/bakery partial work without minting goods or practice.
    /// Entries are claimed in saved order, only after a full batch is admissible.
    pub fn queue_grain_work(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        id: &BuildingId,
        elapsed: Seconds,
    ) -> Result<(), Error> {
        self.check_owner(town)?;
        town.building(id)?;
        let kind = &ledger.store(id)?.policy.kind;
        if !matches!(kind.as_str(), "mill" | "bakery") {
            return Err(Error::Invalid);
        }
        let recipe = Workplace::new(kind).selected_recipe()?;
        if elapsed.get() > recipe.seconds().get() {
            return Err(Error::Invalid);
        }
        self.grain.entry(id.clone()).or_default().push(PartialWork {
            output: recipe.output.as_str().into(),
            elapsed: elapsed.get(),
        });
        Ok(())
    }
    /// Command-side cleanup, also preparing later advanced-recipe execution.
    /// Selection reuses R13 qualification; obsolete inputs return to output.
    /// Existing supply trips and grain queues retain Python's set_recipe policy.
    #[allow(clippy::too_many_arguments)]
    pub fn change_recipe(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        id: &BuildingId,
        workplace: &mut Workplace,
        output: &str,
        sandbox: bool,
        assigned: &[Worker],
    ) -> Result<(), Error> {
        self.check_owner(town)?;
        if !kind_supported(&workplace.kind)
            || workplace.kind == "workshop"
            || town.building(id)?.construction != ConstructionState::Complete
            || ledger.store(id)?.policy.construction
            || ledger.store(id)?.policy.kind != workplace.kind
        {
            return Err(Error::Invalid);
        }
        let recipe = progression::choose_recipe(&workplace.kind, output, sandbox, assigned)?;
        let (mut owner, mut goods) = (self.clone(), ledger.clone());
        let obsolete = Stock::from_entries(
            goods
                .store(id)?
                .inputs
                .ordered_entries()
                .filter(|(k, _)| !recipe.inputs.entries().contains_key(*k))
                .map(|(k, n)| (k.clone(), *n)),
        )?;
        goods.release(id, &obsolete)?;
        for job in owner
            .jobs
            .values_mut()
            .filter(|j| j.building == id.identity().serial())
        {
            job.elapsed = 0.;
            job.active = false;
            job.output = output.into();
        }
        *self = owner;
        *ledger = goods;
        workplace.recipe = Some(output.into());
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        ore: &mut IronDeposits,
        actor: &ActorId,
        worker: &mut Worker,
        workplace: &Workplace,
        elapsed: Seconds,
        navigation: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut owner, mut model, mut goods, mut deposits, mut person) = (
            self.clone(),
            town.clone(),
            ledger.clone(),
            ore.clone(),
            worker.clone(),
        );
        let result = owner.advance_inner(
            &mut model,
            &mut goods,
            &mut deposits,
            actor,
            &mut person,
            workplace,
            elapsed,
            navigation,
            &mut segment,
            None,
        )?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *ore = deposits;
        *worker = person;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        ore: &mut IronDeposits,
        actor: &ActorId,
        worker: &mut Worker,
        workplace: &Workplace,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
        trips: Option<(&mut LogisticsTrips, &BuildingId)>,
    ) -> Result<Outcome, Error> {
        let Some(job) = self.jobs.get(actor).cloned() else {
            return Ok(Outcome::Paused);
        };
        if workplace.kind == "workshop" && trips.is_none() {
            return Err(Error::TripsRequired);
        }
        let effective = self.workplace(&job.id(town)?, workplace)?;
        let workplace = &effective;
        if !job.available(town, ledger)
            || workplace.kind != job.kind
            || workplace.job()? != worker.job()
        {
            self.cancel(town, ledger, actor)?;
            return Ok(Outcome::Cancelled);
        }
        if matches!(town.actor(actor)?.state().task, Task::Transport { .. }) {
            return Ok(Outcome::Paused);
        }
        if !job.matches(town, actor) {
            self.cancel(town, ledger, actor)?;
            return Ok(Outcome::Cancelled);
        }
        let id = job.id(town)?;
        if ledger.store(&id)?.policy.equipment != workplace.equipment {
            return Err(Error::Invalid);
        }
        let Task::Work {
            phase,
            elapsed: phase_elapsed,
            ..
        } = town.actor(actor)?.state().task
        else {
            return Err(Error::Invalid);
        };
        let mut remaining = elapsed.get();
        if phase == WorkPhase::Depositing {
            let take = remaining.min((0.65 - phase_elapsed.get()).max(0.));
            let clock = phase_elapsed.get() + take;
            if clock + 1e-9 < 0.65 {
                town.set_task(actor, job.task(town, phase, clock)?)?;
                return Ok(Outcome::Unloading);
            }
            if ledger
                .store(&id)?
                .pending
                .contains_key(&PendingOwner::Worker(actor.clone()))
            {
                ledger.drop_off(&id, actor)?;
            }
            town.set_task(actor, job.task(town, WorkPhase::Working, 0.)?)?;
            return Ok(Outcome::Unloaded(sec(remaining - take)));
        }
        if phase == WorkPhase::Outbound {
            town.resume_travel(actor, navigation)?;
            let speed = Speed::new(1.8).unwrap();
            let movement =
                town.advance_travel(actor, elapsed, 4096, |a, b| segment(a, b, speed))?;
            if town.actor(actor)?.state().travel
                != TravelIntent::Arrived(Destination::Building(id.clone()))
            {
                return Ok(Outcome::Waiting(Wait::Travel));
            }
            remaining = movement.remaining.get();
            town.set_task(actor, job.task(town, WorkPhase::Working, 0.)?)?;
        } else if phase != WorkPhase::Working
            || matches!(town.actor(actor)?.state().travel, TravelIntent::Active(_))
        {
            return Ok(Outcome::Waiting(Wait::Travel));
        }
        if workplace.kind == "workshop" && workplace.request.is_none() {
            return Ok(Outcome::Waiting(Wait::Requests));
        }
        let recipe = recipe_supported(workplace)?;
        if worker.qualification(worker.job()).is_err()
            || workplace.recipe_requirement(worker).is_err()
        {
            return Ok(Outcome::Waiting(Wait::Qualification));
        }
        let batch = progression::evaluate_batch(worker, workplace, &ledger.store(&id)?.inputs)?;
        // Probe the real ledger transaction before touching partial work; this
        // keeps multi-resource storage, epsilon spending and cap rules identical.
        let mut probe = ledger.clone();
        match probe.produce(&id, Some(actor), &batch.costs, &batch.outputs) {
            Ok(()) => {}
            Err(resources::ResourceError::InsufficientStock) => {
                return Ok(Outcome::Waiting(Wait::Inputs));
            }
            Err(resources::ResourceError::Capacity) => return Ok(Outcome::Waiting(Wait::Capacity)),
            Err(resources::ResourceError::ProductionCap) => return Ok(Outcome::Waiting(Wait::Cap)),
            Err(e) => return Err(e.into()),
        }
        let vein = if job.kind == "mine" {
            let Some(i) = ore.richest(Tile { x: job.x, y: job.y }) else {
                return Ok(Outcome::Waiting(Wait::IronVein));
            };
            let need = batch
                .outputs
                .quantity(&resources::ResourceKey::parse("iron_ore")?)
                .get();
            if ore.veins[i].supply.get() + 1e-9 < need {
                return Ok(Outcome::Waiting(Wait::Ore));
            }
            Some((i, need))
        } else {
            None
        };
        let progress = self.jobs.get_mut(actor).unwrap();
        if progress.output != recipe.output.as_str() {
            progress.output = recipe.output.as_str().into();
            progress.elapsed = 0.;
            progress.active = false;
        }
        if !progress.active
            && let Some(queue) = self.grain.get_mut(&id)
            && let Some(i) = queue.iter().position(|p| p.output == progress.output)
        {
            progress.elapsed = queue.remove(i).elapsed;
        }
        progress.active = true;
        // Avoid overflow on a valid, very large elapsed interval.
        progress.elapsed += remaining.min((batch.seconds.get() - progress.elapsed).max(0.));
        if progress.elapsed < batch.seconds.get() {
            return Ok(Outcome::Working {
                elapsed: sec(progress.elapsed),
                duration: batch.seconds,
            });
        }
        let mut trained = worker.clone();
        trained.practice(worker.job(), batch.seconds)?;
        ledger.produce(&id, Some(actor), &batch.costs, &batch.outputs)?;
        ledger.adopt_orphan_pending(&id, actor)?;
        if let Some((i, need)) = vein {
            ore.veins[i].supply = Quantity::new((ore.veins[i].supply.get() - need).max(0.))
                .map_err(|_| Error::Invalid)?;
        }
        *worker = trained;
        progress.elapsed = 0.;
        progress.active = false;
        if workplace.kind == "workshop" {
            let queue = self.workshops.queues.get_mut(&id).ok_or(Error::Invalid)?;
            let head = queue.first_mut().ok_or(Error::Invalid)?;
            let amount: f64 = batch.outputs.entries().values().map(|n| n.get()).sum();
            head.remaining = (head.remaining - amount).max(0.);
            if head.remaining <= 1e-9 {
                queue.remove(0);
                let (trips, hall) = trips.ok_or(Error::TripsRequired)?;
                self.workshop_head_changed(town, ledger, trips, hall, &id)?;
            }
        }
        Ok(Outcome::Completed(batch))
    }
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        self.check_owner(town)?;
        serde_json::to_string(&Saved {
            version: 2,
            workshops: Some(self.workshops.save()),
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            jobs: self
                .jobs
                .iter()
                .map(|(a, j)| (a.identity().serial(), j.clone()))
                .collect(),
            grain: self
                .grain
                .iter()
                .map(|(b, q)| (b.identity().serial(), q.clone()))
                .collect(),
        })
        .map_err(|_| Error::Invalid)
    }
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if !matches!(saved.version, 1 | 2)
            || (saved.version == 1 && saved.workshops.is_some())
            || (saved.version == 2 && saved.workshops.is_none())
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        let mut owner = Self::default();
        if let Some(workshops) = saved.workshops {
            owner.workshops = workshop::Workshops::decode(workshops, town, ledger)?;
        }
        for (serial, job) in saved.jobs {
            let actor = ActorId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), serial)
                    .map_err(|_| Error::Invalid)?,
            );
            let state = town.actor(&actor)?.state();
            let valid_phase = match state.task {
                Task::Work { phase, elapsed, .. } if job.matches(town, &actor) => match phase {
                    WorkPhase::Outbound | WorkPhase::Working => elapsed.get() == 0.,
                    WorkPhase::Depositing => elapsed.get() <= 0.65,
                    _ => false,
                },
                // R14 trips and stale assignments are restored for resume or
                // cancellation, preserving their current authoritative task.
                _ => true,
            };
            if !kind_supported(&job.kind)
                || (saved.version == 1 && job.kind == "workshop")
                || (job.kind == "workshop"
                    && job.active
                    && owner
                        .workshops
                        .head(&job.id(town)?)
                        .is_none_or(|h| h.recipe != job.output))
                || ledger.store(&job.id(town)?)?.policy.kind != job.kind
                || (!job.active && job.elapsed != 0.)
                || !valid_partial(&job.kind, &job.output, job.elapsed)
                || !state.cargo.goods().is_empty()
                || !valid_phase
                || owner.jobs.insert(actor, job).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        for (serial, queue) in saved.grain {
            let id = serial_building(town, serial)?;
            let kind = &ledger.store(&id)?.policy.kind;
            if !matches!(kind.as_str(), "mill" | "bakery")
                || queue
                    .iter()
                    .any(|p| !valid_partial(kind, &p.output, p.elapsed))
                || owner.grain.insert(id, queue).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        Ok(owner)
    }
}
fn valid_partial(kind: &str, output: &str, elapsed: f64) -> bool {
    let mut workplace = Workplace::new(kind);
    workplace.recipe = Some(output.into());
    workplace.selected_recipe().is_ok_and(|r| {
        r.output.as_str() == output
            && elapsed.is_finite()
            && elapsed >= 0.
            && elapsed <= r.seconds().get()
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    #[serde(default)]
    workshops: Option<workshop::SavedWorkshops>,
    world: String,
    town: String,
    jobs: Vec<(u64, Job)>,
    grain: Vec<(u64, Vec<PartialWork>)>,
}

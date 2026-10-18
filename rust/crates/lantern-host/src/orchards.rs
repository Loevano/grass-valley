//! Experimental authoritative fruit trees and single-pick return trips.
mod save;
use crate::{
    farming::Error,
    progression::Worker,
    resources::{Ledger, PendingOwner, ResourceKey, Stock},
};
use lantern_core::{
    ids::{ActorId, BuildingId},
    model::{
        ConstructionState, Destination, NavigationRevision, Task, Town, TravelIntent, WorkPhase,
    },
    travel::Speed,
    units::{Position, Quantity, Seconds},
};
use std::collections::{BTreeMap, BTreeSet};
pub const HARVEST_YIELD: f64 = 8.;
pub const PICK_SECONDS: f64 = 3.2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Sapling,
    Blossom,
    Fruit,
    Ripe,
    Harvested,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub id: BuildingId,
    pub orchard: BuildingId,
    pub stage: Stage,
    pub elapsed: Seconds,
    pub fruit: Quantity,
}
#[derive(Clone, Debug, PartialEq)]
struct Job {
    orchard: BuildingId,
    tree: BuildingId,
    target: Destination,
    clock: f64,
    practice_start: Option<f64>,
    practice_seconds: f64,
    duration: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Orchards {
    trees: Vec<Tree>,
    jobs: BTreeMap<ActorId, Job>,
    assignments: BTreeMap<ActorId, BuildingId>,
    previous: BTreeMap<ActorId, (BuildingId, BuildingId)>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting,
    Progress,
    Picked {
        tree: BuildingId,
        amount: Quantity,
        remaining: Seconds,
    },
    Unloaded {
        remaining: Seconds,
    },
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).expect("bounded orchard clock")
}
fn fruit() -> ResourceKey {
    ResourceKey::parse("fruit").unwrap()
}
fn complete(town: &Town, ledger: &Ledger, id: &BuildingId) -> bool {
    town.building(id)
        .is_ok_and(|b| b.construction == ConstructionState::Complete)
        && ledger.store(id).is_ok_and(|s| !s.policy.construction)
}
fn task(j: &Job, phase: WorkPhase) -> Task {
    Task::Work {
        workplace: j.orchard.clone(),
        target: j.target.clone(),
        phase,
        elapsed: sec(0.),
    }
}
fn phase(town: &Town, actor: &ActorId) -> Result<WorkPhase, Error> {
    match town.actor(actor)?.state().task {
        Task::Work { phase, .. } => Ok(phase),
        _ => Err(Error::Invalid),
    }
}
fn pending(ledger: &Ledger, id: &BuildingId, actor: &ActorId) -> Result<bool, Error> {
    Ok(ledger
        .store(id)?
        .pending
        .contains_key(&PendingOwner::Worker(actor.clone())))
}
impl Orchards {
    pub fn new(
        town: &Town,
        ledger: &Ledger,
        trees: Vec<Tree>,
        assignments: BTreeMap<ActorId, BuildingId>,
    ) -> Result<Self, Error> {
        let mut seen = BTreeSet::new();
        for t in &trees {
            let b = town.building(&t.id)?;
            let o = town.building(&t.orchard)?;
            let duration = duration(t.stage);
            if !seen.insert(t.id.clone())
                || ledger.store(&t.id)?.policy.kind != "orchard_tree"
                || ledger.store(&t.orchard)?.policy.kind != "orchard"
                || (i64::from(b.location.x) - i64::from(o.location.x))
                    .abs()
                    .max((i64::from(b.location.y) - i64::from(o.location.y)).abs())
                    > 3
                || t.fruit.get() > 8.
                || duration.is_some_and(|d| t.elapsed.get() >= d)
                || (duration.is_none() && t.elapsed.get() != 0.)
                || (t.stage == Stage::Harvested && t.fruit.get() > 1e-9)
            {
                return Err(Error::Invalid);
            }
        }
        for (a, id) in &assignments {
            town.actor(a)?;
            town.building(id)?;
            if ledger.store(id)?.policy.kind != "orchard" {
                return Err(Error::Invalid);
            }
        }
        Ok(Self {
            trees,
            jobs: BTreeMap::new(),
            assignments,
            previous: BTreeMap::new(),
        })
    }
    pub fn set_assignment(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        orchard: Option<&BuildingId>,
    ) -> Result<(), Error> {
        town.actor(actor)?;
        if let Some(id) = orchard {
            town.building(id)?;
            if ledger.store(id)?.policy.kind != "orchard" {
                return Err(Error::Invalid);
            }
        }
        if self.assignments.get(actor) == orchard {
            return Ok(());
        }
        let (mut owner, mut model) = (self.clone(), town.clone());
        owner.cancel(&mut model, actor)?;
        owner.assignments.remove(actor);
        if let Some(id) = orchard {
            owner.assignments.insert(actor.clone(), id.clone());
        }
        *self = owner;
        *town = model;
        Ok(())
    }
    pub fn tree(&self, id: &BuildingId) -> Option<&Tree> {
        self.trees.iter().find(|t| &t.id == id)
    }
    pub fn grow(&mut self, town: &Town, ledger: &Ledger, elapsed: Seconds) {
        for tree in &mut self.trees {
            if !complete(town, ledger, &tree.id) {
                continue;
            }
            let mut left = elapsed.get();
            while let Some(d) = duration(tree.stage) {
                let take = left.min(d - tree.elapsed.get());
                tree.elapsed = sec(tree.elapsed.get() + take);
                left -= take;
                if tree.elapsed.get() < d {
                    break;
                }
                tree.elapsed = sec(0.);
                tree.stage = match tree.stage {
                    Stage::Sapling => Stage::Blossom,
                    Stage::Blossom => Stage::Fruit,
                    Stage::Fruit => Stage::Ripe,
                    Stage::Harvested => Stage::Blossom,
                    _ => unreachable!(),
                };
                if tree.stage == Stage::Ripe {
                    tree.fruit = Quantity::new(8.).unwrap();
                }
                if left <= 0. {
                    break;
                }
            }
        }
    }
    fn matches(town: &Town, a: &ActorId, j: &Job) -> bool {
        matches!(town.actor(a).map(|a|&a.state().task),Ok(Task::Work{workplace,target,phase:WorkPhase::Outbound|WorkPhase::Working|WorkPhase::Returning|WorkPhase::Depositing,elapsed}) if workplace==&j.orchard&&target==&j.target&&elapsed.get()==0.)
    }
    pub fn cancel(&mut self, town: &mut Town, actor: &ActorId) -> Result<(), Error> {
        if let Some(j) = self.jobs.get(actor)
            && Self::matches(town, actor, j)
        {
            town.set_task(actor, Task::Idle)?;
        }
        if let Some(j) = self.jobs.remove(actor) {
            self.previous.insert(actor.clone(), (j.orchard, j.tree));
        }
        Ok(())
    }
    pub fn assign(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        orchard: &BuildingId,
        nav: NavigationRevision,
    ) -> Result<Option<BuildingId>, Error> {
        let (mut owner, mut model) = (self.clone(), town.clone());
        let result = owner.assign_inner(&mut model, ledger, actor, orchard, nav)?;
        *self = owner;
        *town = model;
        Ok(result)
    }
    fn assign_inner(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        orchard: &BuildingId,
        nav: NavigationRevision,
    ) -> Result<Option<BuildingId>, Error> {
        town.actor(actor)?;
        if self.assignments.get(actor) != Some(orchard) || !complete(town, ledger, orchard) {
            return Err(Error::Invalid);
        }
        self.jobs.retain(|a, j| Self::matches(town, a, j));
        if let Some(j) = self.jobs.get(actor) {
            if &j.orchard == orchard || pending(ledger, &j.orchard, actor)? {
                return Ok(Some(j.tree.clone()));
            }
            self.cancel(town, actor)?;
        }
        if town.actor(actor)?.state().task != Task::Idle
            || ledger.held().contains_key(actor)
            || !town.actor(actor)?.state().cargo.goods().is_empty()
        {
            return Err(Error::Busy);
        }
        let source = ledger
            .stores()
            .iter()
            .find(|s| s.pending.contains_key(&PendingOwner::Worker(actor.clone())))
            .map(|s| s.id.clone());
        let orchard = source.as_ref().unwrap_or(orchard);
        if ledger.store(orchard)?.policy.kind != "orchard" || !complete(town, ledger, orchard) {
            return Err(Error::Busy);
        }
        let carrying = pending(ledger, orchard, actor)?;
        let p = town.actor(actor)?.state().position;
        let candidates: Vec<_> = self
            .trees
            .iter()
            .filter(|t| {
                &t.orchard == orchard
                    && t.stage == Stage::Ripe
                    && complete(town, ledger, &t.id)
                    && !self.jobs.iter().any(|(a, j)| {
                        j.tree == t.id
                            && matches!(
                                phase(town, a),
                                Ok(WorkPhase::Outbound | WorkPhase::Working)
                            )
                    })
            })
            .collect();
        let previous = self
            .previous
            .get(actor)
            .filter(|(o, _)| o == orchard)
            .map(|(_, t)| t);
        let tree = candidates
            .iter()
            .copied()
            .find(|t| Some(&t.id) == previous)
            .or_else(|| {
                candidates.into_iter().min_by(|a, b| {
                    let d = |t: &Tree| {
                        let t = town.building(&t.id).unwrap().location;
                        (f64::from(t.x) - p.x.get()).abs() + (f64::from(t.y) - p.y.get()).abs()
                    };
                    d(a).total_cmp(&d(b))
                })
            });
        if !carrying
            && (ledger
                .store(orchard)?
                .local_room("fruit")?
                .min(ledger.production_room(&fruit())?)
                <= 1e-9
                || tree.is_none())
        {
            return Ok(None);
        }
        let id = tree.map_or_else(|| orchard.clone(), |t| t.id.clone());
        let tile = town.building(&id)?.location;
        let j = Job {
            orchard: orchard.clone(),
            tree: id.clone(),
            target: Destination::Position(
                Position::new(f64::from(tile.x), f64::from(tile.y)).unwrap(),
            ),
            clock: 0.,
            practice_start: None,
            practice_seconds: 0.,
            duration: None,
        };
        let phase = if carrying {
            WorkPhase::Returning
        } else {
            WorkPhase::Outbound
        };
        town.set_task(actor, task(&j, phase))?;
        town.request_route(
            actor,
            if carrying {
                Destination::Building(orchard.clone())
            } else {
                j.target.clone()
            },
            nav,
        )?;
        self.jobs.insert(actor.clone(), j);
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
        nav: NavigationRevision,
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
            nav,
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
        nav: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let Some(j) = self.jobs.get(actor).cloned() else {
            return Ok(Outcome::Waiting);
        };
        if !Self::matches(town, actor, &j)
            || worker.job() != "orcharding"
            || !complete(town, ledger, &j.orchard)
        {
            self.cancel(town, actor)?;
            return Ok(Outcome::Waiting);
        }
        worker.qualification("orcharding")?;
        let mut phase = phase(town, actor)?;
        let mut left = elapsed.get();
        if matches!(phase, WorkPhase::Outbound | WorkPhase::Working)
            && !pending(ledger, &j.orchard, actor)?
            && (!complete(town, ledger, &j.tree)
                || self.tree(&j.tree).is_none_or(|t| t.stage != Stage::Ripe))
        {
            self.cancel(town, actor)?;
            return Ok(Outcome::Waiting);
        }
        if matches!(phase, WorkPhase::Outbound | WorkPhase::Working)
            && !pending(ledger, &j.orchard, actor)?
            && ledger
                .store(&j.orchard)?
                .local_room("fruit")?
                .min(ledger.production_room(&fruit())?)
                <= 1e-9
        {
            return Ok(Outcome::Waiting);
        }
        if matches!(phase, WorkPhase::Outbound | WorkPhase::Returning) {
            town.resume_travel(actor, nav)?;
            let movement = town.advance_travel(actor, elapsed, 4096, |a, b| {
                segment(a, b, Speed::new(1.8).unwrap())
            })?;
            let dest = if phase == WorkPhase::Returning {
                Destination::Building(j.orchard.clone())
            } else {
                j.target.clone()
            };
            if town.actor(actor)?.state().travel != TravelIntent::Arrived(dest) {
                return Ok(Outcome::Waiting);
            }
            left = movement.remaining.get();
            phase = if phase == WorkPhase::Returning {
                WorkPhase::Depositing
            } else {
                WorkPhase::Working
            };
            town.set_task(actor, task(&j, phase))?;
        }
        let clock = self.jobs[actor].clock;
        if phase == WorkPhase::Depositing {
            let take = left.min((0.65 - clock).max(0.));
            self.jobs.get_mut(actor).unwrap().clock += take;
            if clock + take + 1e-9 < 0.65 {
                return Ok(Outcome::Progress);
            }
            if pending(ledger, &j.orchard, actor)? {
                ledger.drop_off(&j.orchard, actor)?;
            }
            self.cancel(town, actor)?;
            return Ok(Outcome::Unloaded {
                remaining: sec(left - take),
            });
        }
        if phase != WorkPhase::Working {
            return Err(Error::Invalid);
        }
        let room = ledger
            .store(&j.orchard)?
            .local_room("fruit")?
            .min(ledger.production_room(&fruit())?);
        if room <= 1e-9 {
            return Ok(Outcome::Waiting);
        }
        let speed = worker.efficiency("orcharding")?
            * (1. + 0.25 * f64::from(ledger.store(&j.orchard)?.policy.level - 1));
        if speed <= 0. {
            return Err(Error::Invalid);
        }
        let active = self.jobs.get_mut(actor).unwrap();
        let duration = *active.duration.get_or_insert(PICK_SECONDS / speed);
        if !duration.is_finite() || duration <= 0. {
            return Err(Error::Invalid);
        }
        let take = left.min((duration - clock).max(0.));
        active.clock += take;
        active.practice_seconds += take;
        let start = *active
            .practice_start
            .get_or_insert(worker.practice_value("orcharding"));
        worker.set_practice(
            "orcharding",
            lantern_core::units::Practice::new(start).map_err(|_| Error::Invalid)?,
        )?;
        worker.practice("orcharding", sec(active.practice_seconds))?;
        if active.clock + 1e-9 < duration {
            return Ok(Outcome::Progress);
        }
        let t = self
            .trees
            .iter_mut()
            .find(|t| t.id == j.tree)
            .ok_or(Error::Invalid)?;
        let amount = room.min(t.fruit.get());
        if amount <= 1e-9 {
            return Ok(Outcome::Waiting);
        }
        ledger.produce(
            &j.orchard,
            Some(actor),
            &Stock::default(),
            &Stock::from_entries([(fruit(), Quantity::new(amount).map_err(|_| Error::Invalid)?)])?,
        )?;
        ledger.adopt_orphan_pending(&j.orchard, actor)?;
        t.fruit = Quantity::new((t.fruit.get() - amount).max(0.)).map_err(|_| Error::Invalid)?;
        if t.fruit.get() <= 1e-9 {
            t.stage = Stage::Harvested;
            t.elapsed = sec(0.);
        }
        self.jobs.get_mut(actor).unwrap().clock = 0.;
        town.set_task(actor, task(&j, WorkPhase::Returning))?;
        town.request_route(actor, Destination::Building(j.orchard.clone()), nav)?;
        Ok(Outcome::Picked {
            tree: j.tree,
            amount: Quantity::new(amount).unwrap(),
            remaining: sec(left - take),
        })
    }
    /// Drop tree facts after an authorized structural removal; pending returns
    /// deliberately keep their original source identity and cargo.
    pub fn remove_tree(&mut self, town: &mut Town, id: &BuildingId) -> Result<(), Error> {
        let (mut owner, mut model) = (self.clone(), town.clone());
        let actors: Vec<_> = owner
            .jobs
            .iter()
            .filter(|(a, j)| {
                &j.tree == id
                    && matches!(
                        phase(&model, a),
                        Ok(WorkPhase::Outbound | WorkPhase::Working)
                    )
            })
            .map(|(a, _)| a.clone())
            .collect();
        for a in actors {
            owner.cancel(&mut model, &a)?;
        }
        owner.trees.retain(|t| &t.id != id);
        *self = owner;
        *town = model;
        Ok(())
    }
}
fn duration(stage: Stage) -> Option<f64> {
    match stage {
        Stage::Sapling | Stage::Fruit => Some(60.),
        Stage::Blossom | Stage::Harvested => Some(30.),
        Stage::Ripe => None,
    }
}

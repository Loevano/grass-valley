//! Experimental woodcutting actors over admitted terrain, R11 movement and R13 goods.
//! The caller pumps navigation and applies depletion events to its R07 landscape.
//! This is not a live-world adapter or a second terrain renewal clock.
use crate::{
    progression::{self, Worker},
    resources::{self, Ledger, PendingOwner, ResourceKey, Stock},
};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity},
    model::{
        ConstructionState, Destination, ModelError, NavigationRevision, RouteState, Task, Town,
        TravelIntent, WorkPhase,
    },
    travel::Speed,
    units::{Position, Quantity, Seconds, Tile},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq)]
pub enum Error {
    Model(ModelError),
    Resource(resources::ResourceError),
    Progression(progression::Error),
    Invalid,
    Busy,
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
fn seconds(n: f64) -> Seconds {
    Seconds::new(n).expect("bounded time")
}
fn quantity(n: f64) -> Result<Quantity, Error> {
    Quantity::new(n).map_err(|_| Error::Invalid)
}
fn position(x: i32, y: i32) -> Position {
    Position::new(f64::from(x), f64::from(y)).unwrap()
}
fn wood() -> ResourceKey {
    ResourceKey::parse("wood").unwrap()
}

/// Actual remaining supply admitted from the authoritative landscape, in map order.
/// Depleted trees are removed here; returned tiles must be committed with the R07
/// resource/stump transaction before exposing the next world snapshot. Species and
/// biome/regrowth metadata stay with that landscape owner, never inferred here.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub tile: Tile,
    pub supply: Quantity,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Trees {
    trees: Vec<Tree>,
}
impl Trees {
    pub fn new(trees: Vec<Tree>) -> Result<Self, Error> {
        let mut seen = BTreeSet::new();
        if trees.iter().any(|t| !seen.insert((t.tile.x, t.tile.y))) {
            return Err(Error::Invalid);
        }
        Ok(Self { trees })
    }
    pub fn trees(&self) -> &[Tree] {
        &self.trees
    }
    pub fn remove(&mut self, tile: Tile) {
        self.trees.retain(|t| t.tile != tile);
    }
    fn available(&self, x: i32, y: i32) -> bool {
        self.trees
            .iter()
            .any(|t| t.tile == Tile { x, y } && t.supply.get() > 0.)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Reachable,
    Pending,
    Unreachable,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    camp: u64,
    camp_x: i32,
    camp_y: i32,
    x: i32,
    y: i32,
}
impl Claim {
    fn building(&self, town: &Town) -> Result<BuildingId, Error> {
        Ok(BuildingId::from_saved(
            EntityIdentity::from_saved(town.owner().clone(), self.camp)
                .map_err(|_| Error::Invalid)?,
        ))
    }
    fn target(&self) -> Destination {
        Destination::Position(position(self.x, self.y))
    }
    fn task(&self, town: &Town, phase: WorkPhase, elapsed: f64) -> Result<Task, Error> {
        Ok(Task::Work {
            workplace: self.building(town)?,
            target: self.target(),
            phase,
            elapsed: seconds(elapsed),
        })
    }
    fn available(&self, town: &Town, ledger: &Ledger) -> bool {
        self.building(town).is_ok_and(|id| {
            town.building(&id).is_ok_and(|b| {
                b.location
                    == Tile {
                        x: self.camp_x,
                        y: self.camp_y,
                    }
                    && b.construction == ConstructionState::Complete
            }) && ledger
                .store(&id)
                .is_ok_and(|s| s.policy.kind == "woodcutter" && !s.policy.construction)
        })
    }
    fn matches(&self, town: &Town, actor: &ActorId) -> bool {
        matches!(
            town.actor(actor).map(|a| &a.state().task),
            Ok(Task::Work { workplace, target, .. })
                if self.building(town).is_ok_and(|b| *workplace == b)
                    && *target == self.target()
        )
    }
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Woodcutters {
    claims: BTreeMap<ActorId, Claim>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting,
    Advanced {
        productive: Seconds,
        produced: Quantity,
        depleted: Option<Tile>,
        remaining: Seconds,
    },
    /// Caller may spend this time on the next admitted leg; it is never banked.
    Unloaded(Seconds),
    Cancelled,
}
impl Woodcutters {
    pub fn target(&self, actor: &ActorId) -> Option<Tile> {
        self.claims.get(actor).map(|c| Tile { x: c.x, y: c.y })
    }
    /// Rank by camp distance, retaining an active target. Pending claims are real
    /// claims; unreachable candidates are skipped. Route queries must use R09–R11
    /// shared-world obstacles, fences and adjacency, never client route answers.
    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trees: &Trees,
        actor: &ActorId,
        camp: &BuildingId,
        worker: &Worker,
        navigation: NavigationRevision,
        mut access: impl FnMut(Tile) -> Access,
    ) -> Result<Option<Tile>, Error> {
        let (mut owner, mut model, mut goods) = (self.clone(), town.clone(), ledger.clone());
        let result = owner.assign_inner(
            &mut model,
            &mut goods,
            trees,
            actor,
            camp,
            worker,
            navigation,
            &mut access,
        )?;
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
        trees: &Trees,
        actor: &ActorId,
        camp: &BuildingId,
        worker: &Worker,
        navigation: NavigationRevision,
        mut access: impl FnMut(Tile) -> Access,
    ) -> Result<Option<Tile>, Error> {
        if self
            .claims
            .keys()
            .any(|a| a.identity().validate_owner(town.owner()).is_err())
        {
            return Err(Error::Invalid);
        }
        town.actor(actor)?;
        if worker.job() != "woodcutting" {
            return Err(Error::Invalid);
        }
        worker.qualification("woodcutting")?;
        let b = town.building(camp)?;
        if b.construction != ConstructionState::Complete
            || ledger.store(camp)?.policy.kind != "woodcutter"
            || ledger.store(camp)?.policy.construction
        {
            return Err(Error::Invalid);
        }
        if let Some(c) = self.claims.get(actor) {
            if c.building(town)? == *camp
                && c.available(town, ledger)
                && c.matches(town, actor)
                && (pending(ledger, camp, actor)?
                    || (trees.available(c.x, c.y)
                        && (!matches!(
                            town.actor(actor)?.route(),
                            RouteState::Unreachable(_) | RouteState::Blocked(_)
                        ) || access(Tile { x: c.x, y: c.y }) != Access::Unreachable)))
            {
                return Ok(self.target(actor));
            }
            self.cancel(town, ledger, actor)?;
        }
        if town.actor(actor)?.state().task != Task::Idle
            || ledger.held().contains_key(actor)
            || !town.actor(actor)?.state().cargo.goods().is_empty()
        {
            return Err(Error::Busy);
        }
        let b = town.building(camp)?;
        let origin = b.location;
        let mut candidates: Vec<_> = trees
            .trees
            .iter()
            .filter(|t| {
                t.supply.get() > 0.
                    && distance(t.tile, origin) <= 10
                    && !self.claims.iter().any(|(a, c)| {
                        c.x == t.tile.x
                            && c.y == t.tile.y
                            && c.available(town, ledger)
                            && c.matches(town, a)
                    })
            })
            .collect();
        candidates.sort_by_key(|t| distance(t.tile, origin));
        for tree in candidates {
            if access(tree.tile) == Access::Unreachable {
                continue;
            }
            let claim = Claim {
                camp: camp.identity().serial(),
                camp_x: origin.x,
                camp_y: origin.y,
                x: tree.tile.x,
                y: tree.tile.y,
            };
            let mut model = town.clone();
            model.set_task(actor, claim.task(town, WorkPhase::Outbound, 0.)?)?;
            model.request_route(actor, claim.target(), navigation)?;
            *town = model;
            self.claims.insert(actor.clone(), claim);
            return Ok(Some(tree.tile));
        }
        Ok(None)
    }
    /// Release the target while retaining finished goods exactly once as orphan
    /// output at the original workplace. Reassignment keeps its new task/position.
    pub fn cancel(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
    ) -> Result<(), Error> {
        let Some(c) = self.claims.get(actor) else {
            return Ok(());
        };
        let id = c.building(town)?;
        let (mut model, mut goods) = (town.clone(), ledger.clone());
        if pending(&goods, &id, actor)? {
            goods.orphan_pending(&id, actor)?;
        }
        if c.matches(town, actor) {
            model.set_task(actor, Task::Idle)?;
        }
        *town = model;
        *ledger = goods;
        self.claims.remove(actor);
        Ok(())
    }
    /// Atomic productive work/accounting. No work or practice accrues while route
    /// pending, blocked, returning or unloading. Supply/cap clipping still trains
    /// the productive interval, matching Python's per-update practice rule.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trees: &mut Trees,
        actor: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        navigation: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut owner, mut model, mut goods, mut forest, mut person) = (
            self.clone(),
            town.clone(),
            ledger.clone(),
            trees.clone(),
            worker.clone(),
        );
        let outcome = owner.advance_inner(
            &mut model,
            &mut goods,
            &mut forest,
            actor,
            &mut person,
            elapsed,
            navigation,
            &mut segment,
        )?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *trees = forest;
        *worker = person;
        Ok(outcome)
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trees: &mut Trees,
        actor: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let Some(c) = self.claims.get(actor).cloned() else {
            return Ok(Outcome::Waiting);
        };
        if !c.available(town, ledger) || !c.matches(town, actor) || worker.job() != "woodcutting" {
            self.cancel(town, ledger, actor)?;
            return Ok(Outcome::Cancelled);
        }
        worker.qualification("woodcutting")?;
        let camp = c.building(town)?;
        let Task::Work {
            mut phase,
            elapsed: clock,
            ..
        } = town.actor(actor)?.state().task.clone()
        else {
            return Err(Error::Invalid);
        };
        let mut clock = clock.get();
        let mut remaining = elapsed.get();
        let live = trees.available(c.x, c.y);
        // Saved cargo finishes its original return even if its tree disappeared.
        let interrupted = matches!(
            town.actor(actor)?.route(),
            RouteState::Unreachable(_) | RouteState::Blocked(_)
        );
        if (!live || (interrupted && pending(ledger, &camp, actor)?))
            && matches!(phase, WorkPhase::Outbound | WorkPhase::Working)
        {
            if !pending(ledger, &camp, actor)? {
                self.cancel(town, ledger, actor)?;
                return Ok(Outcome::Cancelled);
            }
            phase = WorkPhase::Returning;
            clock = 0.;
            town.set_task(actor, c.task(town, phase, 0.)?)?;
            town.request_route(actor, Destination::Building(camp.clone()), navigation)?;
        }
        if matches!(phase, WorkPhase::Outbound | WorkPhase::Returning) {
            town.resume_travel(actor, navigation)?;
            let speed = Speed::new(1.8).unwrap();
            let movement =
                town.advance_travel(actor, seconds(remaining), 4096, |a, b| segment(a, b, speed))?;
            let destination = if phase == WorkPhase::Outbound {
                c.target()
            } else {
                Destination::Building(camp.clone())
            };
            if town.actor(actor)?.state().travel != TravelIntent::Arrived(destination) {
                return Ok(Outcome::Waiting);
            }
            remaining = movement.remaining.get();
            phase = if phase == WorkPhase::Outbound {
                WorkPhase::Working
            } else {
                WorkPhase::Depositing
            };
            clock = 0.;
            town.set_task(actor, c.task(town, phase, clock)?)?;
        }
        if phase == WorkPhase::Depositing {
            let take = remaining.min((0.65 - clock).max(0.));
            clock += take;
            remaining -= take;
            if clock + 1e-9 < 0.65 {
                town.set_task(actor, c.task(town, phase, clock)?)?;
                return Ok(Outcome::Waiting);
            }
            if pending(ledger, &camp, actor)? {
                ledger.drop_off(&camp, actor)?;
            }
            if live {
                town.set_task(actor, c.task(town, WorkPhase::Outbound, 0.)?)?;
                town.request_route(actor, c.target(), navigation)?;
            } else {
                self.claims.remove(actor);
                town.set_task(actor, Task::Idle)?;
            }
            return Ok(Outcome::Unloaded(seconds(remaining)));
        }
        if phase != WorkPhase::Working {
            return Err(Error::Invalid);
        }
        let take = remaining.min((3.2 - clock).max(0.));
        let room = ledger
            .store(&camp)?
            .local_room("wood")?
            .min(ledger.production_room(&wood())?);
        let mut amount = 0.;
        let mut depleted = None;
        if take > 1e-9 && room > 1e-9 {
            let bonus = match worker.tool.as_deref() {
                Some("crafted_axe" | "fine_axe") => 0.25,
                Some("steel_axe") => 0.5,
                Some("masterwork_axe") => 0.75,
                _ => 0.,
            };
            let rate = resources::public_catalog()["wood_per_cycle"]
                .as_f64()
                .ok_or(Error::Invalid)?
                / 3.
                * (1. + bonus)
                * worker.efficiency("woodcutting")?;
            let tree = trees
                .trees
                .iter_mut()
                .find(|t| t.tile == Tile { x: c.x, y: c.y })
                .ok_or(Error::Invalid)?;
            amount = (rate * take).min(room).min(tree.supply.get());
            if amount > 0. {
                let output = Stock::from_entries([(wood(), quantity(amount)?)])?;
                ledger.produce(&camp, Some(actor), &Stock::default(), &output)?;
                ledger.adopt_orphan_pending(&camp, actor)?;
                tree.supply = quantity(tree.supply.get() - amount)?;
                if tree.supply.get() <= 1e-9 {
                    depleted = Some(tree.tile);
                }
                worker.practice("woodcutting", seconds(take))?;
            }
        }
        if let Some(tile) = depleted {
            trees.remove(tile);
        }
        clock += take;
        if clock + 1e-9 >= 3.2 || depleted.is_some() || room <= 1e-9 {
            town.set_task(actor, c.task(town, WorkPhase::Returning, 0.)?)?;
            town.request_route(actor, Destination::Building(camp), navigation)?;
        } else {
            town.set_task(actor, c.task(town, phase, clock)?)?;
        }
        Ok(Outcome::Advanced {
            productive: seconds(if amount > 0. { take } else { 0. }),
            produced: quantity(amount)?,
            depleted,
            remaining: seconds(remaining - take),
        })
    }
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        if self
            .claims
            .keys()
            .any(|a| a.identity().validate_owner(town.owner()).is_err())
        {
            return Err(Error::Invalid);
        }
        serde_json::to_string(&Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            claims: self
                .claims
                .iter()
                .map(|(a, c)| (a.identity().serial(), c.clone()))
                .collect(),
        })
        .map_err(|_| Error::Invalid)
    }
    /// Restore alongside independent model/ledger/terrain facts. Routes rebuild
    /// through R11. Missing trees are allowed so finished cargo can return.
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if saved.version != 1
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        let mut result = Self::default();
        let mut targets = BTreeSet::new();
        for (serial, c) in saved.claims {
            let actor = ActorId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), serial)
                    .map_err(|_| Error::Invalid)?,
            );
            let state = town.actor(&actor)?.state();
            // Stale assignments are admitted only for cancellation on advance;
            // matching work must carry a valid woodcutting phase/clock.
            let valid_clock = match state.task {
                Task::Work { phase, elapsed, .. } if c.matches(town, &actor) => {
                    let limit = match phase {
                        WorkPhase::Working => 3.2,
                        WorkPhase::Depositing => 0.65,
                        WorkPhase::Outbound | WorkPhase::Returning => 0.,
                        _ => return Err(Error::Invalid),
                    };
                    elapsed.get() <= limit
                }
                _ => true,
            };
            if distance(
                Tile { x: c.x, y: c.y },
                Tile {
                    x: c.camp_x,
                    y: c.camp_y,
                },
            ) > 10
                || ledger.store(&c.building(town)?)?.policy.kind != "woodcutter"
                || !valid_clock
                || !targets.insert((c.x, c.y))
                || ledger.held().contains_key(&actor)
                || !town.actor(&actor)?.state().cargo.goods().is_empty()
                || result.claims.insert(actor, c).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        Ok(result)
    }
}
fn pending(ledger: &Ledger, camp: &BuildingId, actor: &ActorId) -> Result<bool, Error> {
    Ok(ledger
        .store(camp)?
        .pending
        .contains_key(&PendingOwner::Worker(actor.clone())))
}
fn distance(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    world: String,
    town: String,
    claims: Vec<(u64, Claim)>,
}

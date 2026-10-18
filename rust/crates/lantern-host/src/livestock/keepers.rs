use super::*;
const SPOTS: [(i32, i32); 8] = [
    (2, 0),
    (0, 2),
    (-2, 0),
    (0, -2),
    (2, 2),
    (-2, 2),
    (-2, -2),
    (2, -2),
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Keeper {
    pub actor: u64,
    pub barn: u64,
    pub(super) origin: [i32; 2],
    pub(super) cursor: usize,
    #[serde(deserialize_with = "super::save::required")]
    pub(super) destination: Option<[i32; 2]>,
    last_practice: u64,
}
impl Keeper {
    fn task(&self, town: &Town, phase: WorkPhase, clock: f64) -> Result<Task, Error> {
        Ok(Task::Work {
            workplace: bid(town, self.barn)?,
            target: self.target(town)?,
            phase,
            elapsed: sec(clock),
        })
    }
    pub(super) fn target(&self, town: &Town) -> Result<Destination, Error> {
        Ok(if let Some(d) = self.destination {
            Destination::Position(pos(tile(d)))
        } else {
            Destination::Building(bid(town, self.barn)?)
        })
    }
    pub(super) fn matches(&self, town: &Town) -> bool {
        aid(town,self.actor).is_ok_and(|id|matches!(town.actor(&id).map(|a|&a.state().task),Ok(Task::Work{workplace,target,..}) if workplace.identity().serial()==self.barn && self.target(town).is_ok_and(|d|d==*target)))
    }
}
impl Livestock {
    /// R18 supplies assigned, admitted husbandry workers, excluding construction
    /// helpers. Assignment persists even when navigation or production is paused.
    pub fn set_keepers(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        id: &BuildingId,
        roster: &[(ActorId, Worker)],
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        self.validate(town)?;
        self.barn(id)?;
        if !complete(town, ledger, id, "barn") {
            return Err(Error::Invalid);
        }
        let (mut owner, mut model) = (self.clone(), town.clone());
        let mut ids = BTreeSet::new();
        for (a, w) in roster {
            model.actor(a)?;
            if w.job() != "husbandry"
                || !ids.insert(a.identity().serial())
                || owner
                    .orders
                    .iter()
                    .any(|o| o.handler == Some(a.identity().serial()))
                || owner
                    .keepers
                    .iter()
                    .any(|k| k.actor == a.identity().serial() && k.barn != id.identity().serial())
            {
                return Err(Error::Busy);
            }
        }
        for k in owner
            .keepers
            .iter()
            .filter(|k| k.barn == id.identity().serial() && !ids.contains(&k.actor))
        {
            if k.matches(&model) {
                model.set_task(&aid(&model, k.actor)?, Task::Idle)?;
            }
        }
        owner
            .keepers
            .retain(|k| k.barn != id.identity().serial() || ids.contains(&k.actor));
        for (index, (a, _)) in roster.iter().enumerate() {
            if let Some(k) = owner
                .keepers
                .iter()
                .find(|k| k.actor == a.identity().serial())
            {
                if model.actor(a)?.state().task == Task::Idle {
                    model.set_task(a, k.task(&model, WorkPhase::Outbound, 0.)?)?;
                    model.request_route(a, k.target(&model)?, nav)?;
                } else if !k.matches(&model)
                    && !matches!(
                        model.actor(a)?.state().task,
                        Task::Transport { .. } | Task::Construction { .. }
                    )
                {
                    return Err(Error::Busy);
                }
                continue;
            }
            if model.actor(a)?.state().task != Task::Idle
                || !model.actor(a)?.state().cargo.goods().is_empty()
                || ledger.held().contains_key(a)
            {
                return Err(Error::Busy);
            }
            let k = Keeper {
                actor: a.identity().serial(),
                barn: id.identity().serial(),
                origin: pair(model.building(id)?.location),
                cursor: index % 8,
                destination: None,
                last_practice: 0,
            };
            model.set_task(a, k.task(&model, WorkPhase::Outbound, 0.)?)?;
            model.request_route(a, Destination::Building(id.clone()), nav)?;
            owner.keepers.push(k);
        }
        *self = owner;
        *town = model;
        Ok(())
    }
    fn round(&self, town: &Town, scene: &Scene, k: &mut Keeper) -> Result<(), Error> {
        let at = town.building(&bid(town, k.barn)?)?.location;
        for step in 0..8 {
            let i = (k.cursor + step) % 8;
            let (dx, dy) = SPOTS[i];
            let Some(x) = at.x.checked_add(dx) else {
                continue;
            };
            let Some(y) = at.y.checked_add(dy) else {
                continue;
            };
            let p = Tile { x, y };
            let claimed = self.keepers.iter().any(|other| {
                other.actor != k.actor
                    && other.barn == k.barn
                    && other.destination == Some(pair(p))
                    && other.matches(town)
                    && aid(town, other.actor).is_ok_and(|a| {
                        matches!(
                            town.actor(&a).map(|a| &a.state().task),
                            Ok(Task::Work {
                                phase: WorkPhase::Outbound | WorkPhase::Working,
                                ..
                            })
                        )
                    })
            });
            if scene.open(p) && !claimed {
                k.destination = Some(pair(p));
                k.cursor = (i + 1) % 8;
                return Ok(());
            }
        }
        k.destination = None;
        Ok(())
    }
    pub fn keeper_query(&self, town: &Town, scene: &Scene, a: &ActorId) -> Result<Query, Error> {
        self.validate(town)?;
        let k = self
            .keepers
            .iter()
            .find(|k| k.actor == a.identity().serial())
            .ok_or(Error::Invalid)?;
        if !k.matches(town) {
            return Err(Error::Invalid);
        }
        let state = town.actor(a)?.state();
        let at = town.building(&bid(town, k.barn)?)?.location;
        let returning = matches!(
            state.task,
            Task::Work {
                phase: WorkPhase::Returning,
                ..
            }
        );
        let target = match k.destination {
            Some(d) if !returning => Target::Exact(tile(d)),
            _ => Target::Perimeter(scene.footprint(town, &bid(town, k.barn)?)?),
        };
        let start = rounded(state.position);
        let bounds = if (i64::from(start.x) - i64::from(at.x)).abs() <= 3
            && (i64::from(start.y) - i64::from(at.y)).abs() <= 3
        {
            Some((
                at.x.saturating_sub(3),
                at.x.saturating_add(3),
                at.y.saturating_sub(3),
                at.y.saturating_add(3),
            ))
        } else {
            None
        };
        scene.query(state.position, target, bounds)
    }
    /// Reconcile after R12 commits a move. Replans from the saved actor pose;
    /// neither growth, inventory nor the keeper's practice clock advances here.
    pub fn relocate_keepers(
        &mut self,
        town: &mut Town,
        id: &BuildingId,
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        self.validate(town)?;
        let (mut owner, mut model) = (self.clone(), town.clone());
        let at = model.building(id)?.location;
        for k in owner
            .keepers
            .iter_mut()
            .filter(|k| k.barn == id.identity().serial())
        {
            if k.origin == pair(at) {
                continue;
            }
            let a = aid(&model, k.actor)?;
            let active = k.matches(&model);
            k.origin = pair(at);
            k.destination = None;
            if !active {
                continue;
            }
            model.set_task(&a, k.task(&model, WorkPhase::Outbound, 0.)?)?;
            model.request_route(&a, Destination::Building(id.clone()), nav)?;
        }
        *self = owner;
        *town = model;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance_keeper(
        &mut self,
        town: &mut Town,
        scene: &Scene,
        a: &ActorId,
        elapsed: Seconds,
        nav: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        self.validate(town)?;
        let Some(index) = self
            .keepers
            .iter()
            .position(|k| k.actor == a.identity().serial())
        else {
            return Ok(Outcome::Waiting);
        };
        let (mut owner, mut model) = (self.clone(), town.clone());
        let mut k = owner.keepers[index].clone();
        if !k.matches(&model) || pair(model.building(&bid(&model, k.barn)?)?.location) != k.origin {
            return Ok(Outcome::NeedsReconcile);
        }
        let Task::Work {
            mut phase,
            elapsed: clock,
            ..
        } = model.actor(a)?.state().task
        else {
            return Err(Error::Invalid);
        };
        let mut clock = clock.get();
        let mut remaining = elapsed.get();
        if phase == WorkPhase::Outbound
            && matches!(model.actor(a)?.route(), RouteState::Unreachable(_))
        {
            owner.round(&model, scene, &mut k)?;
            model.set_task(a, k.task(&model, phase, 0.)?)?;
            model.request_route(a, k.target(&model)?, nav)?;
            owner.keepers[index] = k;
            *self = owner;
            *town = model;
            return Ok(Outcome::RouteRequested(elapsed));
        }
        if matches!(phase, WorkPhase::Outbound | WorkPhase::Returning) {
            model.resume_travel(a, nav)?;
            let query = self.keeper_query(&model, scene, a)?;
            let movement = model.advance_travel(a, sec(remaining), 4096, |x, y| {
                query
                    .segment_safe(x, y)
                    .then(|| segment(x, y, Speed::new(1.8).unwrap()))
                    .flatten()
            })?;
            if !matches!(model.actor(a)?.state().travel, TravelIntent::Arrived(_)) {
                *town = model;
                return Ok(Outcome::Waiting);
            }
            remaining = movement.remaining.get();
            phase = if phase == WorkPhase::Outbound {
                WorkPhase::Working
            } else {
                WorkPhase::Depositing
            };
            clock = 0.;
            model.set_task(a, k.task(&model, phase, 0.)?)?;
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
            model.set_task(a, k.task(&model, phase, clock)?)?;
            *town = model;
            return Ok(Outcome::Advanced);
        }
        let result = if phase == WorkPhase::Depositing {
            owner.round(&model, scene, &mut k)?;
            model.set_task(a, k.task(&model, WorkPhase::Outbound, 0.)?)?;
            model.request_route(a, k.target(&model)?, nav)?;
            Outcome::Unloaded(sec(remaining))
        } else {
            model.set_task(a, k.task(&model, WorkPhase::Returning, 0.)?)?;
            model.request_route(a, Destination::Building(bid(&model, k.barn)?), nav)?;
            Outcome::RouteRequested(sec(remaining))
        };
        owner.keepers[index] = k;
        *self = owner;
        *town = model;
        Ok(result)
    }
    /// Python trains assigned keepers once per production batch even in confirmed
    /// walking phases, but never during pending/blocked routing or empty herds.
    pub fn practice_keeper(
        &mut self,
        town: &Town,
        a: &ActorId,
        worker: &mut Worker,
        update: u64,
    ) -> Result<bool, Error> {
        self.validate(town)?;
        if update == 0 {
            return Err(Error::Invalid);
        }
        let Some(i) = self
            .keepers
            .iter()
            .position(|k| k.actor == a.identity().serial())
        else {
            return Ok(false);
        };
        if update <= self.keepers[i].last_practice {
            return Ok(false);
        }
        let k = &self.keepers[i];
        let empty = self
            .barns
            .iter()
            .find(|b| b.id == k.barn)
            .is_none_or(|b| b.herds.iter().all(|h| h.animals.is_empty()));
        let allowed = worker.job() == "husbandry"
            && k.matches(town)
            && !empty
            && !matches!(
                town.actor(a)?.route(),
                RouteState::Pending(_)
                    | RouteState::Unreachable(_)
                    | RouteState::Blocked(_)
                    | RouteState::Cancelled
            );
        let mut person = worker.clone();
        if allowed {
            person.qualification("husbandry")?;
            person.practice("husbandry", sec(3.))?;
        }
        self.keepers[i].last_practice = update;
        *worker = person;
        Ok(allowed)
    }
}

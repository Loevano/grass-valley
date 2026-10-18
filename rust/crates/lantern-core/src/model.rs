//! Experimental entity/task substrate, not a complete gameplay/save schema.
//!
//! Actor facts survive task replacement and route invalidation. Route state is
//! derived and starts empty after restoration; tasks retain their phase/leg.
//! No serialization, search frontiers, clocks or transfer rules here.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use crate::{
    ids::{ActorId, BuildingId, IdentityError, Owner},
    units::{Position, Quantity, Seconds, Tile, Work},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    Identity(IdentityError),
    MissingActor,
    MissingBuilding,
    InvalidCargoKey,
    DuplicateCargoKey,
    GenerationExhausted,
    NoTask,
    StaleRoute,
    EmptyRoute,
    InvalidRouteGeometry,
    RouteNotReady,
}

impl From<IdentityError> for ModelError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}

macro_rules! generation {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $name(u64);

        impl $name {
            pub fn from_saved(value: u64) -> Self {
                Self(value)
            }
            pub fn get(self) -> u64 {
                self.0
            }
            fn next(self) -> Result<Self, ModelError> {
                self.0
                    .checked_add(1)
                    .map(Self)
                    .ok_or(ModelError::GenerationExhausted)
            }
        }
    };
}

generation!(TaskGeneration);
generation!(RouteGeneration);

/// Version of relevant navigation dependencies, supplied by the navigation owner.
/// It is independent of economic output, API and persistence revisions. R10 will
/// supply region/access dependency tracking; this model does not version terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationRevision(pub u64);

/// Named goods remain fractional and separate from tasks, paths and presentation.
/// Resource catalog validation and exact-once transfer rules belong to R13–R17.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cargo(BTreeMap<String, Quantity>);

impl Cargo {
    pub fn from_entries(
        entries: impl IntoIterator<Item = (String, Quantity)>,
    ) -> Result<Self, ModelError> {
        let mut goods = BTreeMap::new();
        for (key, amount) in entries {
            if key.is_empty() {
                return Err(ModelError::InvalidCargoKey);
            }
            if goods.insert(key, amount).is_some() {
                return Err(ModelError::DuplicateCargoKey);
            }
        }
        Ok(Self(goods))
    }

    pub fn goods(&self) -> &BTreeMap<String, Quantity> {
        &self.0
    }
}

/// Stable references to buildings; free positions are locations, never IDs.
#[derive(Clone, Debug, PartialEq)]
pub enum Destination {
    Building(BuildingId),
    Position(Position),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkPhase {
    Outbound,
    Working,
    Returning,
    Depositing,
    FetchingWater,
    CollectingWater,
    ReturningWater,
    WaitingStorage,
    WaitingField,
    HarvestReady,
    FindingAnimal,
    Dragging,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportPhase {
    ToPickup,
    PickingUp,
    ToDropoff,
    DroppingOff,
    Returning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionPhase {
    Outbound,
    Building,
}

/// Persistent intent/phase. Route pending/blocked status does not overwrite this.
/// Feature-specific task data (farm claims, boats, recipes, reservations, etc.)
/// remains unported and must not be silently imported into these variants.
#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Idle,
    Move {
        destination: Position,
    },
    Work {
        workplace: BuildingId,
        target: Destination,
        phase: WorkPhase,
        elapsed: Seconds,
    },
    Transport {
        source: BuildingId,
        destination: BuildingId,
        phase: TransportPhase,
    },
    Construction {
        site: BuildingId,
        phase: ConstructionPhase,
        elapsed: Seconds,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstructionState {
    Complete,
    InProgress { remaining: Work },
}

/// Building identity/location foundation. Catalog/footprint/inventory/progression
/// fields are added with their feature ports, not inferred or defaulted on import.
#[derive(Clone, Debug, PartialEq)]
pub struct Building {
    pub id: BuildingId,
    pub location: Tile,
    pub construction: ConstructionState,
}

/// Internal authoritative actor facts, explicitly separate from derived routes.
/// This is not a protocol/save DTO and has no Serialize/Deserialize implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorState {
    pub id: ActorId,
    pub position: Position,
    pub cargo: Cargo,
    pub task: Task,
    pub task_generation: TaskGeneration,
    pub route_generation: RouteGeneration,
}

impl ActorState {
    pub fn new(id: ActorId, position: Position, cargo: Cargo) -> Self {
        Self {
            id,
            position,
            cargo,
            task: Task::Idle,
            task_generation: TaskGeneration::default(),
            route_generation: RouteGeneration::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteRequest {
    actor: ActorId,
    task_generation: TaskGeneration,
    route_generation: RouteGeneration,
    origin: Position,
    destination: Destination,
    goal: Position,
    navigation: NavigationRevision,
}

impl RouteRequest {
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }
    pub fn task_generation(&self) -> TaskGeneration {
        self.task_generation
    }
    pub fn route_generation(&self) -> RouteGeneration {
        self.route_generation
    }
    pub fn navigation(&self) -> NavigationRevision {
        self.navigation
    }
    /// Retained identity bytes, including an optional building destination.
    pub fn identity_bytes(&self) -> usize {
        let owner = self.actor.identity().owner();
        let mut bytes = owner
            .world
            .as_str()
            .len()
            .saturating_add(owner.town.as_str().len());
        if let Destination::Building(id) = &self.destination {
            let owner = id.identity().owner();
            bytes = bytes
                .saturating_add(owner.world.as_str().len())
                .saturating_add(owner.town.as_str().len());
        }
        bytes
    }
    pub fn origin(&self) -> Position {
        self.origin
    }
    pub fn goal(&self) -> Position {
        self.goal
    }
    pub fn destination(&self) -> &Destination {
        &self.destination
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RouteOutcome {
    Ready(Vec<Position>),
    Arrived,
    Unreachable,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RouteState {
    NotRequested,
    Pending(RouteRequest),
    Ready {
        request: RouteRequest,
        waypoints: Vec<Position>,
    },
    Arrived(RouteRequest),
    Unreachable(RouteRequest),
    Blocked(RouteRequest),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    state: ActorState,
    route: RouteState,
    /// Derived cursor, discarded together with the route on restoration.
    route_step: usize,
}

impl Actor {
    pub fn state(&self) -> &ActorState {
        &self.state
    }
    pub fn route(&self) -> &RouteState {
        &self.route
    }
    pub fn travel_status(&self) -> crate::travel::Status {
        use crate::travel::Status;
        match self.route {
            RouteState::NotRequested => Status::NotRequested,
            RouteState::Pending(_) => Status::Waiting,
            RouteState::Ready { .. } => Status::Walking,
            RouteState::Arrived(_) => Status::Arrived,
            RouteState::Unreachable(_) => Status::Unreachable,
            RouteState::Blocked(_) => Status::Blocked,
            RouteState::Cancelled => Status::Cancelled,
        }
    }
}

/// One owned collection. Mutation uses stable IDs, never list offsets. A world
/// loader must additionally validate the world-wide serial set/allocator (ids.rs)
/// and retire jobs from any previous simulation instance before restoring. A
/// rollback to an older snapshot also rolls back counters; host job lifetimes
/// must not cross that boundary.
#[derive(Debug, PartialEq)]
pub struct Town {
    owner: Owner,
    buildings: Vec<Building>,
    actors: Vec<Actor>,
}

impl Town {
    pub fn restore(
        owner: Owner,
        buildings: Vec<Building>,
        actors: Vec<ActorState>,
    ) -> Result<Self, ModelError> {
        let mut seen = BTreeSet::new();
        for id in buildings
            .iter()
            .map(|b| b.id.identity())
            .chain(actors.iter().map(|a| a.id.identity()))
        {
            id.validate_owner(&owner)?;
            if !seen.insert(id.serial()) {
                return Err(IdentityError::DuplicateSerial.into());
            }
        }
        let town = Self {
            owner,
            buildings,
            actors: actors
                .into_iter()
                .map(|state| Actor {
                    state,
                    route: RouteState::NotRequested,
                    route_step: 0,
                })
                .collect(),
        };
        for actor in &town.actors {
            town.validate_task(&actor.state.task)?;
        }
        Ok(town)
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn buildings(&self) -> &[Building] {
        &self.buildings
    }
    pub fn actors(&self) -> &[Actor] {
        &self.actors
    }

    pub fn building(&self, id: &BuildingId) -> Result<&Building, ModelError> {
        id.identity().validate_owner(&self.owner)?;
        self.buildings
            .iter()
            .find(|b| &b.id == id)
            .ok_or(ModelError::MissingBuilding)
    }

    pub fn actor(&self, id: &ActorId) -> Result<&Actor, ModelError> {
        Ok(&self.actors[self.actor_index(id)?])
    }

    fn actor_index(&self, id: &ActorId) -> Result<usize, ModelError> {
        id.identity().validate_owner(&self.owner)?;
        self.actors
            .iter()
            .position(|a| &a.state.id == id)
            .ok_or(ModelError::MissingActor)
    }

    fn resolve(&self, destination: &Destination) -> Result<Position, ModelError> {
        match destination {
            Destination::Building(id) => {
                let tile = self.building(id)?.location;
                // i32 tile coordinates convert exactly to binary64.
                Ok(Position::new(f64::from(tile.x), f64::from(tile.y))
                    .expect("finite i32 coordinates"))
            }
            Destination::Position(position) => Ok(*position),
        }
    }

    fn validate_task(&self, task: &Task) -> Result<(), ModelError> {
        match task {
            Task::Idle | Task::Move { .. } => {}
            Task::Work {
                workplace, target, ..
            } => {
                self.building(workplace)?;
                self.resolve(target)?;
            }
            Task::Transport {
                source,
                destination,
                ..
            } => {
                self.building(source)?;
                self.building(destination)?;
            }
            Task::Construction { site, .. } => {
                self.building(site)?;
            }
        }
        Ok(())
    }

    /// Replace intent without teleporting the actor or disposing of its cargo.
    /// Cancellation/refunds/delivery must be an explicit later accounting action.
    pub fn set_task(&mut self, id: &ActorId, task: Task) -> Result<(), ModelError> {
        let index = self.actor_index(id)?;
        self.validate_task(&task)?;
        let generation = self.actors[index].state.task_generation.next()?;
        let actor = &mut self.actors[index];
        actor.state.task = task;
        actor.state.task_generation = generation;
        actor.route = RouteState::NotRequested;
        Ok(())
    }

    /// Structural relocation only; collision, costs and linked fields are R12.
    /// Only routes targeting this building are invalidated here. Later navigation
    /// dependency validation also accounts for changed footprints/access en route.
    pub fn relocate_building(&mut self, id: &BuildingId, location: Tile) -> Result<(), ModelError> {
        self.building(id)?;
        let building = self
            .buildings
            .iter_mut()
            .find(|b| &b.id == id)
            .expect("validated building");
        if building.location == location {
            return Ok(());
        }
        building.location = location;
        for actor in &mut self.actors {
            let request = match &actor.route {
                RouteState::Pending(r)
                | RouteState::Arrived(r)
                | RouteState::Unreachable(r)
                | RouteState::Blocked(r)
                | RouteState::Ready { request: r, .. } => Some(r),
                _ => None,
            };
            if request.is_some_and(
                |r| matches!(&r.destination, Destination::Building(target) if target == id),
            ) {
                actor.route = RouteState::NotRequested;
            }
        }
        Ok(())
    }

    /// Explicit position recovery/teleport, not a per-step movement integrator.
    pub fn reposition_actor(&mut self, id: &ActorId, position: Position) -> Result<(), ModelError> {
        let index = self.actor_index(id)?;
        let actor = &mut self.actors[index];
        if actor.state.position != position {
            actor.state.position = position;
            actor.route = RouteState::NotRequested;
        }
        Ok(())
    }

    /// Rules choose the task's next goal/arrival policy; R09–R11 validate access
    /// and paths. The request captures identity, origin, goal and generations.
    pub fn request_route(
        &mut self,
        id: &ActorId,
        destination: Destination,
        navigation: NavigationRevision,
    ) -> Result<RouteRequest, ModelError> {
        let index = self.actor_index(id)?;
        let goal = self.resolve(&destination)?;
        let actor = &mut self.actors[index];
        if actor.state.task == Task::Idle {
            return Err(ModelError::NoTask);
        }
        let generation = actor.state.route_generation.next()?;
        let request = RouteRequest {
            actor: id.clone(),
            task_generation: actor.state.task_generation,
            route_generation: generation,
            origin: actor.state.position,
            destination,
            goal,
            navigation,
        };
        actor.state.route_generation = generation;
        actor.route = RouteState::Pending(request.clone());
        Ok(request)
    }

    /// Owner token check for dependency-validated navigation. Revision alone is
    /// not a dependency proof; the service must additionally validate its query.
    pub fn route_generation_is_current(&self, request: &RouteRequest) -> bool {
        self.route_is_current(request, request.navigation())
    }

    /// Read-only bridge for the derived navigation service. It owns no actor facts.
    pub fn route_is_current(&self, request: &RouteRequest, navigation: NavigationRevision) -> bool {
        self.actor(request.actor()).is_ok_and(|actor| {
            matches!(&actor.route, RouteState::Pending(pending) if pending == request)
                && request.navigation == navigation
                && request.origin == actor.state.position
        })
    }

    /// A result can be accepted once, only for the current pending request and
    /// current navigation dependencies. Failed validation never alters the actor.
    pub fn accept_route(
        &mut self,
        id: &ActorId,
        request: &RouteRequest,
        navigation: NavigationRevision,
        outcome: RouteOutcome,
    ) -> Result<(), ModelError> {
        let index = self.actor_index(id)?;
        let actor = &mut self.actors[index];
        if !matches!(&actor.route, RouteState::Pending(pending) if pending == request)
            || request.navigation != navigation
            || request.origin != actor.state.position
        {
            return Err(ModelError::StaleRoute);
        }
        let route = match outcome {
            RouteOutcome::Ready(waypoints) => {
                if waypoints.is_empty() {
                    return Err(ModelError::EmptyRoute);
                }
                let mut previous = actor.state.position;
                for point in &waypoints {
                    if !libm::hypot(
                        point.x.get() - previous.x.get(),
                        point.y.get() - previous.y.get(),
                    )
                    .is_finite()
                    {
                        return Err(ModelError::InvalidRouteGeometry);
                    }
                    previous = *point;
                }
                RouteState::Ready {
                    request: request.clone(),
                    waypoints,
                }
            }
            RouteOutcome::Arrived => RouteState::Arrived(request.clone()),
            RouteOutcome::Unreachable => RouteState::Unreachable(request.clone()),
        };
        actor.route = route;
        actor.route_step = 0;
        Ok(())
    }

    /// Advance only a confirmed route, preserving cargo and task/leg/elapsed.
    /// The host supplies elapsed simulation time, not navigation/wall-clock time.
    /// `segment` checks CURRENT geometry/access and returns the actual speed
    /// (including roads/equipment), or None to stop before a blocked segment.
    /// Fractional origins must be checked too; callers may use the R09 query's
    /// accepted-route validation, but must not reuse a stale geometry certificate.
    /// `max_segments` bounds traversal even for duplicate/zero-length waypoints.
    /// Reaching this bound leaves a Walking route and returns unused time.
    pub fn advance_travel(
        &mut self,
        id: &ActorId,
        elapsed: Seconds,
        max_segments: usize,
        mut segment: impl FnMut(Position, Position) -> Option<crate::travel::Speed>,
    ) -> Result<crate::travel::Advance, ModelError> {
        let index = self.actor_index(id)?;
        let actor = &mut self.actors[index];
        let mut remaining = elapsed.get();
        let mut segments = 0;
        let mut arrived_now = false;
        if let RouteState::Ready { request, waypoints } = &actor.route {
            let mut blocked = false;
            while actor.route_step < waypoints.len() && segments < max_segments {
                let before = actor.state.position;
                let target = waypoints[actor.route_step];
                let dx = target.x.get() - before.x.get();
                let dy = target.y.get() - before.y.get();
                let distance = libm::hypot(dx, dy);
                // Search paths include their origin. Consuming it costs no time.
                if distance == 0.0 {
                    actor.route_step += 1;
                    segments += 1;
                    continue;
                }
                if remaining == 0.0 {
                    break;
                }
                let Some(speed) = segment(before, target) else {
                    blocked = true;
                    break;
                };
                let duration = distance / speed.get();
                segments += 1;
                if duration <= remaining {
                    actor.state.position = target;
                    actor.route_step += 1;
                    remaining -= duration;
                } else {
                    let fraction = if duration.is_infinite() {
                        // Here speed < 1: multiply first to avoid an overflowing
                        // duration discarding otherwise representable movement.
                        remaining * speed.get() / distance
                    } else {
                        remaining / duration
                    };
                    actor.state.position = Position::new(
                        before.x.get() + dx * fraction,
                        before.y.get() + dy * fraction,
                    )
                    .expect("finite interpolation of validated segment");
                    remaining = 0.0;
                    break;
                }
            }
            if blocked {
                actor.route = RouteState::Blocked(request.clone());
            } else if actor.route_step == waypoints.len() {
                actor.route = RouteState::Arrived(request.clone());
                arrived_now = true;
            }
        }
        Ok(crate::travel::Advance {
            status: actor.travel_status(),
            remaining: Seconds::new(remaining).expect("nonnegative finite remainder"),
            arrived_now,
            segments,
        })
    }

    pub fn cancel_route(&mut self, id: &ActorId) -> Result<(), ModelError> {
        let index = self.actor_index(id)?;
        self.actors[index].route = RouteState::Cancelled;
        Ok(())
    }

    pub fn block_route(&mut self, id: &ActorId) -> Result<(), ModelError> {
        let index = self.actor_index(id)?;
        let actor = &mut self.actors[index];
        let RouteState::Ready { request, .. } = &actor.route else {
            return Err(ModelError::RouteNotReady);
        };
        actor.route = RouteState::Blocked(request.clone());
        Ok(())
    }
}

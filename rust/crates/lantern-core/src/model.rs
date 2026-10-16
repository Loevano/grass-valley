//! Experimental entity/task substrate, not a complete gameplay/save schema.
//!
//! Actor facts survive task replacement and route invalidation. Route state is
//! derived and starts empty after restoration; tasks retain their phase/leg.
//! No serialization, search frontiers, clocks, transfer rules or simulation here.

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
}

impl Actor {
    pub fn state(&self) -> &ActorState {
        &self.state
    }
    pub fn route(&self) -> &RouteState {
        &self.route
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
                RouteState::Ready {
                    request: request.clone(),
                    waypoints,
                }
            }
            RouteOutcome::Arrived => RouteState::Arrived(request.clone()),
            RouteOutcome::Unreachable => RouteState::Unreachable(request.clone()),
        };
        actor.route = route;
        Ok(())
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

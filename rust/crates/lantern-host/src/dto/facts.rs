use super::wire::{Decimal, Number};
use crate::{BoundaryError, ErrorKind};
use lantern_core::{
    ids::{EntityIdentity, Owner, PersistentId, TownId, WorldId},
    model::{
        ConstructionPhase, ConstructionState, Destination, Task, TransportPhase, TravelIntent,
        WorkPhase,
    },
    units::{Position, Seconds, Tile, Work},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityIdDto {
    world_id: String,
    town_id: String,
    serial: Decimal,
}

impl EntityIdDto {
    pub fn from_id<K>(id: &PersistentId<K>) -> Self {
        Self {
            world_id: id.identity().owner().world.as_str().into(),
            town_id: id.identity().owner().town.as_str().into(),
            serial: Decimal(id.identity().serial()),
        }
    }

    pub(crate) fn into_id<K>(self, path: &str) -> Result<PersistentId<K>, BoundaryError> {
        let owner = Owner {
            world: WorldId::new(self.world_id).map_err(|e| {
                BoundaryError::at(format!("{path}.world_id"), ErrorKind::Identity(e))
            })?,
            town: TownId::new(self.town_id).map_err(|e| {
                BoundaryError::at(format!("{path}.town_id"), ErrorKind::Identity(e))
            })?,
        };
        EntityIdentity::from_saved(owner, self.serial.0)
            .map(PersistentId::from_saved)
            .map_err(|e| BoundaryError::at(format!("{path}.serial"), ErrorKind::Identity(e)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionDto {
    pub x: Number,
    pub y: Number,
}

impl From<Position> for PositionDto {
    fn from(p: Position) -> Self {
        Self {
            x: Number::core(p.x.get()),
            y: Number::core(p.y.get()),
        }
    }
}

impl PositionDto {
    pub fn into_core(self, path: &str) -> Result<Position, BoundaryError> {
        Position::new(self.x.get(), self.y.get())
            .map_err(|e| BoundaryError::at(path, ErrorKind::Unit(e)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DestinationDto {
    Building { id: EntityIdDto },
    Position { position: PositionDto },
}

impl From<&Destination> for DestinationDto {
    fn from(d: &Destination) -> Self {
        match d {
            Destination::Building(id) => Self::Building {
                id: EntityIdDto::from_id(id),
            },
            Destination::Position(p) => Self::Position {
                position: (*p).into(),
            },
        }
    }
}

impl DestinationDto {
    pub(crate) fn into_core(self, path: &str) -> Result<Destination, BoundaryError> {
        match self {
            Self::Building { id } => Ok(Destination::Building(
                id.into_id(&format!("{path}.building.id"))?,
            )),
            Self::Position { position } => Ok(Destination::Position(
                position.into_core(&format!("{path}.position.position"))?,
            )),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TravelDto {
    None {},
    Active { destination: DestinationDto },
    Arrived { destination: DestinationDto },
    Cancelled {},
}
impl From<&TravelIntent> for TravelDto {
    fn from(value: &TravelIntent) -> Self {
        match value {
            TravelIntent::None => Self::None {},
            TravelIntent::Active(d) => Self::Active {
                destination: d.into(),
            },
            TravelIntent::Arrived(d) => Self::Arrived {
                destination: d.into(),
            },
            TravelIntent::Cancelled => Self::Cancelled {},
        }
    }
}
impl TravelDto {
    pub fn into_core(self, path: &str) -> Result<TravelIntent, BoundaryError> {
        match self {
            Self::None {} => Ok(TravelIntent::None),
            Self::Active { destination } => Ok(TravelIntent::Active(
                destination.into_core(&format!("{path}.active.destination"))?,
            )),
            Self::Arrived { destination } => Ok(TravelIntent::Arrived(
                destination.into_core(&format!("{path}.arrived.destination"))?,
            )),
            Self::Cancelled {} => Ok(TravelIntent::Cancelled),
        }
    }
}

// Exhaustive matches force new core phases to receive an explicit wire mapping.
macro_rules! phases {
    ($dto:ident, $core:ident, [$($phase:ident),+]) => {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $dto { $($phase),+ }
        impl From<&$core> for $dto {
            fn from(value: &$core) -> Self { match value { $($core::$phase => Self::$phase),+ } }
        }
        impl From<$dto> for $core {
            fn from(value: $dto) -> Self { match value { $($dto::$phase => Self::$phase),+ } }
        }
    };
}

phases!(
    WorkPhaseDto,
    WorkPhase,
    [
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
        Dragging
    ]
);
phases!(
    TransportPhaseDto,
    TransportPhase,
    [ToPickup, PickingUp, ToDropoff, DroppingOff, Returning]
);
phases!(
    ConstructionPhaseDto,
    ConstructionPhase,
    [Outbound, Building]
);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskDto {
    Idle {},
    Move {
        destination: PositionDto,
    },
    Work {
        workplace: EntityIdDto,
        target: DestinationDto,
        phase: WorkPhaseDto,
        elapsed: Number,
    },
    Transport {
        source: EntityIdDto,
        destination: EntityIdDto,
        phase: TransportPhaseDto,
    },
    Construction {
        site: EntityIdDto,
        phase: ConstructionPhaseDto,
        elapsed: Number,
    },
    RoadConstruction {
        x: i32,
        y: i32,
        phase: ConstructionPhaseDto,
    },
}

impl From<&Task> for TaskDto {
    fn from(task: &Task) -> Self {
        match task {
            Task::Idle => Self::Idle {},
            Task::Move { destination } => Self::Move {
                destination: (*destination).into(),
            },
            Task::Work {
                workplace,
                target,
                phase,
                elapsed,
            } => Self::Work {
                workplace: EntityIdDto::from_id(workplace),
                target: target.into(),
                phase: phase.into(),
                elapsed: Number::core(elapsed.get()),
            },
            Task::Transport {
                source,
                destination,
                phase,
            } => Self::Transport {
                source: EntityIdDto::from_id(source),
                destination: EntityIdDto::from_id(destination),
                phase: phase.into(),
            },
            Task::Construction {
                site,
                phase,
                elapsed,
            } => Self::Construction {
                site: EntityIdDto::from_id(site),
                phase: phase.into(),
                elapsed: Number::core(elapsed.get()),
            },
            Task::RoadConstruction { tile, phase } => Self::RoadConstruction {
                x: tile.x,
                y: tile.y,
                phase: phase.into(),
            },
        }
    }
}

fn seconds(value: Number, path: &str) -> Result<Seconds, BoundaryError> {
    Seconds::new(value.get()).map_err(|e| BoundaryError::at(path, ErrorKind::Unit(e)))
}

impl TaskDto {
    pub fn into_core(self, path: &str) -> Result<Task, BoundaryError> {
        Ok(match self {
            Self::Idle {} => Task::Idle,
            Self::Move { destination } => Task::Move {
                destination: destination.into_core(&format!("{path}.move.destination"))?,
            },
            Self::Work {
                workplace,
                target,
                phase,
                elapsed,
            } => Task::Work {
                workplace: workplace.into_id(&format!("{path}.work.workplace"))?,
                target: target.into_core(&format!("{path}.work.target"))?,
                phase: phase.into(),
                elapsed: seconds(elapsed, &format!("{path}.work.elapsed"))?,
            },
            Self::Transport {
                source,
                destination,
                phase,
            } => Task::Transport {
                source: source.into_id(&format!("{path}.transport.source"))?,
                destination: destination.into_id(&format!("{path}.transport.destination"))?,
                phase: phase.into(),
            },
            Self::Construction {
                site,
                phase,
                elapsed,
            } => Task::Construction {
                site: site.into_id(&format!("{path}.construction.site"))?,
                phase: phase.into(),
                elapsed: seconds(elapsed, &format!("{path}.construction.elapsed"))?,
            },
            Self::RoadConstruction { x, y, phase } => Task::RoadConstruction {
                tile: Tile { x, y },
                phase: phase.into(),
            },
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ConstructionDto {
    Complete {},
    InProgress { remaining: Number },
}

impl From<&ConstructionState> for ConstructionDto {
    fn from(state: &ConstructionState) -> Self {
        match state {
            ConstructionState::Complete => Self::Complete {},
            ConstructionState::InProgress { remaining } => Self::InProgress {
                remaining: Number::core(remaining.get()),
            },
        }
    }
}

impl ConstructionDto {
    pub fn into_core(self, path: &str) -> Result<ConstructionState, BoundaryError> {
        match self {
            Self::Complete {} => Ok(ConstructionState::Complete),
            Self::InProgress { remaining } => Work::new(remaining.get())
                .map(|remaining| ConstructionState::InProgress { remaining })
                .map_err(|e| {
                    BoundaryError::at(format!("{path}.in_progress.remaining"), ErrorKind::Unit(e))
                }),
        }
    }
}

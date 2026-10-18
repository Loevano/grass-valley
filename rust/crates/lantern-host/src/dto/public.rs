use super::{
    facts::{ConstructionDto, EntityIdDto, TaskDto},
    wire::{Decimal, Number},
};
use crate::state::ModelWorld;
use lantern_core::model::Task;
use lantern_core::travel::{Playback, Status};
use serde::Serialize;
use std::collections::BTreeMap;

/// Deliberate allowlist, Serialize only. Not accepted as authoritative save data.
#[derive(Serialize)]
pub struct PublicWorldDto {
    format: &'static str,
    world_id: String,
    seed: Decimal,
    towns: BTreeMap<String, PublicTownDto>,
}

#[derive(Serialize)]
struct PublicTownDto {
    buildings: Vec<PublicBuildingDto>,
    villagers: Vec<PublicActorDto>,
}

#[derive(Serialize)]
struct PublicBuildingDto {
    id: EntityIdDto,
    x: i32,
    y: i32,
    construction: ConstructionDto,
}

#[derive(Serialize)]
struct PublicActorDto {
    id: EntityIdDto,
    x: Number,
    y: Number,
    facing: i8,
    playback: PlaybackDto,
    pending_output: BTreeMap<String, Number>,
    task: TaskDto,
    route_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    building_index: Option<usize>,
}

#[derive(Serialize)]
struct PlaybackDto {
    duration: Number,
    samples: Vec<SampleDto>,
}
#[derive(Serialize)]
struct SampleDto {
    time: Number,
    x: Number,
    y: Number,
    facing: i8,
    phase: &'static str,
}
fn phase(status: Status) -> &'static str {
    match status {
        Status::NotRequested => "not_requested",
        Status::Waiting => "waiting",
        Status::Walking => "walking",
        Status::Arrived => "arrived",
        Status::Unreachable => "unreachable",
        Status::Blocked => "blocked",
        Status::Cancelled => "cancelled",
    }
}
impl From<Playback> for PlaybackDto {
    fn from(value: Playback) -> Self {
        Self {
            duration: Number::core(value.duration().get()),
            samples: value
                .samples()
                .iter()
                .map(|s| SampleDto {
                    time: Number::core(s.time.get()),
                    x: Number::core(s.pose.position.x.get()),
                    y: Number::core(s.pose.position.y.get()),
                    facing: s.pose.facing.get(),
                    phase: phase(s.pose.status),
                })
                .collect(),
        }
    }
}

pub fn public_snapshot(world: &ModelWorld) -> PublicWorldDto {
    PublicWorldDto {
        format: "lantern-model-observation-v3",
        world_id: world.id.as_str().into(),
        seed: Decimal(world.seed.0),
        towns: world
            .towns
            .iter()
            .map(|town| {
                let buildings = town
                    .buildings()
                    .iter()
                    .map(|b| PublicBuildingDto {
                        id: EntityIdDto::from_id(&b.id),
                        x: b.location.x,
                        y: b.location.y,
                        construction: (&b.construction).into(),
                    })
                    .collect();
                let villagers = town
                    .actors()
                    .iter()
                    .map(|actor| {
                        let a = actor.state();
                        let workplace = match &a.task {
                            Task::Work { workplace, .. } => Some(workplace),
                            Task::Construction { site, .. } => Some(site),
                            Task::Idle
                            | Task::Move { .. }
                            | Task::Transport { .. }
                            | Task::RoadConstruction { .. } => None,
                        };
                        let route_status = match actor.travel_status() {
                            Status::NotRequested => "not_requested",
                            Status::Waiting => "pending",
                            Status::Walking => "ready",
                            Status::Arrived => "arrived",
                            Status::Unreachable => "unreachable",
                            Status::Blocked => "blocked",
                            Status::Cancelled => "cancelled",
                        };
                        PublicActorDto {
                            id: EntityIdDto::from_id(&a.id),
                            x: Number::core(a.position.x.get()),
                            y: Number::core(a.position.y.get()),
                            facing: a.facing.get(),
                            playback: actor.confirmed_playback().into(),
                            pending_output: a
                                .cargo
                                .goods()
                                .iter()
                                .map(|(k, v)| (k.clone(), Number::core(v.get())))
                                .collect(),
                            task: (&a.task).into(),
                            route_status,
                            building_index: workplace
                                .and_then(|id| town.buildings().iter().position(|b| &b.id == id)),
                        }
                    })
                    .collect();
                (
                    town.owner().town.as_str().into(),
                    PublicTownDto {
                        buildings,
                        villagers,
                    },
                )
            })
            .collect(),
    }
}

//! Immutable evidence of an already-confirmed R11 observation. Never installs a
//! route, query, cursor or playback into a restored authoritative actor.
use crate::{
    BoundaryError,
    building::Rules,
    dto::facts::{EntityIdDto, TaskDto, TravelDto},
    progression::Worker,
};
use lantern_core::{
    ids::ActorId,
    model::{Actor, Destination, Task, Town, TravelIntent, WorkPhase},
    travel::Status,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn is_false(value: &bool) -> bool {
    !*value
}
fn nullable_bool<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(false))
}
fn invalid() -> BoundaryError {
    BoundaryError::invalid("$.work_observation", "worker observation coverage")
}
#[derive(Clone, Default)]
pub struct Observations(BTreeMap<ActorId, Observation>);
#[derive(Clone)]
struct Observation {
    task: Task,
    travel: TravelIntent,
    task_generation: u64,
    route_generation: u64,
    frame: Frame,
}
/// Non-authoritative presentation counters; task, geometry, route and samples
/// always come from the native actor, never from a supplied JSON frame.
pub struct Labels {
    pub target_kind: String,
    pub version: u64,
    pub relocation: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x: f64,
    y: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    time: f64,
    x: f64,
    y: f64,
    phase: String,
    elapsed: f64,
    facing: i8,
    #[serde(default, rename = "waterReady", deserialize_with = "nullable_bool")]
    water_ready: bool,
    #[serde(default, rename = "waterSource")]
    water_source: Option<Value>,
    #[serde(default, rename = "farmAction")]
    farm_action: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    task: Vec<Value>,
    x: f64,
    y: f64,
    phase: String,
    elapsed: f64,
    facing: i8,
    path: Vec<Point>,
    playback: Vec<Sample>,
    #[serde(default)]
    version: u64,
    #[serde(default)]
    relocation: u64,
    #[serde(default, rename = "waterReady", deserialize_with = "nullable_bool")]
    water_ready: bool,
    #[serde(default, rename = "waterSource")]
    water_source: Option<Value>,
    #[serde(default, rename = "farmAction")]
    farm_action: Option<String>,
    #[serde(
        default,
        deserialize_with = "nullable_bool",
        skip_serializing_if = "is_false"
    )]
    route_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resume_phase: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    actor: EntityIdDto,
    task_generation: crate::dto::wire::Decimal,
    route_generation: crate::dto::wire::Decimal,
    task: TaskDto,
    travel: TravelDto,
    frame: Frame,
}
fn phase(p: WorkPhase) -> Result<&'static str, BoundaryError> {
    Ok(match p {
        WorkPhase::Outbound => "walking",
        WorkPhase::Working => "working",
        WorkPhase::Returning => "returning",
        WorkPhase::Depositing => "depositing",
        // Feature-specific phase/action/cargo adapters remain 1i–1j.
        _ => return Err(invalid()),
    })
}
fn observed_phase(status: Status, base: &str) -> &str {
    match status {
        Status::Waiting | Status::Unreachable | Status::Blocked => "waiting",
        _ => base,
    }
}
impl Observations {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn capture(
        &mut self,
        town: &Town,
        id: &ActorId,
        worker: &Worker,
        rules: &Rules,
        labels: Labels,
    ) -> Result<(), BoundaryError> {
        let actor = town.actor(id).map_err(|_| invalid())?;
        let state = actor.state();
        let Task::Work {
            workplace,
            target,
            phase: work_phase,
            elapsed,
        } = &state.task
        else {
            return Err(invalid());
        };
        let site = rules.sites.get(workplace).ok_or_else(invalid)?;
        let home = town.building(workplace).map_err(|_| invalid())?.location;
        let target = match target {
            Destination::Building(id) => {
                if rules.sites.get(id).ok_or_else(invalid)?.kind != labels.target_kind {
                    return Err(invalid());
                }
                let t = town.building(id).map_err(|_| invalid())?.location;
                [f64::from(t.x), f64::from(t.y)]
            }
            Destination::Position(p) => [p.x.get(), p.y.get()],
        };
        if !state.cargo.goods().is_empty() || labels.target_kind.is_empty() {
            return Err(invalid());
        }
        let base = phase(*work_phase)?;
        let status = actor.travel_status();
        if status == Status::Walking
            && !matches!(work_phase, WorkPhase::Outbound | WorkPhase::Returning)
        {
            return Err(invalid());
        }
        let waiting = observed_phase(status, base) == "waiting";
        let playback = actor.confirmed_playback();
        let mut frame = Frame {
            task: vec![
                json!(worker.job()),
                json!(site.kind),
                json!(home.x),
                json!(home.y),
                json!(labels.target_kind),
                json!(target[0]),
                json!(target[1]),
            ],
            x: state.position.x.get(),
            y: state.position.y.get(),
            facing: state.facing.get(),
            phase: observed_phase(status, base).into(),
            elapsed: elapsed.get(),
            version: labels.version,
            relocation: labels.relocation,
            path: actor
                .remaining_waypoints()
                .iter()
                .map(|p| Point {
                    x: p.x.get(),
                    y: p.y.get(),
                })
                .collect(),
            playback: playback
                .samples()
                .iter()
                .map(|s| Sample {
                    time: s.time.get(),
                    x: s.pose.position.x.get(),
                    y: s.pose.position.y.get(),
                    facing: s.pose.facing.get(),
                    phase: observed_phase(s.pose.status, base).into(),
                    elapsed: elapsed.get(),
                    water_ready: false,
                    water_source: None,
                    farm_action: None,
                })
                .collect(),
            water_ready: false,
            water_source: None,
            farm_action: None,
            route_pending: status == Status::Waiting,
            resume_phase: waiting.then(|| base.into()),
        };
        frame.normalize()?;
        frame.validate()?;
        if self.0.contains_key(id) {
            return Err(invalid());
        }
        self.0.insert(
            id.clone(),
            Observation {
                task: state.task.clone(),
                travel: state.travel.clone(),
                task_generation: state.task_generation.get(),
                route_generation: state.route_generation.get(),
                frame,
            },
        );
        Ok(())
    }
    /// Compare the independent observation to the current native intent, or to
    /// R12's retained suspended intent. A helper's current construction pose is
    /// checked by the construction adapter, not overwritten by its old work pose.
    pub(crate) fn check(
        &self,
        actor: &Actor,
        task: &Task,
        travel: &TravelIntent,
        suspended: bool,
        public: Option<&[u8]>,
    ) -> Result<bool, BoundaryError> {
        let saved = self.0.get(&actor.state().id);
        match (saved, public) {
            (None, None) if !matches!(task, Task::Work { .. }) => Ok(false),
            (Some(o), Some(bytes)) => {
                if &o.task != task || &o.travel != travel {
                    return Err(invalid());
                }
                if !suspended
                    && (o.task_generation != actor.state().task_generation.get()
                        || o.route_generation != actor.state().route_generation.get()
                        || o.frame.x != actor.state().position.x.get()
                        || o.frame.y != actor.state().position.y.get()
                        || o.frame.facing != actor.state().facing.get())
                {
                    return Err(invalid());
                }
                let mut frame: Frame = serde_json::from_slice(bytes).map_err(|_| invalid())?;
                frame.normalize()?;
                frame.validate()?;
                if frame != o.frame {
                    return Err(invalid());
                }
                Ok(!suspended)
            }
            _ => Err(invalid()),
        }
    }
    pub fn frame(&self, id: &ActorId) -> Result<Value, BoundaryError> {
        serde_json::to_value(&self.0.get(id).ok_or_else(invalid)?.frame).map_err(|_| invalid())
    }
    pub(crate) fn encode(&self) -> Result<String, BoundaryError> {
        serde_json::to_string(&Saved {
            version: 1,
            entries: self
                .0
                .iter()
                .map(|(id, o)| Entry {
                    actor: EntityIdDto::from_id(id),
                    task_generation: crate::dto::wire::Decimal(o.task_generation),
                    route_generation: crate::dto::wire::Decimal(o.route_generation),
                    task: (&o.task).into(),
                    travel: (&o.travel).into(),
                    frame: o.frame.clone(),
                })
                .collect(),
        })
        .map_err(|_| invalid())
    }
    pub(crate) fn decode(text: &str, town: &Town) -> Result<Self, BoundaryError> {
        let saved: Saved = serde_json::from_str(text).map_err(|_| invalid())?;
        if saved.version != 1 {
            return Err(invalid());
        }
        let mut result = Self::default();
        for e in saved.entries {
            let id = e.actor.into_id("$.work_observation.actor")?;
            town.actor(&id).map_err(|_| invalid())?;
            let task = e.task.into_core("$.work_observation.task")?;
            town.validate_task(&task).map_err(|_| invalid())?;
            if !matches!(task, Task::Work { .. }) {
                return Err(invalid());
            }
            e.frame.validate()?;
            if let Task::Work { elapsed, .. } = &task
                && e.frame.elapsed != elapsed.get()
            {
                return Err(invalid());
            }
            if result
                .0
                .insert(
                    id,
                    Observation {
                        task,
                        task_generation: e.task_generation.0,
                        route_generation: e.route_generation.0,
                        travel: e.travel.into_core("$.work_observation.travel")?,
                        frame: e.frame,
                    },
                )
                .is_some()
            {
                return Err(invalid());
            }
        }
        Ok(result)
    }
}
impl Frame {
    fn normalize(&mut self) -> Result<(), BoundaryError> {
        for i in [2, 3, 5, 6] {
            let n = self
                .task
                .get(i)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(invalid)?;
            self.task[i] = json!(n);
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), BoundaryError> {
        if !["walking", "working", "returning", "depositing", "waiting"]
            .contains(&self.phase.as_str())
            || self.task.len() != 7
            || [0, 1, 4]
                .iter()
                .any(|i| self.task[*i].as_str().is_none_or(str::is_empty))
            || self.water_ready
            || self.water_source.is_some()
            || self.farm_action.is_some()
            || !self.x.is_finite()
            || !self.y.is_finite()
            || !self.elapsed.is_finite()
            || self.elapsed < 0.
            || ![-1, 1].contains(&self.facing)
            || self.playback.is_empty()
            || self
                .path
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(invalid());
        }
        let mut time = 0.;
        for s in &self.playback {
            if !["walking", "working", "returning", "depositing", "waiting"]
                .contains(&s.phase.as_str())
                || !s.time.is_finite()
                || s.time < time
                || !s.x.is_finite()
                || !s.y.is_finite()
                || !s.elapsed.is_finite()
                || s.elapsed < 0.
                || ![-1, 1].contains(&s.facing)
                || s.water_ready
                || s.water_source.is_some()
                || s.farm_action.is_some()
            {
                return Err(invalid());
            }
            time = s.time;
        }
        let last = self.playback.last().unwrap();
        if self.playback[0].time != 0.
            || last.x != self.x
            || last.y != self.y
            || last.facing != self.facing
            || last.phase != self.phase
            || last.elapsed != self.elapsed
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lantern_core::{
        ids::{IdAllocator, Owner, TownId, WorldId},
        model::{
            ActorState, Building, Cargo, ConstructionState, NavigationRevision, RouteOutcome,
            RouteState,
        },
        travel::Speed,
        units::{Position, Quantity, Seconds, Tile},
    };
    fn fixture() -> (Town, ActorId, Rules) {
        let owner = Owner {
            world: WorldId::new("observations").unwrap(),
            town: TownId::new("town").unwrap(),
        };
        let mut ids = IdAllocator::new(owner.world.clone());
        let building = ids.allocate(&owner).unwrap();
        let id = ids.allocate(&owner).unwrap();
        let mut state =
            ActorState::new(id.clone(), Position::new(1., 2.).unwrap(), Cargo::default());
        state.task = Task::Work {
            workplace: building.clone(),
            target: Destination::Building(building.clone()),
            phase: WorkPhase::Outbound,
            elapsed: Seconds::new(0.125).unwrap(),
        };
        let mut town = Town::restore(
            owner,
            vec![Building {
                id: building.clone(),
                location: Tile { x: 4, y: 2 },
                construction: ConstructionState::Complete,
            }],
            vec![state],
        )
        .unwrap();
        let request = town
            .request_route(
                &id,
                Destination::Building(building.clone()),
                NavigationRevision(1),
            )
            .unwrap();
        town.accept_route(
            &id,
            &request,
            NavigationRevision(1),
            RouteOutcome::Ready(vec![
                Position::new(2., 2.).unwrap(),
                Position::new(3., 2.).unwrap(),
            ]),
        )
        .unwrap();
        town.advance_travel(&id, Seconds::new(0.75).unwrap(), 16, |_, _| Speed::new(1.8))
            .unwrap();
        let rules = Rules {
            level: 1,
            sandbox: false,
            origin: Tile { x: 0, y: 0 },
            player: Position::new(0., 0.).unwrap(),
            coins: Quantity::new(0.).unwrap(),
            sites: BTreeMap::from([(building, crate::building::Site::new("sawpit", (1, 1)))]),
            roads: vec![],
            progression: Default::default(),
        };
        (town, id, rules)
    }
    fn captured(town: &Town, id: &ActorId, rules: &Rules) -> Observations {
        let mut observations = Observations::default();
        observations
            .capture(
                town,
                id,
                &Worker::new("woodworking").unwrap(),
                rules,
                Labels {
                    target_kind: "sawpit".into(),
                    version: 4,
                    relocation: 2,
                },
            )
            .unwrap();
        observations
    }
    #[test]
    fn detached_checkpoint_survives_reload_without_restoring_navigation() {
        let (town, id, rules) = fixture();
        let observations = captured(&town, &id, &rules);
        let frame = observations.frame(&id).unwrap();
        // The first waypoint was consumed; the evidence retains only what the
        // browser needs, not the search request, generations or route cursor.
        assert_eq!(frame["path"].as_array().unwrap().len(), 1);
        assert_eq!(frame["playback"].as_array().unwrap().len(), 3);
        let reloaded = Town::restore(
            town.owner().clone(),
            town.buildings().to_vec(),
            town.actors().iter().map(|a| a.state().clone()).collect(),
        )
        .unwrap();
        let actor = reloaded.actor(&id).unwrap();
        assert_eq!(actor.route(), &RouteState::NotRequested);
        assert!(actor.remaining_waypoints().is_empty());
        assert_eq!(actor.confirmed_playback().duration().get(), 0.);
        let restored = Observations::decode(&observations.encode().unwrap(), &reloaded).unwrap();
        assert_eq!(restored.frame(&id).unwrap(), frame);
        assert_eq!(
            restored.check(
                actor,
                &actor.state().task,
                &actor.state().travel,
                false,
                Some(frame.to_string().as_bytes())
            ),
            Ok(true)
        );
        assert!(
            restored
                .check(
                    actor,
                    &Task::Idle,
                    &TravelIntent::None,
                    false,
                    Some(frame.to_string().as_bytes())
                )
                .is_err()
        );
        let mut changed = reloaded.clone();
        let destination = changed
            .actor(&id)
            .unwrap()
            .travel_destination()
            .unwrap()
            .clone();
        changed
            .request_route(&id, destination, NavigationRevision(2))
            .unwrap();
        let actor = changed.actor(&id).unwrap();
        assert!(
            restored
                .check(
                    actor,
                    &actor.state().task,
                    &actor.state().travel,
                    false,
                    Some(frame.to_string().as_bytes())
                )
                .is_err()
        );
    }
    #[test]
    fn corrupt_duplicate_foreign_unknown_and_nonfinite_observations_refuse() {
        let (town, id, rules) = fixture();
        let observations = captured(&town, &id, &rules);
        let saved: Value = serde_json::from_str(&observations.encode().unwrap()).unwrap();
        for case in 0..6 {
            let mut value = saved.clone();
            match case {
                0 => value["version"] = json!(2),
                1 => {
                    let e = value["entries"][0].clone();
                    value["entries"].as_array_mut().unwrap().push(e);
                }
                2 => value["entries"][0]["actor"]["town_id"] = json!("foreign"),
                3 => value["entries"][0]["frame"]["credentials"] = json!("PRIVATE_CANARY"),
                4 => value["entries"][0]["frame"]["playback"][0]["time"] = json!(-1),
                _ => value["entries"][0]["frame"]["x"] = Value::Null,
            }
            assert!(Observations::decode(&value.to_string(), &town).is_err());
        }
        let before = observations.encode().unwrap();
        let mut copy = observations.clone();
        assert!(
            copy.capture(
                &town,
                &id,
                &Worker::new("woodworking").unwrap(),
                &rules,
                Labels {
                    target_kind: "sawpit".into(),
                    version: 5,
                    relocation: 0
                }
            )
            .is_err()
        );
        assert_eq!(copy.encode().unwrap(), before);
    }
}

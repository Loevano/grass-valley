use super::*;
use crate::dto::facts::{DestinationDto, EntityIdDto};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    trees: Vec<TreeDto>,
    jobs: Vec<JobDto>,
    assignments: Vec<(EntityIdDto, EntityIdDto)>,
    previous: Vec<(EntityIdDto, EntityIdDto, EntityIdDto)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeDto {
    id: EntityIdDto,
    orchard: EntityIdDto,
    stage: String,
    elapsed: f64,
    fruit: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobDto {
    actor: EntityIdDto,
    orchard: EntityIdDto,
    tree: EntityIdDto,
    target: DestinationDto,
    clock: f64,
    #[serde(deserialize_with = "required_option")]
    practice_start: Option<f64>,
    practice_seconds: f64,
    #[serde(deserialize_with = "required_option")]
    duration: Option<f64>,
}
fn id<K>(d: EntityIdDto) -> Result<lantern_core::ids::PersistentId<K>, Error> {
    d.into_id("$.orchards").map_err(|_| Error::Invalid)
}
fn dt<K>(d: &lantern_core::ids::PersistentId<K>) -> EntityIdDto {
    EntityIdDto::from_id(d)
}
fn stage(s: Stage) -> &'static str {
    match s {
        Stage::Sapling => "sapling",
        Stage::Blossom => "blossom",
        Stage::Fruit => "fruit",
        Stage::Ripe => "ripe",
        Stage::Harvested => "harvested",
    }
}
fn parse(s: &str) -> Result<Stage, Error> {
    match s {
        "sapling" => Ok(Stage::Sapling),
        "blossom" => Ok(Stage::Blossom),
        "fruit" => Ok(Stage::Fruit),
        "ripe" => Ok(Stage::Ripe),
        "harvested" => Ok(Stage::Harvested),
        _ => Err(Error::Invalid),
    }
}
impl Orchards {
    pub fn encode(&self, town: &Town, ledger: &Ledger) -> Result<String, Error> {
        let s = Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            trees: self
                .trees
                .iter()
                .map(|t| TreeDto {
                    id: dt(&t.id),
                    orchard: dt(&t.orchard),
                    stage: stage(t.stage).into(),
                    elapsed: t.elapsed.get(),
                    fruit: t.fruit.get(),
                })
                .collect(),
            jobs: self
                .jobs
                .iter()
                .map(|(a, j)| JobDto {
                    actor: dt(a),
                    orchard: dt(&j.orchard),
                    tree: dt(&j.tree),
                    target: (&j.target).into(),
                    clock: j.clock,
                    practice_start: j.practice_start,
                    practice_seconds: j.practice_seconds,
                    duration: j.duration,
                })
                .collect(),
            previous: self
                .previous
                .iter()
                .map(|(a, (o, t))| (dt(a), dt(o), dt(t)))
                .collect(),
            assignments: self
                .assignments
                .iter()
                .map(|(a, b)| (dt(a), dt(b)))
                .collect(),
        };
        let json = serde_json::to_string(&s).map_err(|_| Error::Invalid)?;
        Self::decode(&json, town, ledger)?;
        Ok(json)
    }
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let s: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        let trees = s
            .trees
            .into_iter()
            .map(|t| {
                Ok(Tree {
                    id: id(t.id)?,
                    orchard: id(t.orchard)?,
                    stage: parse(&t.stage)?,
                    elapsed: Seconds::new(t.elapsed).map_err(|_| Error::Invalid)?,
                    fruit: Quantity::new(t.fruit).map_err(|_| Error::Invalid)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut assignments = BTreeMap::new();
        for (a, b) in s.assignments {
            if assignments.insert(id(a)?, id(b)?).is_some() {
                return Err(Error::Invalid);
            }
        }
        let mut result = Self::new(town, ledger, trees, assignments)?;
        for (a, o, t) in s.previous {
            let a = id(a)?;
            let o: BuildingId = id(o)?;
            let t: BuildingId = id(t)?;
            town.actor(&a)?;
            o.identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
            t.identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
            if result.previous.insert(a, (o, t)).is_some() {
                return Err(Error::Invalid);
            }
        }
        let mut claimed = BTreeSet::new();
        for j in s.jobs {
            let a = id(j.actor)?;
            town.actor(&a)?;
            let orchard = id(j.orchard)?;
            town.building(&orchard)?;
            let tree: BuildingId = id(j.tree)?;
            tree.identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
            let job = Job {
                orchard,
                tree,
                target: j
                    .target
                    .into_core("$.orchards.target")
                    .map_err(|_| Error::Invalid)?,
                clock: j.clock,
                practice_start: j.practice_start,
                practice_seconds: j.practice_seconds,
                duration: j.duration,
            };
            if ledger.store(&job.orchard)?.policy.kind != "orchard"
                || !Self::matches(town, &a, &job)
                || !result.assignments.contains_key(&a)
                || !job.clock.is_finite()
                || job.clock < 0.
                || job.duration.is_some_and(|d| !d.is_finite() || d <= 0.)
                || job.clock > job.duration.unwrap_or(0.65).max(0.65)
                || !job.practice_seconds.is_finite()
                || !(0. ..=job.duration.unwrap_or(0.) + 1e-9).contains(&job.practice_seconds)
                || job.practice_start.is_some_and(|n| !n.is_finite() || n < 0.)
                || (job.practice_seconds > 0. && job.practice_start.is_none())
                || ledger.held().contains_key(&a)
                || !town.actor(&a)?.state().cargo.goods().is_empty()
            {
                return Err(Error::Invalid);
            }
            let p = phase(town, &a)?;
            let goal = match p {
                WorkPhase::Outbound => Some(job.target.clone()),
                WorkPhase::Returning => Some(Destination::Building(job.orchard.clone())),
                _ => None,
            };
            if town.actor(&a)?.state().travel.destination() != goal.as_ref()
                || (matches!(p, WorkPhase::Outbound | WorkPhase::Returning) && job.clock != 0.)
                || (p == WorkPhase::Depositing && job.clock > 0.65)
                || (matches!(p, WorkPhase::Returning | WorkPhase::Depositing)
                    && !pending(ledger, &job.orchard, &a)?)
            {
                return Err(Error::Invalid);
            }
            if matches!(p, WorkPhase::Outbound | WorkPhase::Working) {
                let t = result.tree(&job.tree).ok_or(Error::Invalid)?;
                let tile = town.building(&job.tree)?.location;
                let expected = Destination::Position(
                    Position::new(f64::from(tile.x), f64::from(tile.y)).unwrap(),
                );
                if job.target != expected
                    || t.orchard != job.orchard
                    || t.stage != Stage::Ripe
                    || !claimed.insert(job.tree.clone())
                {
                    return Err(Error::Invalid);
                }
            }
            if result.jobs.insert(a, job).is_some() {
                return Err(Error::Invalid);
            }
        }
        Ok(result)
    }
    pub fn reconcile_relocation(
        &mut self,
        before: &Town,
        after: &mut Town,
        ledger: &Ledger,
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        if before.owner() != after.owner() {
            return Err(Error::Invalid);
        }
        let (mut owner, mut model) = (self.clone(), after.clone());
        for (a, j) in self.jobs.clone() {
            if !Self::matches(&model, &a, &j) {
                continue;
            }
            let home =
                before.building(&j.orchard)?.location != model.building(&j.orchard)?.location;
            let source = match (before.building(&j.tree), model.building(&j.tree)) {
                (Ok(b), Ok(c)) => b.location != c.location,
                _ => false,
            };
            if !(home || source) {
                continue;
            }
            let p = phase(&model, &a)?;
            let mut next = j.clone();
            if source {
                let t = model.building(&j.tree)?.location;
                next.target =
                    Destination::Position(Position::new(f64::from(t.x), f64::from(t.y)).unwrap());
            }
            let returning = matches!(p, WorkPhase::Returning | WorkPhase::Depositing);
            if (source && !returning) || (home && returning) {
                let p = if returning || pending(ledger, &j.orchard, &a)? {
                    WorkPhase::Returning
                } else {
                    WorkPhase::Outbound
                };
                next.clock = 0.;
                next.practice_start = None;
                next.practice_seconds = 0.;
                next.duration = None;
                model.set_task(&a, task(&next, p))?;
                model.request_route(
                    &a,
                    if p == WorkPhase::Returning {
                        Destination::Building(j.orchard.clone())
                    } else {
                        next.target.clone()
                    },
                    nav,
                )?;
            } else {
                let travel = model.actor(&a)?.state().travel.clone();
                model.restore_task_intent(&a, task(&next, p), travel)?;
            }
            owner.jobs.insert(a, next);
        }
        *self = owner;
        *after = model;
        Ok(())
    }
    pub fn admit_legacy(
        town: &Town,
        ledger: &Ledger,
        buildings: &[serde_json::Value],
        ids: &[BuildingId],
        assignments: BTreeMap<ActorId, BuildingId>,
    ) -> Result<Self, Error> {
        if buildings.len() != ids.len() {
            return Err(Error::Invalid);
        }
        let mut trees = Vec::new();
        let mut seen = BTreeSet::new();
        for (b, id) in buildings.iter().zip(ids) {
            town.building(id)?;
            if !seen.insert(id.clone())
                || b["kind"].as_str() != Some(ledger.store(id)?.policy.kind.as_str())
            {
                return Err(Error::Invalid);
            }
            if b["kind"] != "orchard_tree" {
                continue;
            }
            let parent = b["orchard_index"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| ids.get(n))
                .ok_or(Error::Invalid)?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Legacy {
                stage: String,
                elapsed: f64,
                fruit: f64,
            }
            let s = if let Some(v) = b.get("tree") {
                serde_json::from_value::<Legacy>(v.clone()).map_err(|_| Error::Invalid)?
            } else {
                Legacy {
                    stage: "sapling".into(),
                    elapsed: 0.,
                    fruit: 8.,
                }
            };
            trees.push(Tree {
                id: id.clone(),
                orchard: parent.clone(),
                stage: parse(&s.stage)?,
                elapsed: Seconds::new(s.elapsed).map_err(|_| Error::Invalid)?,
                fruit: Quantity::new(s.fruit).map_err(|_| Error::Invalid)?,
            });
        }
        Self::new(town, ledger, trees, assignments)
    }
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

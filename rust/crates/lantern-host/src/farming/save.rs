//! Explicit owner DTOs; paths/playback and the model/ledger are saved separately.
use super::*;
use crate::dto::facts::{DestinationDto, EntityIdDto};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    fields: Vec<FieldDto>,
    claims: Vec<ClaimDto>,
    previous: Vec<ClaimDto>,
    assignments: Vec<(EntityIdDto, EntityIdDto)>,
    rounds: Vec<(EntityIdDto, EntityIdDto, u64)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldDto {
    id: EntityIdDto,
    #[serde(deserialize_with = "required_option")]
    parent: Option<EntityIdDto>,
    selected: String,
    stage: String,
    elapsed: f64,
    progress: f64,
    cycle: u64,
    #[serde(deserialize_with = "required_option")]
    crop: Option<String>,
    #[serde(deserialize_with = "required_option")]
    yield_remaining: Option<f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimDto {
    actor: EntityIdDto,
    workplace: EntityIdDto,
    field: EntityIdDto,
    destination: DestinationDto,
    #[serde(deserialize_with = "required_option")]
    practice_start: Option<f64>,
    practice_seconds: f64,
    clock: f64,
    #[serde(deserialize_with = "required_option")]
    water: Option<BankDto>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BankDto {
    #[serde(deserialize_with = "required_option")]
    well: Option<EntityIdDto>,
    source: [i32; 2],
    bank: [i32; 2],
}
fn stage(s: Stage) -> &'static str {
    match s {
        Stage::Unplowed => "unplowed",
        Stage::Plowed => "plowed",
        Stage::Sprouting => "sprouting",
        Stage::Thirsty => "thirsty",
        Stage::Growing => "growing",
        Stage::Ripe => "ripe",
    }
}
pub(super) fn parse_stage(s: &str) -> Result<Stage, Error> {
    match s {
        "unplowed" => Ok(Stage::Unplowed),
        "plowed" => Ok(Stage::Plowed),
        "sprouting" => Ok(Stage::Sprouting),
        "thirsty" => Ok(Stage::Thirsty),
        "growing" => Ok(Stage::Growing),
        "ripe" => Ok(Stage::Ripe),
        _ => Err(Error::Invalid),
    }
}
fn id<K>(dto: EntityIdDto) -> Result<lantern_core::ids::PersistentId<K>, Error> {
    dto.into_id("$.farming").map_err(|_| Error::Invalid)
}
fn dt<K>(id: &lantern_core::ids::PersistentId<K>) -> EntityIdDto {
    EntityIdDto::from_id(id)
}
impl ClaimDto {
    fn new(a: &ActorId, c: &Claim) -> Self {
        Self {
            actor: dt(a),
            workplace: dt(&c.workplace),
            field: dt(&c.field),
            destination: (&c.destination).into(),
            practice_start: c.practice_start,
            practice_seconds: c.practice_seconds,
            clock: c.clock,
            water: c.water.as_ref().map(|b| {
                let (well, tile) = match &b.source {
                    WaterSource::Surface(t) => (None, *t),
                    WaterSource::Well { id, tile } => (Some(dt(id)), *tile),
                };
                BankDto {
                    well,
                    source: [tile.x, tile.y],
                    bank: [b.tile.x, b.tile.y],
                }
            }),
        }
    }
    fn into_claim(self, town: &Town) -> Result<(ActorId, Claim), Error> {
        let actor = id(self.actor)?;
        town.actor(&actor)?;
        let workplace = id(self.workplace)?;
        let field: BuildingId = id(self.field)?;
        field
            .identity()
            .validate_owner(town.owner())
            .map_err(|_| Error::Invalid)?;
        town.building(&workplace)?;
        if !self.clock.is_finite()
            || !(0. ..=0.65).contains(&self.clock)
            || !self.practice_seconds.is_finite()
            || !(0. ..=2. + 1e-9).contains(&self.practice_seconds)
            || self
                .practice_start
                .is_some_and(|n| !n.is_finite() || n < 0.)
            || (self.practice_seconds > 0. && self.practice_start.is_none())
        {
            return Err(Error::Invalid);
        }
        let water = self
            .water
            .map(|b| {
                let source = Tile {
                    x: b.source[0],
                    y: b.source[1],
                };
                let tile = Tile {
                    x: b.bank[0],
                    y: b.bank[1],
                };
                if (i64::from(source.x) - i64::from(tile.x)).abs()
                    + (i64::from(source.y) - i64::from(tile.y)).abs()
                    != 1
                {
                    return Err(Error::Invalid);
                }
                let source = if let Some(well) = b.well {
                    let well: BuildingId = id(well)?;
                    if well.identity().owner().world != town.owner().world {
                        return Err(Error::Invalid);
                    }
                    WaterSource::Well {
                        id: well,
                        tile: source,
                    }
                } else {
                    WaterSource::Surface(source)
                };
                Ok(WaterBank { source, tile })
            })
            .transpose()?;
        let destination = self
            .destination
            .into_core("$.farming.destination")
            .map_err(|_| Error::Invalid)?;
        Ok((
            actor,
            Claim {
                workplace,
                field,
                destination,
                practice_start: self.practice_start,
                practice_seconds: self.practice_seconds,
                clock: self.clock,
                water,
            },
        ))
    }
}
impl Fields {
    pub fn encode(&self, town: &Town, ledger: &Ledger) -> Result<String, Error> {
        let saved = Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            fields: self
                .fields
                .iter()
                .map(|f| FieldDto {
                    id: dt(&f.id),
                    parent: f.farm.as_ref().map(dt),
                    selected: f.selected_crop.as_str().into(),
                    stage: stage(f.state.stage).into(),
                    elapsed: f.state.elapsed.get(),
                    progress: f.state.progress.get(),
                    cycle: f.state.cycle,
                    crop: f.state.crop.as_ref().map(|c| c.as_str().into()),
                    yield_remaining: f.state.yield_remaining.map(|n| n.get()),
                })
                .collect(),
            claims: self
                .claims
                .iter()
                .map(|(a, c)| ClaimDto::new(a, c))
                .collect(),
            previous: self
                .previous
                .iter()
                .map(|(a, c)| ClaimDto::new(a, c))
                .collect(),
            assignments: self
                .assignments
                .iter()
                .map(|(a, b)| (dt(a), dt(b)))
                .collect(),
            rounds: self
                .rounds
                .iter()
                .map(|(a, (b, c))| (dt(a), dt(b), *c))
                .collect(),
        };
        let json = serde_json::to_string(&saved).map_err(|_| Error::Invalid)?;
        Self::decode(&json, town, ledger)?;
        Ok(json)
    }
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if saved.version != 1
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        let fields = saved
            .fields
            .into_iter()
            .map(|f| {
                Ok(Field {
                    id: id(f.id)?,
                    farm: f.parent.map(id).transpose()?,
                    selected_crop: ResourceKey::parse(&f.selected)?,
                    state: FieldState {
                        stage: parse_stage(&f.stage)?,
                        elapsed: Seconds::new(f.elapsed).map_err(|_| Error::Invalid)?,
                        progress: Seconds::new(f.progress).map_err(|_| Error::Invalid)?,
                        cycle: f.cycle,
                        crop: f.crop.map(|s| ResourceKey::parse(&s)).transpose()?,
                        yield_remaining: f
                            .yield_remaining
                            .map(|n| Quantity::new(n).map_err(|_| Error::Invalid))
                            .transpose()?,
                    },
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut assignments = BTreeMap::new();
        for (a, b) in saved.assignments {
            if assignments.insert(id(a)?, id(b)?).is_some() {
                return Err(Error::Invalid);
            }
        }
        let mut result = Self::new(town, ledger, fields, assignments)?;
        for (a, b, c) in saved.rounds {
            let a = id(a)?;
            let b = id(b)?;
            town.actor(&a)?;
            result.field(&b)?;
            if result.rounds.insert(a, (b, c)).is_some() {
                return Err(Error::Invalid);
            }
        }
        for c in saved.previous {
            let (a, c) = c.into_claim(town)?;
            if result.previous.insert(a, c).is_some() {
                return Err(Error::Invalid);
            }
        }
        let mut targets = BTreeSet::new();
        for c in saved.claims {
            let (a, c) = c.into_claim(town)?;
            if !Self::matches(town, &a, &c)
                || !result.assignments.contains_key(&a)
                || ledger.held().contains_key(&a)
                || !town.actor(&a)?.state().cargo.goods().is_empty()
            {
                return Err(Error::Invalid);
            }
            let p = phase(town, &a)?;
            let returning = matches!(p, WorkPhase::Returning | WorkPhase::Depositing);
            let field = result.field(&c.field)?;
            if !returning
                && (lifecycle::work_destination(town, ledger, field)? != c.destination
                    || (field.id != c.workplace && field.farm.as_ref() != Some(&c.workplace)))
            {
                return Err(Error::Invalid);
            }
            let Destination::Position(position) = c.destination else {
                return Err(Error::Invalid);
            };
            let tile = town.building(&c.field)?.location;
            if (position.x.get() - f64::from(tile.x)).abs() > 0.5
                || (position.y.get() - f64::from(tile.y)).abs() > 0.5
            {
                return Err(Error::Invalid);
            }
            let goal = match p {
                WorkPhase::Outbound | WorkPhase::ReturningWater => Some(c.destination.clone()),
                WorkPhase::FetchingWater => {
                    Some(c.water.as_ref().ok_or(Error::Invalid)?.destination())
                }
                WorkPhase::Returning => Some(Destination::Building(c.workplace.clone())),
                _ => None,
            };
            if town.actor(&a)?.state().travel.destination() != goal.as_ref()
                || (!matches!(p, WorkPhase::CollectingWater | WorkPhase::Depositing)
                    && c.clock != 0.)
                || (!returning && !targets.insert(c.field.clone()))
                || (returning && pending(ledger, &a)?.as_ref() != Some(&c.workplace))
                || (matches!(
                    p,
                    WorkPhase::FetchingWater
                        | WorkPhase::CollectingWater
                        | WorkPhase::ReturningWater
                ) && field.state.stage != Stage::Thirsty)
                || (matches!(p, WorkPhase::FetchingWater | WorkPhase::CollectingWater)
                    && c.water.is_none())
                || (p == WorkPhase::ReturningWater
                    && !result.water_ready(&a, &c.workplace, field.state.cycle))
                || result.claims.insert(a, c).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        Ok(result)
    }
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

//! Explicit private offline construction schema; no filesystem or HTTP writer.
use super::*;
use crate::{
    BoundaryError,
    dto::{
        facts::{EntityIdDto, PositionDto},
        save::{SaveWorldDto, write_error},
        wire::{Number, UniqueMap},
    },
};
use serde::{Deserialize, Serialize};
mod extensions;
use extensions::Extensions;
#[derive(Serialize)]
#[serde(transparent)]
struct Added<T>(Option<T>);
impl<T> Default for Added<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Added<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(|v| Self(Some(v)))
    }
}
impl<T> Added<T> {
    fn missing(&self) -> bool {
        self.0.is_none()
    }
}

#[derive(Serialize, Deserialize)]
enum Format {
    #[serde(rename = "lantern-building-model")]
    Building,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    format: Format,
    version: u32,
    model: SaveWorldDto,
    shared: bool,
    towns: Vec<TownDto>,
    landscape: LandscapeDto,
    builders: Vec<BuilderDto>,
    #[serde(default, skip_serializing_if = "Added::missing")]
    extensions: Added<Extensions>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TileDto {
    x: i32,
    y: i32,
}
impl From<Tile> for TileDto {
    fn from(t: Tile) -> Self {
        Self { x: t.x, y: t.y }
    }
}
impl From<TileDto> for Tile {
    fn from(t: TileDto) -> Self {
        Self { x: t.x, y: t.y }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FootprintDto {
    origin: TileDto,
    dimensions: [u32; 2],
}
impl From<Footprint> for FootprintDto {
    fn from(f: Footprint) -> Self {
        let (w, d) = f.dimensions();
        Self {
            origin: f.origin().into(),
            dimensions: [w, d],
        }
    }
}
impl FootprintDto {
    fn core(self, budget: &mut u64) -> Result<Footprint, BoundaryError> {
        let [w, d] = self.dimensions;
        *budget = budget
            .checked_sub(u64::from(w) * u64::from(d))
            .ok_or_else(|| invalid("geometry expansion limit"))?;
        Footprint::saved(self.origin.into(), Some((w, d))).map_err(|_| invalid("invalid footprint"))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteDto {
    id: EntityIdDto,
    kind: String,
    dimensions: [u32; 2],
    orientation: u8,
    fence_orientation: u8,
    stored: UniqueMap<Number>,
    pending: UniqueMap<Number>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoadDto {
    tile: TileDto,
    #[serde(deserialize_with = "required_option")]
    remaining: Option<Number>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TownDto {
    town_id: String,
    level: u8,
    sandbox: bool,
    origin: TileDto,
    player: PositionDto,
    coins: Number,
    sites: Vec<SiteDto>,
    roads: Vec<RoadDto>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceDto {
    kind: String,
    footprint: FootprintDto,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LandscapeDto {
    size: u32,
    unavailable: Vec<TileDto>,
    resources: Vec<ResourceDto>,
    locals: Vec<FootprintDto>,
    local_positions: Vec<PositionDto>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RoleDto {
    Dedicated,
    Idle,
    Busy,
}
impl From<Role> for RoleDto {
    fn from(r: Role) -> Self {
        match r {
            Role::Dedicated => Self::Dedicated,
            Role::Idle => Self::Idle,
            Role::Busy => Self::Busy,
        }
    }
}
impl From<RoleDto> for Role {
    fn from(r: RoleDto) -> Self {
        match r {
            RoleDto::Dedicated => Self::Dedicated,
            RoleDto::Idle => Self::Idle,
            RoleDto::Busy => Self::Busy,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ClaimDto {
    Building { id: EntityIdDto },
    Road { tile: TileDto },
}
impl From<&Claim> for ClaimDto {
    fn from(c: &Claim) -> Self {
        match c {
            Claim::Building(id) => Self::Building {
                id: EntityIdDto::from_id(id),
            },
            Claim::Road(tile) => Self::Road {
                tile: (*tile).into(),
            },
        }
    }
}
impl ClaimDto {
    fn core(self) -> Result<Claim, BoundaryError> {
        Ok(match self {
            Self::Building { id } => Claim::Building(id.into_id("$.builders.claim")?),
            Self::Road { tile } => Claim::Road(tile.into()),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WalkDto {
    claim: ClaimDto,
    footprint: FootprintDto,
    role: RoleDto,
    path: Vec<PositionDto>,
    step: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuilderDto {
    id: EntityIdDto,
    role: RoleDto,
    #[serde(deserialize_with = "required_option")]
    claim: Option<ClaimDto>,
    #[serde(deserialize_with = "required_option")]
    walk: Option<WalkDto>,
}
fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
fn numbers(goods: &BTreeMap<String, Quantity>) -> UniqueMap<Number> {
    UniqueMap(
        goods
            .iter()
            .map(|(k, q)| (k.clone(), Number::core(q.get())))
            .collect(),
    )
}
fn quantities(goods: UniqueMap<Number>) -> Result<BTreeMap<String, Quantity>, BoundaryError> {
    goods
        .0
        .into_iter()
        .map(|(k, q)| {
            Ok((
                k,
                Quantity::new(q.get()).map_err(|_| invalid("invalid quantity"))?,
            ))
        })
        .collect()
}
fn invalid(reason: &'static str) -> BoundaryError {
    BoundaryError::invalid("$.building_model", reason)
}

fn capture(world: &BuildingWorld) -> Result<Save, BoundaryError> {
    for town in world.model.towns() {
        for actor in town.actors() {
            if matches!(
                actor.state().task,
                lantern_core::model::Task::Construction { .. }
                    | lantern_core::model::Task::RoadConstruction { .. }
            ) && !world.builders.contains_key(&actor.state().id)
            {
                return Err(invalid("unregistered construction task"));
            }
        }
    }
    let model = SaveWorldDto::from(&world.model);
    let towns = world
        .rules
        .iter()
        .map(|(id, r)| TownDto {
            town_id: id.as_str().into(),
            level: r.level,
            sandbox: r.sandbox,
            origin: r.origin.into(),
            player: r.player.into(),
            coins: Number::core(r.coins.get()),
            sites: r
                .sites
                .iter()
                .map(|(id, s)| SiteDto {
                    id: EntityIdDto::from_id(id),
                    kind: s.kind.clone(),
                    dimensions: [s.dimensions.0, s.dimensions.1],
                    orientation: s.orientation,
                    fence_orientation: s.fence_orientation,
                    stored: numbers(&s.stored),
                    pending: numbers(&s.pending),
                })
                .collect(),
            roads: r
                .roads
                .iter()
                .map(|r| RoadDto {
                    tile: r.tile.into(),
                    remaining: r.remaining.map(|w| Number::core(w.get())),
                })
                .collect(),
        })
        .collect();
    let land = &world.landscape;
    let landscape = LandscapeDto {
        size: land.size,
        unavailable: land.unavailable.iter().copied().map(Into::into).collect(),
        resources: land
            .resources
            .iter()
            .map(|r| ResourceDto {
                kind: r.kind.clone(),
                footprint: r.footprint.into(),
            })
            .collect(),
        locals: land.locals.iter().copied().map(Into::into).collect(),
        local_positions: land
            .local_positions
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
    };
    let builders = world
        .builders
        .iter()
        .map(|(id, b)| BuilderDto {
            id: EntityIdDto::from_id(id),
            role: b.role.into(),
            claim: b.claim.as_ref().map(Into::into),
            walk: b.walk.as_ref().map(|w| WalkDto {
                claim: (&w.claim).into(),
                footprint: w.footprint.into(),
                role: w.role.into(),
                path: w.path.iter().copied().map(Into::into).collect(),
                step: w.step,
            }),
        })
        .collect();
    Ok(Save {
        format: Format::Building,
        version: 2,
        model,
        shared: world.shared,
        towns,
        landscape,
        builders,
        extensions: Added(Some(Extensions::from_world(world))),
    })
}
/// Owned private construction/model facts. No serialization or I/O occurs during
/// capture; the owner can continue while a worker writes this immutable value.
/// This format covers BuildingWorld only, not the separate profession owners.
/// ```compile_fail
/// use lantern_host::building::BuildingSaveSnapshot;
/// fn leak(s: &BuildingSaveSnapshot) { serde_json::to_string(s).unwrap(); }
/// ```
/// ```compile_fail
/// use lantern_host::building::BuildingSaveSnapshot;
/// fn leak(s: &BuildingSaveSnapshot) { println!("{s:?}"); }
/// ```
pub struct BuildingSaveSnapshot {
    facts: Save,
}
impl BuildingSaveSnapshot {
    pub fn capture(world: &BuildingWorld) -> Result<Self, BoundaryError> {
        Ok(Self {
            facts: capture(world)?,
        })
    }

    /// Emits private JSON. Publication, flush and sync belong to the save worker.
    pub fn write_to(&self, writer: impl std::io::Write) -> Result<(), BoundaryError> {
        serde_json::to_writer(writer, &self.facts).map_err(write_error)
    }

    pub fn encode(&self) -> Result<Vec<u8>, BoundaryError> {
        serde_json::to_vec(&self.facts).map_err(|_| invalid("invalid construction encoding"))
    }

    pub fn decode(source: &[u8]) -> Result<Self, BoundaryError> {
        Self::capture(&decode_building_save(source)?)
    }

    pub fn restore(self) -> Result<BuildingWorld, BoundaryError> {
        restore(self.facts)
    }
}

pub fn encode_building_save(world: &BuildingWorld) -> Result<Vec<u8>, BoundaryError> {
    BuildingSaveSnapshot::capture(world)?.encode()
}

pub fn decode_building_save(source: &[u8]) -> Result<BuildingWorld, BoundaryError> {
    let save: Save =
        serde_json::from_slice(source).map_err(|_| invalid("invalid construction schema"))?;
    restore(save)
}

fn restore(save: Save) -> Result<BuildingWorld, BoundaryError> {
    if ![1, 2].contains(&save.version) {
        return Err(invalid("unsupported construction version"));
    }
    if (save.version == 1 && save.extensions.0.is_some())
        || (save.version == 2 && save.extensions.0.is_none())
    {
        return Err(invalid("version-gated construction extensions"));
    }
    let model = save.model.into_world()?;
    let mut rules = BTreeMap::new();
    let mut budget = 1_000_000_u64;
    for t in save.towns {
        let id = TownId::new(t.town_id).map_err(|_| invalid("invalid town identity"))?;
        let mut sites = BTreeMap::new();
        for s in t.sites {
            let id = s.id.into_id("$.towns.sites.id")?;
            let dimensions = (s.dimensions[0], s.dimensions[1]);
            budget = budget
                .checked_sub(u64::from(dimensions.0) * u64::from(dimensions.1))
                .ok_or_else(|| invalid("geometry expansion limit"))?;
            if sites
                .insert(
                    id,
                    Site {
                        kind: s.kind.clone(),
                        dimensions,
                        orientation: s.orientation,
                        fence_orientation: s.fence_orientation,
                        stored: quantities(s.stored)?,
                        pending: quantities(s.pending)?,
                        ..Site::new(s.kind, dimensions)
                    },
                )
                .is_some()
            {
                return Err(invalid("duplicate site identity"));
            }
        }
        let r = Rules {
            level: t.level,
            sandbox: t.sandbox,
            origin: t.origin.into(),
            player: t.player.into_core("$.towns.player")?,
            coins: Quantity::new(t.coins.get()).map_err(|_| invalid("invalid coins"))?,
            sites,
            progression: Progression::default(),
            roads: t
                .roads
                .into_iter()
                .map(|r| {
                    Ok(Road {
                        tile: r.tile.into(),
                        remaining: r
                            .remaining
                            .map(|n| Work::new(n.get()).map_err(|_| invalid("invalid work")))
                            .transpose()?,
                    })
                })
                .collect::<Result<_, BoundaryError>>()?,
        };
        if rules.insert(id, r).is_some() {
            return Err(invalid("duplicate town identity"));
        }
    }
    let l = save.landscape;
    let unavailable = l.unavailable.into_iter().map(Into::into).collect();
    let resources = l
        .resources
        .into_iter()
        .map(|r| {
            Ok(Resource {
                kind: r.kind,
                footprint: r.footprint.core(&mut budget)?,
            })
        })
        .collect::<Result<_, BoundaryError>>()?;
    let locals = l
        .locals
        .into_iter()
        .map(|f| f.core(&mut budget))
        .collect::<Result<_, _>>()?;
    let local_positions = l
        .local_positions
        .into_iter()
        .map(|p| p.into_core("$.landscape.local_positions"))
        .collect::<Result<_, _>>()?;
    let landscape = Landscape {
        size: l.size,
        unavailable,
        resources,
        locals,
        local_positions,
    };
    let mut builders = BTreeMap::new();
    for b in save.builders {
        let id = b.id.into_id("$.builders.id")?;
        let claim = b.claim.map(ClaimDto::core).transpose()?;
        let walk = b
            .walk
            .map(|w| {
                Ok(Walk {
                    claim: w.claim.core()?,
                    footprint: w.footprint.core(&mut budget)?,
                    role: w.role.into(),
                    path: w
                        .path
                        .into_iter()
                        .map(|p| p.into_core("$.builders.walk.path"))
                        .collect::<Result<_, _>>()?,
                    step: w.step,
                })
            })
            .transpose()?;
        if builders
            .insert(
                id,
                Builder {
                    role: b.role.into(),
                    claim,
                    walk,
                    helper: None,
                    suspended: None,
                },
            )
            .is_some()
        {
            return Err(invalid("duplicate builder identity"));
        }
    }
    if let Some(extensions) = save.extensions.0 {
        extensions.apply(&mut rules, &mut builders)?;
    }
    BuildingWorld::restore_with_builders(model, rules, landscape, save.shared, builders)
        .map_err(|_| invalid("inconsistent construction facts"))
}

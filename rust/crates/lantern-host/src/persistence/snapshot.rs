use super::{Result, invalid};
use crate::{
    building::{BuildingSaveSnapshot, BuildingWorld, decode_building_save},
    dto::{ModelSaveSnapshot, decode_save},
    farming::Fields,
    fishing::{Boats, ExplorationBoat, Fishers},
    gathering::Woodcutters,
    legacy::{ChunkKey, TerrainArchive, TerrainSaveSnapshot, VillageLife},
    livestock::Livestock,
    logistics::InputTrips,
    orchards::Orchards,
    population::Population,
    processing::Processors,
    progression::Worker,
    resources::Ledger,
    social::Social,
    state::ModelWorld,
};
use lantern_core::{
    ids::{ActorId, EntityIdentity, WorldId},
    model::Town,
    units::Practice,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, BTreeSet};

/// All admitted town owners are copied in one owner turn. This type is not a
/// public wire DTO. Worker practice/equipment is included even without Population.
#[derive(Clone)]
pub struct TownState {
    pub ledger: Ledger,
    pub workers: BTreeMap<ActorId, Worker>,
    pub trips: InputTrips,
    pub woodcutters: Woodcutters,
    pub processors: Processors,
    pub fields: Fields,
    pub orchards: Orchards,
    pub fishers: Fishers,
    pub boats: Boats,
    pub exploration: Option<ExplorationBoat>,
    pub livestock: Option<Livestock>,
    pub population: Option<Population>,
    pub social: Option<Social>,
}
impl TownState {
    pub fn empty(ledger: Ledger) -> Self {
        Self {
            ledger,
            workers: BTreeMap::new(),
            trips: InputTrips::default(),
            woodcutters: Woodcutters::default(),
            processors: Processors::default(),
            fields: Fields::default(),
            orchards: Orchards::default(),
            fishers: Fishers::default(),
            boats: Boats::default(),
            exploration: None,
            livestock: None,
            population: None,
            social: None,
        }
    }
}
enum ModelCopy {
    Basic(ModelSaveSnapshot),
    Building(Box<BuildingSaveSnapshot>),
}
enum TerrainCopy {
    Active(Box<TerrainSaveSnapshot>),
    Archive(TerrainArchive),
}
/// Immutable detached whole-world copy; all encoding/validation runs on the writer.
pub struct WorldSnapshot {
    id: WorldId,
    name: String,
    model: ModelCopy,
    towns: Vec<TownState>,
    terrain: Option<TerrainCopy>,
    village: Option<VillageLife>,
    resources: Resources,
    clocks: [f64; 2],
    random: BTreeMap<String, lantern_core::python_random::PythonRandom>,
}
impl WorldSnapshot {
    pub fn capture(model: &ModelWorld, towns: &[TownState]) -> Self {
        Self {
            id: model.id().clone(),
            name: model.id().as_str().into(),
            model: ModelCopy::Basic(ModelSaveSnapshot::capture(model)),
            towns: towns.to_vec(),
            terrain: None,
            village: None,
            resources: Resources::default(),
            clocks: [0.0; 2],
            random: BTreeMap::new(),
        }
    }
    pub fn capture_building(world: &BuildingWorld, towns: &[TownState]) -> Result<Self> {
        Ok(Self {
            id: world.model().id().clone(),
            name: world.model().id().as_str().into(),
            model: ModelCopy::Building(Box::new(
                BuildingSaveSnapshot::capture(world)
                    .map_err(|_| invalid("construction capture"))?,
            )),
            towns: towns.to_vec(),
            terrain: None,
            village: None,
            resources: Resources::default(),
            clocks: [0.0; 2],
            random: BTreeMap::new(),
        })
    }
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.into();
        self
    }
    pub fn with_resources(
        mut self,
        trees: &crate::gathering::Trees,
        water: &crate::fishing::Waters,
        iron: &crate::processing::IronDeposits,
    ) -> Self {
        self.resources = Resources {
            trees: trees.clone(),
            water: water.clone(),
            iron: iron.clone(),
        };
        self
    }
    pub fn with_clocks(
        mut self,
        simulation: lantern_core::units::Seconds,
        economic: lantern_core::units::Seconds,
    ) -> Self {
        self.clocks = [simulation.get(), economic.get()];
        self
    }
    pub fn with_random_streams(
        mut self,
        random: &BTreeMap<String, lantern_core::python_random::PythonRandom>,
    ) -> Self {
        self.random = random.clone();
        self
    }
    pub fn with_terrain(mut self, terrain: TerrainSaveSnapshot) -> Result<Self> {
        if terrain.world() != &self.id {
            return Err(invalid("terrain owner"));
        }
        self.terrain = Some(TerrainCopy::Active(Box::new(terrain)));
        Ok(self)
    }
    pub fn with_archive(mut self, terrain: &TerrainArchive) -> Self {
        self.terrain = Some(TerrainCopy::Archive(terrain.clone()));
        self
    }
    pub fn with_village(mut self, village: &VillageLife) -> Self {
        self.village = Some(village.clone());
        self
    }
}
/// No paths or live owner references, and no implicit Debug/Serialize.
/// ```compile_fail
/// use lantern_host::persistence::RegistrySnapshot;
/// fn leak(s: &RegistrySnapshot) { serde_json::to_string(s).unwrap(); }
/// ```
/// ```compile_fail
/// use lantern_host::persistence::RegistrySnapshot;
/// fn leak(s: &RegistrySnapshot) { println!("{s:?}"); }
/// ```
pub struct RegistrySnapshot {
    active: Option<WorldId>,
    worlds: Vec<WorldSnapshot>,
    provision: Option<Vec<u8>>,
}
impl RegistrySnapshot {
    pub fn capture(active: Option<&WorldId>, worlds: Vec<WorldSnapshot>) -> Self {
        Self {
            active: active.cloned(),
            worlds,
            provision: None,
        }
    }
    pub fn with_provision_archive(mut self, bytes: &[u8]) -> Self {
        self.provision = Some(bytes.to_vec());
        self
    }
    pub(super) fn encode(self) -> Result<Bundle> {
        let mut worlds = Vec::new();
        let mut chunks = Vec::new();
        for (index, world) in self.worlds.into_iter().enumerate() {
            let (model, building) = match world.model {
                ModelCopy::Basic(m) => (m.encode().map_err(|_| invalid("model encoding"))?, None),
                ModelCopy::Building(b) => {
                    let bytes = b.encode().map_err(|_| invalid("construction encoding"))?;
                    let loaded = decode_building_save(&bytes)
                        .map_err(|_| invalid("construction encoding"))?;
                    (
                        crate::dto::encode_save(loaded.model())
                            .map_err(|_| invalid("model encoding"))?,
                        Some(raw(std::str::from_utf8(&bytes)
                            .map_err(|_| invalid("construction encoding"))?)?),
                    )
                }
            };
            let loaded = decode_save(&model).map_err(|_| invalid("model encoding"))?;
            if loaded.id() != &world.id {
                return Err(invalid("world identity"));
            }
            let mut towns = Vec::new();
            for state in world.towns {
                let town = loaded
                    .towns()
                    .iter()
                    .find(|t| t.owner() == state.ledger.owner())
                    .ok_or_else(|| invalid("town owner"))?;
                towns.push(encode_town(&state, town)?);
            }
            let terrain = match world.terrain {
                Some(TerrainCopy::Active(t)) => {
                    Some(t.encode().map_err(|_| invalid("terrain encoding"))?)
                }
                Some(TerrainCopy::Archive(t)) => Some(t),
                None => None,
            };
            let terrain_payload = terrain.as_ref().map(|t| t.private_world_bytes().to_vec());
            let mut keys = Vec::new();
            if let Some(terrain) = terrain {
                for (key, bytes) in terrain.records() {
                    keys.push([key.x, key.y]);
                    chunks.push(ChunkBlob {
                        world: index,
                        key: *key,
                        bytes: bytes.clone(),
                    });
                }
            }
            worlds.push(SavedWorld {
                id: world.id.as_str().into(),
                name: world.name,
                model: raw(&model)?,
                building,
                towns,
                terrain: terrain_payload,
                keys,
                village: world.village.map(|v| v.encode()),
                trees: world
                    .resources
                    .trees
                    .trees()
                    .iter()
                    .map(|r| (r.tile.x, r.tile.y, r.supply.get()))
                    .collect(),
                water: world
                    .resources
                    .water
                    .water()
                    .iter()
                    .map(|r| (r.tile.x, r.tile.y, r.supply.get()))
                    .collect(),
                iron: world
                    .resources
                    .iron
                    .veins()
                    .iter()
                    .map(|r| (r.tile.x, r.tile.y, r.supply.get()))
                    .collect(),
                clocks: world.clocks,
                random: world
                    .random
                    .into_iter()
                    .map(|(name, r)| {
                        let (words, index) = r.state();
                        (name, "python-mt19937-v1".into(), words.to_vec(), index)
                    })
                    .collect(),
            });
        }
        let saved = SavedRegistry {
            format: "lantern-registry".into(),
            version: 1,
            active: self.active.map(|id| id.as_str().into()),
            provision: self.provision,
            worlds,
        };
        let registry = serde_json::to_vec(&saved).map_err(|_| invalid("registry encoding"))?;
        let bundle = Bundle { registry, chunks };
        bundle.restore()?; // Refuse inconsistent/unsupported ownership before any I/O.
        Ok(bundle)
    }
}
pub struct RestoredRegistry {
    provision: Option<Vec<u8>>,
    pub active: Option<WorldId>,
    pub worlds: Vec<RestoredWorld>,
}
impl RestoredRegistry {
    pub fn private_provision_bytes(&self) -> Option<&[u8]> {
        self.provision.as_deref()
    }
}
pub enum RestoredModel {
    Basic(ModelWorld),
    Building(BuildingWorld),
}
impl RestoredModel {
    pub fn model(&self) -> &ModelWorld {
        match self {
            Self::Basic(m) => m,
            Self::Building(b) => b.model(),
        }
    }
}
pub struct Resources {
    pub trees: crate::gathering::Trees,
    pub water: crate::fishing::Waters,
    pub iron: crate::processing::IronDeposits,
}
impl Default for Resources {
    fn default() -> Self {
        Self {
            trees: crate::gathering::Trees::default(),
            water: crate::fishing::Waters::new(vec![]).expect("empty waters"),
            iron: crate::processing::IronDeposits::default(),
        }
    }
}
pub struct RestoredWorld {
    pub name: String,
    pub model: RestoredModel,
    pub towns: Vec<TownState>,
    pub terrain: Option<TerrainArchive>,
    pub village: Option<VillageLife>,
    pub resources: Resources,
    pub clocks: [lantern_core::units::Seconds; 2],
    pub random: BTreeMap<String, lantern_core::python_random::PythonRandom>,
}
pub(super) struct ChunkBlob {
    pub world: usize,
    pub key: ChunkKey,
    pub bytes: Vec<u8>,
}
pub(super) struct Bundle {
    pub registry: Vec<u8>,
    pub chunks: Vec<ChunkBlob>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRegistry {
    #[serde(deserialize_with = "required")]
    provision: Option<Vec<u8>>,
    format: String,
    version: u32,
    #[serde(deserialize_with = "required")]
    active: Option<String>,
    worlds: Vec<SavedWorld>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedWorld {
    id: String,
    name: String,
    model: Box<RawValue>,
    #[serde(deserialize_with = "required")]
    building: Option<Box<RawValue>>,
    towns: Vec<SavedTown>,
    #[serde(deserialize_with = "required")]
    terrain: Option<Vec<u8>>,
    keys: Vec<[u32; 2]>,
    #[serde(deserialize_with = "required")]
    village: Option<Vec<u8>>,
    trees: Vec<(i32, i32, f64)>,
    water: Vec<(i32, i32, f64)>,
    iron: Vec<(i32, i32, f64)>,
    clocks: [f64; 2],
    random: Vec<(String, String, Vec<u32>, usize)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedTown {
    town: String,
    ledger: Box<RawValue>,
    workers: Vec<SavedWorker>,
    trips: Box<RawValue>,
    woodcutters: Box<RawValue>,
    processors: Box<RawValue>,
    fields: Box<RawValue>,
    orchards: Box<RawValue>,
    fishers: Box<RawValue>,
    boats: Box<RawValue>,
    #[serde(deserialize_with = "required")]
    exploration: Option<Box<RawValue>>,
    #[serde(deserialize_with = "required")]
    livestock: Option<Box<RawValue>>,
    #[serde(deserialize_with = "required")]
    population: Option<Box<RawValue>>,
    #[serde(deserialize_with = "required")]
    social: Option<Box<RawValue>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedWorker {
    actor: u64,
    job: String,
    skills: crate::dto::wire::UniqueMap<f64>,
    #[serde(deserialize_with = "required")]
    talents: Option<crate::dto::wire::UniqueMap<f64>>,
    sandbox: bool,
    #[serde(deserialize_with = "required")]
    tool: Option<String>,
}
fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
fn raw(json: &str) -> Result<Box<RawValue>> {
    RawValue::from_string(json.into()).map_err(|_| invalid("component JSON"))
}
fn component<T, E>(result: std::result::Result<T, E>) -> Result<T> {
    result.map_err(|_| invalid("component state"))
}
fn encode_town(s: &TownState, town: &Town) -> Result<SavedTown> {
    if s.population.as_ref().is_some_and(|p| {
        p.residents()
            .iter()
            .any(|r| s.workers.contains_key(&r.actor))
    }) {
        return Err(invalid("duplicate progression owner"));
    }
    for actor in s.workers.keys() {
        component(actor.identity().validate_owner(town.owner()))?;
        component(town.actor(actor))?;
    }

    Ok(SavedTown {
        town: town.owner().town.as_str().into(),
        ledger: raw(&component(s.ledger.encode(town))?)?,
        workers: s
            .workers
            .iter()
            .map(|(a, w)| SavedWorker {
                actor: a.identity().serial(),
                job: w.job().into(),
                skills: crate::dto::wire::UniqueMap(
                    w.skills()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.get()))
                        .collect(),
                ),
                talents: w.talents().map(|m| crate::dto::wire::UniqueMap(m.clone())),
                sandbox: w.sandbox,
                tool: w.tool.clone(),
            })
            .collect(),
        trips: raw(&component(s.trips.encode(town, &s.ledger))?)?,
        woodcutters: raw(&component(s.woodcutters.encode(town))?)?,
        processors: raw(&component(s.processors.encode(town))?)?,
        fields: raw(&component(s.fields.encode(town, &s.ledger))?)?,
        orchards: raw(&component(s.orchards.encode(town, &s.ledger))?)?,
        fishers: raw(&component(s.fishers.encode(town))?)?,
        boats: raw(&component(s.boats.encode(town))?)?,
        exploration: s
            .exploration
            .as_ref()
            .map(|v| raw(&component(v.encode())?))
            .transpose()?,
        livestock: s
            .livestock
            .as_ref()
            .map(|v| raw(&component(v.encode(town))?))
            .transpose()?,
        population: s
            .population
            .as_ref()
            .map(|v| raw(&component(v.encode(town))?))
            .transpose()?,
        social: s
            .social
            .as_ref()
            .map(|v| raw(&component(v.encode())?))
            .transpose()?,
    })
}
impl Bundle {
    pub fn restore(&self) -> Result<RestoredRegistry> {
        let saved: SavedRegistry =
            serde_json::from_slice(&self.registry).map_err(|_| invalid("registry schema"))?;
        if saved.format != "lantern-registry" || saved.version != 1 {
            return Err(invalid("registry version"));
        }
        if let Some(bytes) = &saved.provision {
            component(crate::legacy::validate_provision_archive(bytes))?;
        }
        let mut ids = BTreeSet::new();
        let mut worlds = Vec::new();
        let mut used = BTreeSet::new();
        for (index, w) in saved.worlds.into_iter().enumerate() {
            let model = component(decode_save(w.model.get()))?;
            if model.id().as_str() != w.id || !ids.insert(w.id) {
                return Err(invalid("world identity"));
            }
            let building = w
                .building
                .map(|b| component(decode_building_save(b.get().as_bytes())))
                .transpose()?;
            if let Some(b) = &building
                && component(crate::dto::encode_save(b.model()))?
                    != component(crate::dto::encode_save(&model))?
            {
                return Err(invalid("construction model mismatch"));
            }
            let mut towns = Vec::new();
            let mut town_ids = BTreeSet::new();
            for s in w.towns {
                let town = model
                    .towns()
                    .iter()
                    .find(|t| t.owner().town.as_str() == s.town)
                    .ok_or_else(|| invalid("town owner"))?;
                if !town_ids.insert(s.town) {
                    return Err(invalid("duplicate town owner"));
                }
                let ledger = component(Ledger::decode(s.ledger.get(), town))?;
                let fishers = component(Fishers::decode(s.fishers.get(), town, &ledger))?;
                let boats = component(Boats::decode(s.boats.get(), town, &ledger, &fishers))?;
                let mut workers = BTreeMap::new();
                for w in s.workers {
                    let actor = ActorId::from_saved(component(EntityIdentity::from_saved(
                        town.owner().clone(),
                        w.actor,
                    ))?);
                    component(town.actor(&actor))?;
                    let mut worker = component(Worker::new(&w.job))?;
                    for (k, v) in w.skills.0 {
                        component(worker.set_practice(&k, component(Practice::new(v))?))?;
                    }
                    if let Some(t) = w.talents {
                        component(worker.set_talents(t.0))?;
                    }
                    if w.tool.as_ref().is_some_and(|tool| {
                        !crate::resources::public_catalog()["crafted_worker_tools"]
                            .as_object()
                            .is_some_and(|tools| tools.contains_key(tool))
                    }) {
                        return Err(invalid("worker tool"));
                    }
                    worker.sandbox = w.sandbox;
                    worker.tool = w.tool;
                    if workers.insert(actor, worker).is_some() {
                        return Err(invalid("duplicate worker"));
                    }
                }
                towns.push(TownState {
                    trips: component(InputTrips::decode(s.trips.get(), town, &ledger))?,
                    woodcutters: component(Woodcutters::decode(
                        s.woodcutters.get(),
                        town,
                        &ledger,
                    ))?,
                    processors: component(Processors::decode(s.processors.get(), town, &ledger))?,
                    fields: component(Fields::decode(s.fields.get(), town, &ledger))?,
                    orchards: component(Orchards::decode(s.orchards.get(), town, &ledger))?,
                    fishers,
                    boats,
                    exploration: s
                        .exploration
                        .map(|v| component(ExplorationBoat::decode(v.get(), town)))
                        .transpose()?,
                    livestock: s
                        .livestock
                        .map(|v| component(Livestock::decode(v.get(), town, &ledger)))
                        .transpose()?,
                    population: s
                        .population
                        .map(|v| component(Population::decode(v.get(), town)))
                        .transpose()?,
                    social: s
                        .social
                        .map(|v| component(Social::decode(v.get(), town)))
                        .transpose()?,
                    ledger,
                    workers,
                });
            }
            for state in &towns {
                if state.population.as_ref().is_some_and(|p| {
                    p.residents()
                        .iter()
                        .any(|r| state.workers.contains_key(&r.actor))
                }) {
                    return Err(invalid("duplicate progression owner"));
                }
            }
            if towns.len() != model.towns().len() {
                return Err(invalid("missing town owners"));
            }
            let mut records = Vec::new();
            let mut keys = BTreeSet::new();
            for [x, y] in w.keys {
                let key = ChunkKey { x, y };
                if !keys.insert(key) {
                    return Err(invalid("duplicate chunk reference"));
                }
                let matches: Vec<_> = self
                    .chunks
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.world == index && c.key == key)
                    .collect();
                if matches.len() != 1 {
                    return Err(invalid("missing or duplicate chunk"));
                }
                used.insert(matches[0].0);
                records.push((key, matches[0].1.bytes.clone()));
            }
            let terrain = match w.terrain {
                Some(p) => Some(component(TerrainArchive::decode(&p, records))?),
                None if records.is_empty() => None,
                None => return Err(invalid("chunks without terrain")),
            };
            let village = w
                .village
                .map(|v| component(VillageLife::decode(model.id(), &v)))
                .transpose()?;
            let tile = |x, y| lantern_core::units::Tile { x, y };
            let resources = Resources {
                trees: component(crate::gathering::Trees::new(
                    w.trees
                        .into_iter()
                        .map(|(x, y, n)| {
                            Ok(crate::gathering::Tree {
                                tile: tile(x, y),
                                supply: component(lantern_core::units::Quantity::new(n))?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))?,
                water: component(crate::fishing::Waters::new(
                    w.water
                        .into_iter()
                        .map(|(x, y, n)| {
                            Ok(crate::fishing::Water {
                                tile: tile(x, y),
                                supply: component(lantern_core::units::Quantity::new(n))?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))?,
                iron: component(crate::processing::IronDeposits::new(
                    w.iron
                        .into_iter()
                        .map(|(x, y, n)| {
                            Ok(crate::processing::IronVein {
                                tile: tile(x, y),
                                supply: component(lantern_core::units::Quantity::new(n))?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))?,
            };
            let clocks = [
                component(lantern_core::units::Seconds::new(w.clocks[0]))?,
                component(lantern_core::units::Seconds::new(w.clocks[1]))?,
            ];
            let mut random = BTreeMap::new();
            for (name, algorithm, words, index) in w.random {
                if name.is_empty() || algorithm != "python-mt19937-v1" {
                    return Err(invalid("random stream version"));
                }
                let words: [u32; 624] = words
                    .try_into()
                    .map_err(|_| invalid("random stream words"))?;
                let stream = lantern_core::python_random::PythonRandom::from_state(words, index)
                    .ok_or_else(|| invalid("random stream state"))?;
                if random.insert(name, stream).is_some() {
                    return Err(invalid("duplicate random stream"));
                }
            }
            let model = match building {
                Some(b) => RestoredModel::Building(b),
                None => RestoredModel::Basic(model),
            };
            worlds.push(RestoredWorld {
                name: w.name,
                model,
                towns,
                terrain,
                village,
                resources,
                clocks,
                random,
            });
        }
        if used.len() != self.chunks.len() {
            return Err(invalid("unreferenced chunk"));
        }
        let active = saved
            .active
            .map(|id| component(WorldId::new(id)))
            .transpose()?;
        if active.as_ref().is_some_and(|id| !ids.contains(id.as_str())) {
            return Err(invalid("active world"));
        }
        Ok(RestoredRegistry {
            provision: saved.provision,
            active,
            worlds,
        })
    }
}

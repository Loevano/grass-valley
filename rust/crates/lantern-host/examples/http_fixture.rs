//! Disposable protocol fixture builder, never a playable-save conversion tool.
//! Reads only synthetic test input on stdin and creates a NEW explicit native root.
use lantern_core::{
    ids::{IdAllocator, Owner, TownId, WorldId},
    model::{ActorState, Building, Cargo, ConstructionState, Task, Town},
    units::{Position, Practice, Quantity, TerrainSeed, Tile, Work},
};
use lantern_host::{
    building::{BuildingWorld, Landscape, Progression, Road, Rules, Site, Upgrade},
    legacy::{ChunkKey, TerrainArchive, VillageLife},
    persistence::{RegistrySnapshot, Store, TownState, WorldSnapshot},
    population::{History, Population, Resident},
    progression::Worker,
    resources::{Ledger, ResourceKey, Stock, StoragePolicy, Store as ResourceStore},
    state::{ModelWorld, PrivateCredentials},
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    format: String,
    active: String,
    worlds: Vec<World>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct World {
    id: String,
    name: String,
    world: Box<RawValue>,
    records: BTreeMap<String, Box<RawValue>>,
    #[serde(default)]
    native_towns: bool,
    #[serde(default)]
    native_source: Option<Box<RawValue>>,
    #[serde(default)]
    native_work: bool,
    #[serde(default)]
    native_village: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct Ids {
    towns: BTreeMap<String, serde::de::IgnoredAny>,
}
fn main() {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("explicit new disposable root"),
    );
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let fixture: Fixture = serde_json::from_str(&input).unwrap();
    assert_eq!(fixture.format, "lantern-http-synthetic-v1");
    let active = WorldId::new(fixture.active).unwrap();
    let mut snapshots = Vec::new();
    for w in fixture.worlds {
        let id = WorldId::new(w.id).unwrap();
        let ids: Ids = serde_json::from_str(w.world.get()).unwrap();
        let mut towns = Vec::new();
        let mut states = Vec::new();
        for key in ids.towns.keys() {
            let owner = Owner {
                world: id.clone(),
                town: TownId::new(key).unwrap(),
            };
            towns.push(Town::restore(owner.clone(), vec![], vec![]).unwrap());
            states.push(TownState::empty(
                Ledger::new(owner, Quantity::new(0.).unwrap(), vec![], vec![]).unwrap(),
            ));
        }
        let model =
            ModelWorld::new(id, TerrainSeed(1), 0, towns, PrivateCredentials::default()).unwrap();
        let records = w
            .records
            .into_iter()
            .map(|(key, value)| {
                let (x, y) = key.split_once(',').unwrap();
                (
                    ChunkKey {
                        x: x.parse().unwrap(),
                        y: y.parse().unwrap(),
                    },
                    value.get().as_bytes().to_vec(),
                )
            })
            .collect();
        let terrain = TerrainArchive::decode(w.world.get().as_bytes(), records).unwrap();
        let snapshot = if w.native_towns {
            populated(
                model.id(),
                w.native_source.as_ref().unwrap_or(&w.world).get(),
                w.native_work,
            )
        } else {
            WorldSnapshot::capture(&model, &states)
        };
        let snapshot = if let Some(source) = w.native_village {
            snapshot.with_village(
                &VillageLife::restore(model.id().clone(), source.get().as_bytes()).unwrap(),
            )
        } else {
            snapshot
        };
        snapshots.push(snapshot.with_name(&w.name).with_archive(&terrain));
    }
    Store::create(&root)
        .unwrap()
        .commit(RegistrySnapshot::capture(Some(&active), snapshots))
        .unwrap();
}

fn q(value: f64) -> Quantity {
    Quantity::new(value).unwrap()
}
fn tile(value: &serde_json::Value) -> Tile {
    Tile {
        x: value["x"].as_i64().unwrap() as i32,
        y: value["y"].as_i64().unwrap() as i32,
    }
}
fn populated(id: &WorldId, text: &str, work: bool) -> WorldSnapshot {
    let world: serde_json::Value = serde_json::from_str(text).unwrap();
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../src/resources/catalog.json")).unwrap();
    let mut allocator = IdAllocator::new(id.clone());
    let mut towns = Vec::new();
    let mut states = Vec::new();
    let mut all_rules = BTreeMap::new();
    for (key, source) in world["towns"].as_object().unwrap() {
        let owner = Owner {
            world: id.clone(),
            town: TownId::new(key).unwrap(),
        };
        let mut buildings = Vec::new();
        let mut sites = BTreeMap::new();
        let mut stores = Vec::new();
        for source_building in source["buildings"].as_array().unwrap() {
            let building_id = allocator.allocate(&owner).unwrap();
            let kind = source_building["kind"].as_str().unwrap();
            let footprint = source_building["footprint"].as_array().unwrap();
            let mut site = Site::new(
                kind,
                (
                    footprint[0].as_u64().unwrap() as u32,
                    footprint[1].as_u64().unwrap() as u32,
                ),
            );
            site.level = source_building["level"].as_u64().unwrap_or(1) as u8;
            site.capacity_level = source_building["settler_level"].as_u64().unwrap_or(1) as u8;
            site.orientation = source_building["orientation"].as_u64().unwrap_or(0) as u8;
            site.fence_orientation =
                source_building["fence_orientation"].as_u64().unwrap_or(0) as u8;
            site.equipment = source_building["profession_upgrades"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let construction = if let Some(c) = source_building.get("construction") {
                if let Some(upgrade) = c.get("upgrade") {
                    site.upgrade = Some(if let Some(level) = upgrade["level"].as_u64() {
                        Upgrade::Structure(level as u8)
                    } else if let Some(level) = upgrade["settler_level"].as_u64() {
                        Upgrade::Capacity(level as u8)
                    } else {
                        Upgrade::Equipment(
                            upgrade["profession_upgrades"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().unwrap())
                                .find(|key| !site.equipment.contains(*key))
                                .unwrap()
                                .into(),
                        )
                    });
                } else if kind == "hall" {
                    let level = source["level"].as_u64().unwrap() + 1;
                    let hall_catalog: serde_json::Value =
                        serde_json::from_str(include_str!("../src/building/upgrade_catalog.json"))
                            .unwrap();
                    let reward = hall_catalog["hall"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| v["level"].as_u64() == Some(level))
                        .unwrap()["reward"]
                        .as_f64()
                        .unwrap();
                    site.upgrade = Some(Upgrade::Hall {
                        level: level as u8,
                        reward: q(reward),
                    });
                }
                ConstructionState::InProgress {
                    remaining: Work::new(
                        c["total"].as_f64().unwrap() - c["progress"].as_f64().unwrap(),
                    )
                    .unwrap(),
                }
            } else {
                ConstructionState::Complete
            };
            let mut policy = StoragePolicy::new(kind);
            policy.level = site.level;
            policy.equipment = site.equipment.clone();
            policy.construction = construction != ConstructionState::Complete;
            sites.insert(building_id.clone(), site);
            buildings.push(Building {
                id: building_id.clone(),
                location: tile(source_building),
                construction,
            });
            let mut store = ResourceStore::new(building_id, policy);
            let mut goods = BTreeMap::new();
            if let Some(input) = source_building["stored_goods"].as_object() {
                for (key, value) in input {
                    goods.insert(key.clone(), value.as_f64().unwrap());
                }
            } else if kind == "hall" {
                goods.insert("wood".into(), source["wood"].as_f64().unwrap_or(0.));
            }
            store.stored = Stock::from_entries(
                goods
                    .into_iter()
                    .map(|(k, v)| (ResourceKey::parse(&k).unwrap(), q(v))),
            )
            .unwrap();
            if kind == "hall" {
                store.provisions = q(source["food"].as_f64().unwrap_or(0.));
            }
            stores.push(store);
        }
        let mut actors = Vec::new();
        let mut residents = Vec::new();
        for v in source["villagers"].as_array().unwrap() {
            let actor = allocator.allocate(&owner).unwrap();
            actors.push(ActorState::new(
                actor.clone(),
                Position::new(v["x"].as_f64().unwrap_or(0.), v["y"].as_f64().unwrap_or(0.))
                    .unwrap(),
                Cargo::default(),
            ));
            let mut worker = Worker::new(v["job"].as_str().unwrap()).unwrap();
            let jobs = catalog["rules"]["jobs"].as_array().unwrap();
            for job in jobs {
                let job = job.as_str().unwrap();
                if let Some(n) = v.get(job) {
                    worker
                        .set_practice(job, Practice::new(n.as_f64().unwrap()).unwrap())
                        .unwrap();
                }
            }
            if let Some(talents) = v["talents"].as_object() {
                worker
                    .set_talents(
                        talents
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
                            .collect(),
                    )
                    .unwrap();
            }
            worker.sandbox = v["sandbox"].as_bool().unwrap_or(false);
            residents.push(Resident {
                actor,
                name: v["name"].as_str().unwrap().into(),
                worker,
                assignment: None,
            });
        }
        if work && let Some(actor) = actors.first_mut() {
            actor.task = Task::Move {
                destination: Position::new(20., 20.).unwrap(),
            };
        }
        let town = Town::restore(owner.clone(), buildings, actors).unwrap();
        let ledger = Ledger::new(
            owner.clone(),
            q(source["coins"].as_f64().unwrap_or(0.)),
            stores,
            vec![],
        )
        .unwrap();
        // Synthetic history is installed through the real validated native save
        // decoder, independently of the archive checked by HTTP admission.
        let mut saved: serde_json::Value =
            serde_json::from_str(&ledger.encode(&town).unwrap()).unwrap();
        for (wire, field) in [("produced", "produced"), ("production_caps", "caps")] {
            let value = if wire == "production_caps" {
                &source["rules"][wire]
            } else {
                &source[wire]
            };
            if let Some(values) = value.as_object() {
                saved[field] = serde_json::json!(
                    values
                        .iter()
                        .map(|(k, n)| (k, n.as_f64().unwrap()))
                        .collect::<Vec<_>>()
                );
            }
        }
        saved["market_sales"] = serde_json::json!(source["market_sales"].as_f64().unwrap_or(0.));
        if let Some(activity) = source.get("resource_activity") {
            saved["seconds"] = activity["seconds"].clone();
            saved["buckets"] = serde_json::json!(
                activity["buckets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| (&b["at"], &b["in"], &b["out"]))
                    .collect::<Vec<_>>()
            );
        }
        let ledger = Ledger::decode(&saved.to_string(), &town).unwrap();
        let mut state = TownState::empty(ledger);
        let sandbox = source["sandbox"].as_bool().unwrap_or(false);
        state.population =
            Some(Population::restore(&town, sandbox, residents, History::default()).unwrap());
        all_rules.insert(
            owner.town,
            Rules {
                level: source["level"].as_u64().unwrap() as u8,
                sandbox,
                origin: tile(&source["town_origin"]),
                player: Position::new(
                    source["player"]["x"].as_f64().unwrap(),
                    source["player"]["y"].as_f64().unwrap(),
                )
                .unwrap(),
                coins: q(source["coins"].as_f64().unwrap()),
                sites,
                roads: source["roads"]
                    .as_array()
                    .map(|records| {
                        records
                            .iter()
                            .map(|record| Road {
                                tile: tile(record),
                                remaining: record.get("construction").map(|c| {
                                    Work::new(
                                        c["total"].as_f64().unwrap()
                                            - c["progress"].as_f64().unwrap(),
                                    )
                                    .unwrap()
                                }),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                progression: Progression::default(),
            },
        );
        towns.push(town);
        states.push(state);
    }
    let credentials = PrivateCredentials {
        tokens: BTreeMap::from([(
            "SYNTHETIC_SECRET_TOKEN".into(),
            TownId::new(world["towns"].as_object().unwrap().keys().next().unwrap()).unwrap(),
        )]),
        ..PrivateCredentials::default()
    };
    let model = ModelWorld::new(
        id.clone(),
        TerrainSeed(1),
        allocator.high_water(),
        towns,
        credentials,
    )
    .unwrap();
    let size = world["shared_map"]["map_size"].as_u64().unwrap() as u32;
    let building = BuildingWorld::restore(
        model,
        all_rules,
        Landscape {
            size,
            unavailable: Default::default(),
            resources: vec![],
            locals: vec![],
            local_positions: vec![],
        },
        true,
    )
    .unwrap();
    WorldSnapshot::capture_building(&building, &states).unwrap()
}

//! Disposable protocol fixture builder, never a playable-save conversion tool.
//! Reads only synthetic test input on stdin and creates a NEW explicit native root.
use lantern_core::{
    ids::{IdAllocator, Owner, TownId, WorldId},
    model::{ActorState, Building, Cargo, ConstructionState, Task, Town},
    units::{Position, Practice, Quantity, TerrainSeed, Tile, Work},
};
use lantern_host::{
    building::{
        Builder, BuildingWorld, Claim, Landscape, Progression, Road, Role, Rules, Site, Upgrade,
        Walk,
    },
    fishing::ExplorationBoat,
    legacy::{ChunkKey, TerrainArchive, VillageLife},
    persistence::{RegistrySnapshot, Store, TownState, WorldSnapshot},
    population::{History, Population, Resident},
    progression::Worker,
    resources::{Ledger, PendingOwner, ResourceKey, Stock, StoragePolicy, Store as ResourceStore},
    social::{Player, Social},
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
    #[serde(default)]
    native_people: NativePeople,
    #[serde(default)]
    native_storage: NativeStorage,
    #[serde(default)]
    native_agriculture: NativeAgriculture,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativePeople {
    social: bool,
    boat: bool,
    progression: bool,
    stale_progression: bool,
    returned_tool: bool,
    last_batch: u64,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeStorage {
    mirrors: bool,
    stale_stored: bool,
    stale_pending: bool,
    missing_store: bool,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeAgriculture {
    missing_fields: bool,
    missing_trees: bool,
    stale_child: bool,
    assigned: bool,
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
                &w.native_people,
                &w.native_storage,
                &w.native_agriculture,
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
fn stock_value(value: &serde_json::Value) -> Stock {
    Stock::from_entries(
        value
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, n)| (ResourceKey::parse(k).unwrap(), q(n.as_f64().unwrap()))),
    )
    .unwrap()
}
fn populated(
    id: &WorldId,
    text: &str,
    work: bool,
    people: &NativePeople,
    storage: &NativeStorage,
    agriculture: &NativeAgriculture,
) -> WorldSnapshot {
    let world: serde_json::Value = serde_json::from_str(text).unwrap();
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../src/resources/catalog.json")).unwrap();
    let mut allocator = IdAllocator::new(id.clone());
    let mut towns = Vec::new();
    let mut states = Vec::new();
    let mut all_rules = BTreeMap::new();
    let mut builders = BTreeMap::new();
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
            policy.enabled = source_building
                .get("storage_enabled")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()).unwrap());
            if let Some(values) = source_building["storage_allocations"].as_object() {
                policy.legacy_allocations = values
                    .iter()
                    .map(|(k, n)| (k.clone(), q(n.as_f64().unwrap())))
                    .collect();
            }
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
                    if key == "food" {
                        store.provisions = q(value.as_f64().unwrap());
                    } else {
                        goods.insert(key.clone(), value.as_f64().unwrap());
                    }
                }
            } else if kind == "hall" {
                goods.insert("wood".into(), source["wood"].as_f64().unwrap_or(0.));
                if let Some(values) = source["goods"].as_object() {
                    for (k, n) in values {
                        goods.insert(k.clone(), n.as_f64().unwrap());
                    }
                }
                let edible: f64 = goods
                    .iter()
                    .map(|(k, n)| n * catalog["public"]["edible"][k].as_f64().unwrap_or(0.))
                    .sum();
                store.provisions = q((source["food"].as_f64().unwrap_or(0.) - edible).max(0.));
            }
            store.stored = Stock::from_entries(
                goods
                    .into_iter()
                    .map(|(k, v)| (ResourceKey::parse(&k).unwrap(), q(v))),
            )
            .unwrap();
            store.output = stock_value(&source_building["output_buffer"]);
            store.inputs = stock_value(&source_building["input_buffer"]);
            if source_building.get("orphaned_output").is_some() {
                store.pending.insert(
                    PendingOwner::Orphan,
                    stock_value(&source_building["orphaned_output"]),
                );
            }
            stores.push(store);
        }
        let ids: Vec<_> = buildings.iter().map(|b| b.id.clone()).collect();
        for (b, id) in source["buildings"].as_array().unwrap().iter().zip(&ids) {
            let site = sites.get_mut(id).unwrap();
            let field = site.kind == "acre";
            if field || site.kind == "orchard_tree" {
                let key = if field { "farm_index" } else { "orchard_index" };
                site.parent = Some(ids[b[key].as_u64().unwrap() as usize].clone());
                site.child = Some(if field {
                    lantern_host::building::ChildState::Field {
                        stage: b["field"]["stage"].as_str().unwrap_or("unplowed").into(),
                        elapsed: lantern_core::units::Seconds::new(
                            b["field"]["elapsed"].as_f64().unwrap_or(0.),
                        )
                        .unwrap(),
                        progress: Work::new(b["field"]["progress"].as_f64().unwrap_or(0.)).unwrap(),
                        cycle: b["field"]["cycle"].as_u64().unwrap_or(0),
                    }
                } else {
                    lantern_host::building::ChildState::Tree {
                        stage: b["tree"]["stage"].as_str().unwrap_or("sapling").into(),
                        elapsed: lantern_core::units::Seconds::new(
                            b["tree"]["elapsed"].as_f64().unwrap_or(0.),
                        )
                        .unwrap(),
                        fruit: q(b["tree"]["fruit"].as_f64().unwrap_or(8.)),
                    }
                });
            }
        }
        if agriculture.stale_child {
            for site in sites.values_mut() {
                match site.child.as_mut() {
                    Some(lantern_host::building::ChildState::Field { cycle, .. }) => *cycle += 1,
                    Some(lantern_host::building::ChildState::Tree { fruit, .. }) => *fruit = q(1.),
                    None => (),
                }
            }
        }
        let mut actors = Vec::new();
        let mut residents = Vec::new();
        for v in source["villagers"].as_array().unwrap() {
            let actor = allocator.allocate(&owner).unwrap();
            let position = v.get("construction_position").unwrap_or(v);
            let mut actor_state = ActorState::new(
                actor.clone(),
                Position::new(
                    position["x"].as_f64().unwrap_or(0.),
                    position["y"].as_f64().unwrap_or(0.),
                )
                .unwrap(),
                Cargo::default(),
            );
            actor_state.facing =
                lantern_core::travel::Facing::new(v["facing"].as_i64().unwrap_or(1) as i8).unwrap();
            if v["job"] == "building" || v.get("construction_target").is_some() {
                let role = if v["job"] == "building" {
                    Role::Dedicated
                } else {
                    Role::Idle
                };
                let claim = v.get("construction_target").map(|target| {
                    if target["kind"] == "road" {
                        Claim::Road(tile(target))
                    } else {
                        let index = source["buildings"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .position(|b| ["kind", "x", "y"].iter().all(|k| b[k] == target[k]))
                            .unwrap();
                        Claim::Building(ids[index].clone())
                    }
                });
                let phase = if v["construction_arrived"].as_bool().unwrap_or(false) {
                    lantern_core::model::ConstructionPhase::Building
                } else {
                    lantern_core::model::ConstructionPhase::Outbound
                };
                if let Some(c) = &claim {
                    actor_state.task = match c {
                        Claim::Building(id) => Task::Construction {
                            site: id.clone(),
                            phase,
                            elapsed: lantern_core::units::Seconds::new(0.).unwrap(),
                        },
                        Claim::Road(tile) => Task::RoadConstruction { tile: *tile, phase },
                    };
                }
                let walk = v.get("construction_walk").map(|w| {
                    let target = &w["target"];
                    let footprint = target
                        .get("footprint")
                        .map(|d| (d[0].as_u64().unwrap() as u32, d[1].as_u64().unwrap() as u32));
                    Walk {
                        claim: claim.clone().unwrap(),
                        footprint: lantern_core::spatial::Footprint::saved(tile(target), footprint)
                            .unwrap(),
                        role,
                        path: w["path"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|p| {
                                Position::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap())
                                    .unwrap()
                            })
                            .collect(),
                        step: w["step"].as_u64().unwrap() as usize,
                    }
                });
                builders.insert(
                    actor.clone(),
                    Builder {
                        role,
                        claim,
                        walk,
                        helper: None,
                        suspended: None,
                    },
                );
            }
            actors.push(actor_state);
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
            worker.tool = v["worker_tool"].as_str().map(str::to_owned);
            residents.push(Resident {
                actor,
                name: v["name"].as_str().unwrap().into(),
                worker,
                assignment: None,
            });
        }
        for (v, actor) in source["villagers"].as_array().unwrap().iter().zip(&actors) {
            if let Some(spec) = v.get("pending_workplace") {
                let index = source["buildings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|b| ["kind", "x", "y"].iter().all(|k| b[k] == spec[k]))
                    .unwrap();
                stores[index].pending.insert(
                    PendingOwner::Worker(actor.id.clone()),
                    stock_value(&v["pending_output"]),
                );
            }
        }
        if storage.mirrors {
            for store in &stores {
                let site = sites.get_mut(&store.id).unwrap();
                for stock in [&store.stored, &store.output, &store.inputs] {
                    for (k, n) in stock.entries() {
                        let total = site.stored.get(k.as_str()).map_or(0., |v| v.get()) + n.get();
                        site.stored.insert(k.as_str().into(), q(total));
                    }
                }
                if store.provisions.get() != 0. {
                    site.stored.insert("food".into(), store.provisions);
                }
                site.pending = store
                    .pending_stock()
                    .unwrap()
                    .entries()
                    .iter()
                    .map(|(k, n)| (k.as_str().into(), *n))
                    .collect();
            }
        }
        if storage.stale_stored {
            sites
                .get_mut(&stores[0].id)
                .unwrap()
                .stored
                .insert("wood".into(), q(999.));
        }
        if storage.stale_pending {
            sites
                .get_mut(&stores[0].id)
                .unwrap()
                .pending
                .insert("wood".into(), q(999.));
        }
        if storage.missing_store && stores.len() > 1 {
            stores.remove(1); // Empty home: balances alone cannot detect the missing owner.
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
        let history = History {
            food_shortage_seconds: source["food_shortage_seconds"].as_f64().unwrap_or(0.),
            housing_shortage_seconds: source["housing_shortage_seconds"].as_f64().unwrap_or(0.),
            arrival_delay_seconds: source["arrival_delay_seconds"].as_f64().unwrap_or(0.),
            departure_count: source["departure_count"].as_u64().unwrap_or(0),
            last_departure: source
                .get("last_departure")
                .map(|v| serde_json::from_value(v.clone()).unwrap()),
        };
        let mut population = Population::restore(&town, sandbox, residents, history).unwrap();
        if people.returned_tool {
            let mut saved: serde_json::Value =
                serde_json::from_str(&population.encode(&town).unwrap()).unwrap();
            saved["returned_tools"] = serde_json::json!({"fine_axe": 1});
            population = Population::decode(&saved.to_string(), &town).unwrap();
        }
        state.population = Some(population);
        if !storage.missing_store {
            state.fields = lantern_host::farming::Fields::admit_legacy(
                &town,
                &state.ledger,
                source["buildings"].as_array().unwrap(),
                &ids,
                BTreeMap::new(),
            )
            .unwrap();
            state.orchards = lantern_host::orchards::Orchards::admit_legacy(
                &town,
                &state.ledger,
                source["buildings"].as_array().unwrap(),
                &ids,
                BTreeMap::new(),
            )
            .unwrap();
        }
        if agriculture.missing_fields {
            state.fields = lantern_host::farming::Fields::default();
        }
        if agriculture.missing_trees {
            state.orchards = lantern_host::orchards::Orchards::default();
        }
        if agriculture.assigned
            && let Some(b) = source["buildings"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["kind"] == "farm")
        {
            state
                .fields
                .set_assignment(
                    &mut town.clone(),
                    &state.ledger,
                    &town.actors()[0].state().id,
                    Some(&ids[b]),
                )
                .unwrap();
        }
        let mut founder = lantern_host::progression::Worker::new("idle").unwrap();
        if people.social {
            let p = &source["player"];
            let value = |k: &str, default| p.get(k).cloned().unwrap_or(default);
            let jobs = catalog["rules"]["jobs"].as_array().unwrap();
            let practice = jobs
                .iter()
                .filter_map(|j| {
                    let j = j.as_str().unwrap();
                    p[j].as_f64().map(|n| (j.to_owned(), n))
                })
                .collect();
            let activity = p.get("activity").filter(|v| !v.is_null()).map(|v| {
                lantern_host::social::activity::Activity {
                    job: v["job"].as_str().unwrap().into(),
                    kind: v["kind"].as_str().unwrap().into(),
                    tile: [
                        v["x"].as_i64().unwrap() as i32,
                        v["y"].as_i64().unwrap() as i32,
                    ],
                }
            });
            let player = Player {
                items: serde_json::from_value(value("items", serde_json::json!({}))).unwrap(),
                worker_tools: serde_json::from_value(value("worker_tools", serde_json::json!({})))
                    .unwrap(),
                equipment: serde_json::from_value(value("equipment", serde_json::json!({})))
                    .unwrap(),
                keepsakes: serde_json::from_value(value("keepsakes", serde_json::json!([])))
                    .unwrap(),
                practice,
                activity,
                activity_status: p["activity_status"].as_str().map(str::to_owned),
                facing: p["facing"].as_i64().unwrap_or(1) as i8,
                last_batch: people.last_batch,
            };
            for (job, n) in &player.practice {
                founder
                    .set_practice(job, Practice::new(*n).unwrap())
                    .unwrap();
            }
            let visits = serde_json::from_value(
                source
                    .get("local_visits")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            )
            .unwrap();
            state.social = Some(Social::restore(owner.clone(), player, visits).unwrap());
        }
        if people.boat {
            let p = &source["player"];
            let tier = if p["boat"].as_bool().unwrap_or(false) {
                p["boat_level"].as_u64().unwrap_or(1)
            } else {
                0
            };
            state.exploration = Some(
                ExplorationBoat::decode(
                    &serde_json::json!({"version":1, "world":id.as_str(), "town":key, "tier":tier})
                        .to_string(),
                    &town,
                )
                .unwrap(),
            );
        }
        let mut progression = if people.progression {
            let workers: Vec<_> = state
                .population
                .as_ref()
                .unwrap()
                .residents()
                .iter()
                .map(|r| r.worker.clone())
                .collect();
            lantern_host::progression::construction_evidence(
                &state.ledger,
                &workers,
                people.social.then_some(&founder),
                0,
                state.ledger.market_sales(),
            )
            .unwrap()
        } else {
            Progression::default()
        };
        if people.stale_progression {
            progression.food = Some(q(progression.food.unwrap().get() + 1.));
        }
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
                progression,
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
    let building = BuildingWorld::restore_with_builders(
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
        builders,
    )
    .unwrap();
    WorldSnapshot::capture_building(&building, &states).unwrap()
}

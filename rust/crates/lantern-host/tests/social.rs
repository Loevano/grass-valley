use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::{ActorState, Building, Cargo, ConstructionState, Town},
    units::{Position, Quantity, Seconds, Tile},
};
use lantern_host::{
    population::{History, Population, Resident},
    progression::Worker,
    resources::*,
    social::*,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn owner(t: &str) -> Owner {
    Owner {
        world: WorldId::new("social").unwrap(),
        town: TownId::new(t).unwrap(),
    }
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn key(k: &str) -> ResourceKey {
    ResourceKey::parse(k).unwrap()
}
fn a(o: &Owner, n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(o.clone(), n).unwrap())
}
fn b(o: &Owner, n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(o.clone(), n).unwrap())
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
struct Fixture {
    owner: Owner,
    town: Town,
    pop: Population,
    ledger: Ledger,
    social: Social,
    position: Position,
    locals: Vec<Local>,
    rocks: Vec<[i32; 2]>,
    roads: u64,
    ground: Vec<Ground>,
}
impl Fixture {
    fn new(v: &Value, name: &str) -> Self {
        let o = owner(name);
        let buildings = v["buildings"].as_array().unwrap();
        let villagers = v["villagers"].as_array().unwrap();
        let town = Town::restore(
            o.clone(),
            buildings
                .iter()
                .enumerate()
                .map(|(i, v)| Building {
                    id: b(&o, i as u64 + 1),
                    location: Tile {
                        x: v["x"].as_i64().unwrap_or(1) as i32,
                        y: v["y"].as_i64().unwrap_or(1) as i32,
                    },
                    construction: ConstructionState::Complete,
                })
                .collect(),
            villagers
                .iter()
                .enumerate()
                .map(|(i, _)| ActorState::new(a(&o, 100 + i as u64), p(1., 1.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let pop = Population::restore(
            &town,
            false,
            villagers
                .iter()
                .enumerate()
                .map(|(i, v)| Resident {
                    actor: a(&o, 100 + i as u64),
                    name: v["name"].as_str().unwrap_or("Resident").into(),
                    worker: Worker::new("idle").unwrap(),
                    assignment: None,
                })
                .collect(),
            History::default(),
        )
        .unwrap();
        let stores = buildings
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let mut policy = StoragePolicy::new(v["kind"].as_str().unwrap());
                policy.construction = v.get("construction").is_some();
                policy.enabled = v
                    .get("storage_enabled")
                    .map(|v| serde_json::from_value(v.clone()).unwrap());
                if let Some(allocations) = v["storage_allocations"].as_object() {
                    policy.legacy_allocations = allocations
                        .iter()
                        .map(|(k, v)| (k.clone(), q(v.as_f64().unwrap())))
                        .collect();
                }
                let mut s = Store::new(b(&o, i as u64 + 1), policy);
                if let Some(goods) = v["stored_goods"].as_object() {
                    s.provisions = q(goods.get("food").and_then(Value::as_f64).unwrap_or(0.));
                    s.stored = Stock::from_entries(
                        goods
                            .iter()
                            .filter(|(k, _)| k.as_str() != "food")
                            .map(|(k, v)| (key(k), q(v.as_f64().unwrap()))),
                    )
                    .unwrap();
                }
                s
            })
            .collect();
        let mut ledger = Ledger::new(
            o.clone(),
            q(v["coins"].as_f64().unwrap_or(0.)),
            stores,
            vec![],
        )
        .unwrap();
        if let Some(caps) = v["rules"]["production_caps"].as_object() {
            ledger.restore_caps(
                caps.iter()
                    .map(|(k, n)| (key(k), q(n.as_f64().unwrap())))
                    .collect(),
            );
        }
        let vp = &v["player"];
        let player = Player {
            items: serde_json::from_value(vp.get("items").cloned().unwrap_or(json!({}))).unwrap(),
            worker_tools: serde_json::from_value(
                vp.get("worker_tools").cloned().unwrap_or(json!({})),
            )
            .unwrap(),
            equipment: serde_json::from_value(vp.get("equipment").cloned().unwrap_or(json!({})))
                .unwrap(),
            keepsakes: vec![],
            practice: ["woodcutting", "fishing", "gathering"]
                .into_iter()
                .filter_map(|j| vp[j].as_f64().map(|n| (j.into(), n)))
                .collect(),
            ..Player::default()
        };
        let visits =
            serde_json::from_value(v.get("local_visits").cloned().unwrap_or(json!({}))).unwrap();
        let social = Social::restore(o.clone(), player, visits).unwrap();
        let locals = v["npcs"]
            .as_array()
            .map(|ns| {
                ns.iter()
                    .map(|n| {
                        let curr = n.get("routine").unwrap_or(n);
                        Local {
                            name: n["name"].as_str().unwrap().into(),
                            village: n["village"].as_str().map(str::to_string),
                            key: format!("{}:{}:{}", n["name"].as_str().unwrap(), n["x"], n["y"]),
                            position: p(curr["x"].as_f64().unwrap(), curr["y"].as_f64().unwrap()),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rocks = v["map_resources"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|r| r["kind"] == "rock")
                    .map(|r| {
                        [
                            r["x"].as_i64().unwrap() as i32,
                            r["y"].as_i64().unwrap() as i32,
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ground = v["terrain"]
            .as_array()
            .map(|a| {
                a.iter()
                    .enumerate()
                    .flat_map(|(y, r)| {
                        r.as_str()
                            .unwrap()
                            .chars()
                            .enumerate()
                            .map(move |(x, c)| Ground {
                                tile: [x as i32, y as i32],
                                biome: c.to_string(),
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            owner: o,
            town,
            pop,
            ledger,
            social,
            position: p(vp["x"].as_f64().unwrap(), vp["y"].as_f64().unwrap()),
            locals,
            rocks,
            roads: 0,
            ground,
        }
    }
    fn basic(name: &str) -> Self {
        Self::new(
            &json!({"buildings":[{"kind":"hall","stored_goods":{"wood":100,"herbs":5,"food":50,"crafted_axe":2}},{"kind":"market"}],"villagers":[{"name":"Ada"},{"name":"Rowan"}],"player":{"x":5.,"y":5.},"coins":100,"npcs":[]}),
            name,
        )
    }
    fn change(&mut self, v: &Value) -> Result<(), Error> {
        self.social.change(
            &self.owner,
            &Context {
                world: &self.owner.world,
                town: &self.town,
                position: self.position,
                locals: &self.locals,
                rocks: &self.rocks,
                completed_roads: self.roads,
                ground: &self.ground,
            },
            &mut self.ledger,
            &mut self.pop,
            v,
        )
    }
}
fn check(f: &Fixture, expected: &Value) {
    close(
        f.ledger.free(&key("coins")).unwrap().get(),
        expected["coins"].as_f64().unwrap(),
    );
    close(
        f.ledger.food().unwrap().get(),
        expected["food"].as_f64().unwrap(),
    );
    for (k, v) in expected["goods"].as_object().unwrap() {
        close(f.ledger.free(&key(k)).unwrap().get(), v.as_f64().unwrap());
    }
    let p = f.social.player();
    let e = &expected["player"];
    for (actual, name) in [
        (serde_json::to_value(&p.items).unwrap(), "items"),
        (
            serde_json::to_value(&p.worker_tools).unwrap(),
            "worker_tools",
        ),
        (serde_json::to_value(&p.equipment).unwrap(), "equipment"),
    ] {
        assert_eq!(
            actual,
            e.get(name).cloned().unwrap_or(json!({})),
            "player {name}"
        );
    }
    assert_eq!(
        serde_json::to_value(&p.keepsakes).unwrap(),
        e.get("keepsakes").cloned().unwrap_or(json!([]))
    );
    let visits: BTreeMap<String, Visit> =
        serde_json::from_value(expected["visits"].clone()).unwrap();
    assert_eq!(f.social.visits(), &visits);
    assert_eq!(
        serde_json::to_value(
            f.pop
                .residents()
                .iter()
                .map(|r| r.worker.tool.clone())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        expected["tools"]
    );
}
#[test]
fn python_gifts_all_shops_barters_worker_tools_stories_and_equipment() {
    let cases: Value =
        serde_json::from_slice(include_bytes!("fixtures/social/python.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 26);
    for case in cases.as_array().unwrap() {
        let mut f = Fixture::new(&case["before"], "one");
        for step in case["steps"].as_array().unwrap() {
            let d = &step["command"];
            let action = d["action"].as_str().unwrap();
            let before = f.social.clone();
            let goods = f.ledger.clone();
            let residents = f.pop.clone();
            let result = match action {
                "move" => {
                    f.position = p(d["x"].as_f64().unwrap(), d["y"].as_f64().unwrap());
                    Ok(())
                }
                "roads" => {
                    f.roads = d["count"].as_u64().unwrap();
                    Ok(())
                }
                "restock" => f.social.advance_restocks(
                    &f.owner,
                    Seconds::new(d["seconds"].as_f64().unwrap()).unwrap(),
                ),
                _ => f.change(d),
            };
            assert_eq!(
                result.is_ok(),
                step["ok"].as_bool().unwrap(),
                "{} {d}: {result:?}",
                case["label"]
            );
            if result.is_err() {
                assert_eq!(before, f.social);
                assert_eq!(goods, f.ledger);
                assert_eq!(residents, f.pop);
            }
            check(&f, &step["after"]);
            let saved = f.social.encode().unwrap();
            f.social = Social::decode(&saved, &f.town).unwrap();
            assert_eq!(saved, f.social.encode().unwrap());
        }
    }
}
#[test]
fn ownership_personal_gifts_current_positions_and_unsafe_unequip() {
    let mut f = Fixture::basic("one");
    let mut other = Fixture::basic("two");
    f.locals.push(Local {
        name: "Petra".into(),
        village: None,
        key: "Petra:20:20".into(),
        position: p(5., 6.),
    });
    other.locals = f.locals.clone();
    let gift = json!({"action":"claim_local_gift","npc_index":0});
    f.change(&gift).unwrap();
    other.change(&gift).unwrap();
    assert!(f.change(&gift).is_err());
    f.change(&json!({"action":"equip_item","item":"swamp_boots"}))
        .unwrap();
    // (5,6) is the dark swamp shade and touches this fractional founder position.
    let tile = (0..12)
        .flat_map(|x| (0..12).map(move |y| [x, y]))
        .find(|[x, y]| ((x * 17 + y * 31 + x * y * 7) % 7) % 3 == 2)
        .unwrap();
    f.position = p(tile[0] as f64, tile[1] as f64);
    f.ground = vec![Ground {
        tile,
        biome: "b".into(),
    }];
    assert!(
        f.change(&json!({"action":"unequip_item","item":"swamp_boots"}))
            .is_err()
    );
    f.ground.clear();
    f.change(&json!({"action":"unequip_item","item":"swamp_boots"}))
        .unwrap();
    let before = f.social.clone();
    assert!(
        f.social
            .change(
                &other.owner,
                &Context {
                    world: &f.owner.world,
                    town: &f.town,
                    position: f.position,
                    locals: &f.locals,
                    rocks: &[],
                    completed_roads: 0,
                    ground: &[]
                },
                &mut f.ledger,
                &mut f.pop,
                &gift
            )
            .is_err()
    );
    assert_eq!(before, f.social);
    assert!(Social::decode(&f.social.encode().unwrap(), &other.town).is_err());
    for bad in [json!(true), json!(-1), json!(99)] {
        assert!(
            f.change(&json!({"action":"interact_local","npc_index":bad}))
                .is_err()
        );
    }
}
#[test]
fn departure_receipt_collection_is_atomic_and_once_after_reload() {
    let mut f = Fixture::basic("one");
    f.pop.worker_mut(&a(&f.owner, 100)).unwrap().tool = Some("fine_axe".into());
    // The population owner owns the departure event and its persisted receipt.
    let h = History {
        food_shortage_seconds: 57.,
        ..History::default()
    };
    f.pop = Population::restore(&f.town, false, f.pop.residents().to_vec(), h).unwrap();
    let mut empty = f.ledger.stores().to_vec();
    for s in &mut empty {
        s.stored = Stock::default();
        s.provisions = q(0.);
    }
    f.ledger = Ledger::new(f.owner.clone(), q(0.), empty, vec![]).unwrap();
    let places = f
        .town
        .buildings()
        .iter()
        .zip(f.ledger.stores())
        .map(|(b, s)| lantern_host::population::Place {
            id: b.id.clone(),
            site: lantern_host::building::Site::new(s.policy.kind.clone(), (1, 1)),
        })
        .collect::<Vec<_>>();
    let mut town = f.town.clone();
    f.pop
        .economic_tick(
            &mut town,
            &mut f.ledger,
            &mut Default::default(),
            &places,
            &mut |_| 0,
            &mut || panic!("no arrival"),
        )
        .unwrap();
    f.town = town;
    f.pop = Population::decode(&f.pop.encode(&f.town).unwrap(), &f.town).unwrap();
    f.social = Social::decode(&f.social.encode().unwrap(), &f.town).unwrap();
    f.social
        .collect_departed_tools(&f.owner, &f.town, &mut f.pop)
        .unwrap();
    f.pop = Population::decode(&f.pop.encode(&f.town).unwrap(), &f.town).unwrap();
    f.social = Social::decode(&f.social.encode().unwrap(), &f.town).unwrap();
    f.social
        .collect_departed_tools(&f.owner, &f.town, &mut f.pop)
        .unwrap();
    assert_eq!(f.social.player().worker_tools["fine_axe"], 1);
}

#[test]
fn python_shared_trade_offers_limits_capacity_atomicity_and_replay() {
    use lantern_host::social::trading::*;
    let cases: Value =
        serde_json::from_slice(include_bytes!("fixtures/social/trades.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mut towns: BTreeMap<_, _> = case["before"]["towns"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, v)| {
                let f = Fixture::new(v, name);
                (
                    f.owner.town.clone(),
                    TradeTown {
                        level: v["level"].as_u64().unwrap() as u8,
                        ledger: f.ledger,
                    },
                )
            })
            .collect();
        let world = owner("one").world;
        let mut board =
            Trading::restore(world.clone(), BTreeMap::new(), BTreeMap::new(), &towns).unwrap();
        let mut sequence = 0;
        for step in case["steps"].as_array().unwrap() {
            let d = &step["command"];
            let o = owner(d["actor"].as_str().unwrap());
            let action = d["action"].as_str().unwrap();
            let before = board.encode(&towns).unwrap();
            let goods: Vec<_> = towns.values().map(|t| t.ledger.clone()).collect();
            let result = match action {
                "spend" => towns
                    .get_mut(&o.town)
                    .unwrap()
                    .ledger
                    .spend(
                        &Stock::from_entries([(
                            key(d["resource"].as_str().unwrap()),
                            q(d["amount"].as_f64().unwrap()),
                        )])
                        .unwrap(),
                    )
                    .map_err(Error::Resource),
                "level" => {
                    towns.get_mut(&o.town).unwrap().level = d["level"].as_u64().unwrap() as u8;
                    Ok(())
                }
                _ => board.change(&o, &mut towns, d, &mut || {
                    sequence += 1;
                    format!("offer-{sequence}")
                }),
            };
            assert_eq!(
                result.is_ok(),
                step["ok"].as_bool().unwrap(),
                "{} {d} {result:?}",
                case["label"]
            );
            if result.is_err() {
                assert_eq!(before, board.encode(&towns).unwrap());
                assert_eq!(
                    goods,
                    towns.values().map(|t| t.ledger.clone()).collect::<Vec<_>>()
                );
            }
            assert_eq!(sequence, step["sequence"].as_u64().unwrap());
            for (name, e) in step["after"].as_object().unwrap() {
                let id = TownId::new(name).unwrap();
                let t = &towns[&id];
                for (k, n) in e["goods"].as_object().unwrap() {
                    assert!(
                        (t.ledger.free(&key(k)).unwrap().get() - n.as_f64().unwrap()).abs() < 1e-9,
                        "{} {d} resource {k}: {} != {n}",
                        case["label"],
                        t.ledger.free(&key(k)).unwrap().get()
                    );
                }
                assert_eq!(
                    serde_json::to_value(board.offers().get(name).cloned().unwrap_or_default())
                        .unwrap(),
                    e["offers"]
                );
                assert_eq!(board.completed(&id), e["completed"].as_u64().unwrap());
                assert!(t.ledger.produced().entries().is_empty());
            }
            let saved = board.encode(&towns).unwrap();
            board = Trading::decode(&world, &saved, &towns).unwrap();
            assert_eq!(saved, board.encode(&towns).unwrap());
        }
        let saved = board.encode(&towns).unwrap();
        assert!(Trading::decode(&WorldId::new("other-world").unwrap(), &saved, &towns).is_err());
    }
}
fn resource(v: &Value) -> Resource {
    Resource {
        kind: v["kind"].as_str().unwrap().into(),
        tile: [
            v["x"].as_i64().unwrap() as i32,
            v["y"].as_i64().unwrap() as i32,
        ],
        supply: v["supply"].as_f64(),
        species: v["species"].as_str().map(str::to_string),
    }
}
#[test]
fn python_founder_gathering_mining_chests_fishing_supplies_and_practice() {
    let cases: Value =
        serde_json::from_slice(include_bytes!("fixtures/social/activities.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let before = &case["before"];
        let mut f = Fixture::new(before, "one");
        let world = f.owner.world.clone();
        let mut land = FounderLand::restore(
            world.clone(),
            before["terrain_version"].as_u64().unwrap() as u8,
            f.ground.clone(),
            before["map_resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(resource)
                .collect(),
            vec![],
        )
        .unwrap();
        let mut batch = 0;
        for step in case["steps"].as_array().unwrap() {
            let d = &step["command"];
            let action = d["action"].as_str().unwrap();
            let old = f.social.clone();
            let goods = f.ledger.clone();
            let terrain = land.clone();
            let result = if action == "move" {
                f.position = p(d["x"].as_f64().unwrap(), d["y"].as_f64().unwrap());
                Ok(())
            } else if action == "tick" {
                batch += 1;
                f.social.activity_tick(
                    &f.owner,
                    &f.town,
                    &mut f.ledger,
                    &mut land,
                    f.position,
                    batch,
                    d["roll"].as_u64().unwrap_or(1) as u8,
                    before["player"]["boat_level"].as_u64().unwrap_or(0) as u8,
                )
            } else {
                f.social.start_activity(
                    &f.owner,
                    &f.town,
                    &mut f.ledger,
                    &mut land,
                    f.position,
                    action,
                    false,
                )
            };
            assert_eq!(
                result.is_ok(),
                step["ok"].as_bool().unwrap(),
                "{} {d} {result:?}",
                case["label"]
            );
            if result.is_err() {
                assert_eq!(old, f.social);
                assert_eq!(goods, f.ledger);
                assert_eq!(terrain, land);
            }
            let e = &step["after"];
            check(&f, &e["state"]);
            let ep = &e["state"]["player"];
            assert_eq!(
                f.social.player().facing,
                i8::try_from(ep["facing"].as_i64().unwrap_or(1)).unwrap()
            );
            for job in ["woodcutting", "fishing", "gathering"] {
                close(
                    f.social.player().practice.get(job).copied().unwrap_or(0.),
                    ep[job].as_f64().unwrap_or(0.),
                );
            }
            let ea = ep.get("activity").map(|v| activity::Activity {
                job: v["job"].as_str().unwrap().into(),
                kind: v["kind"].as_str().unwrap().into(),
                tile: [
                    v["x"].as_i64().unwrap() as i32,
                    v["y"].as_i64().unwrap() as i32,
                ],
            });
            assert_eq!(f.social.player().activity, ea);
            assert_eq!(
                f.social.player().activity_status.as_deref(),
                ep["activity_status"].as_str()
            );
            let expected: Vec<_> = e["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(resource)
                .collect();
            assert_eq!(land.resources(), expected, "{} source {d}", case["label"]);
            let stumps: Vec<_> = e["stumps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Stump {
                    tile: [
                        v["x"].as_i64().unwrap() as i32,
                        v["y"].as_i64().unwrap() as i32,
                    ],
                    species: v["species"].as_str().map(str::to_string),
                    remaining: v["remaining"].as_f64().unwrap(),
                })
                .collect();
            assert_eq!(land.stumps(), stumps);
            let saved = f.social.encode().unwrap();
            f.social = Social::decode(&saved, &f.town).unwrap();
            let map = land.encode().unwrap();
            land = FounderLand::decode(&world, &map).unwrap();
            assert_eq!(land.encode().unwrap(), map);
            if action == "tick" {
                let old = f.social.clone();
                let goods = f.ledger.clone();
                let map = land.clone();
                f.social
                    .activity_tick(
                        &f.owner,
                        &f.town,
                        &mut f.ledger,
                        &mut land,
                        f.position,
                        batch,
                        1,
                        0,
                    )
                    .unwrap();
                assert_eq!(old, f.social);
                assert_eq!(goods, f.ledger);
                assert_eq!(map, land);
            }
        }
    }
}
#[test]
fn python_non_village_roster_rng_canopy_relocation_and_visit_keys() {
    use lantern_host::legacy::NpcRoster;
    let cases: Value =
        serde_json::from_slice(include_bytes!("fixtures/social/roster.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 12);
    let huge = lantern_core::spatial::Footprint::saved(
        Tile {
            x: -2_000_000_000,
            y: -2_000_000_000,
        },
        Some((4_000_000_000, 4_000_000_000)),
    )
    .unwrap();
    assert!(
        NpcRoster::materialize(br#"{"map_resources":[],"buildings":[]}"#, &[], &[huge]).is_err()
    );
    for case in cases.as_array().unwrap() {
        let before = &case["before"];
        let f = Fixture::new(
            &{
                let mut v = before.clone();
                if v.get("npcs").is_none() {
                    v["npcs"] = json!([]);
                }
                v
            },
            "one",
        );
        let source = serde_json::to_vec(before).unwrap();
        let result = NpcRoster::materialize(&source, &f.ground, &[]).unwrap();
        assert_eq!(result.private_source_bytes(), source);
        assert_eq!(
            serde_json::from_slice::<Value>(result.private_town_bytes()).unwrap(),
            case["after"]
        );
        let moved: BTreeMap<_, _> = result.moved_visit_keys().iter().cloned().collect();
        assert_eq!(serde_json::to_value(moved).unwrap(), case["moved"]);
        let repeat = NpcRoster::materialize(result.private_town_bytes(), &f.ground, &[]).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(repeat.private_town_bytes()).unwrap(),
            case["repeated"]
        );
        assert!(repeat.moved_visit_keys().is_empty());
    }
}

#[test]
fn shared_chest_owner_adventure_arbitration_and_finite_inventory_rollback() {
    let mut first = Fixture::basic("one");
    let mut second = Fixture::basic("two");
    let mut land = FounderLand::restore(
        first.owner.world.clone(),
        7,
        vec![],
        vec![Resource {
            kind: "chest".into(),
            tile: [5, 6],
            supply: None,
            species: None,
        }],
        vec![],
    )
    .unwrap();
    first
        .social
        .start_activity(
            &first.owner,
            &first.town,
            &mut first.ledger,
            &mut land,
            first.position,
            "gather",
            false,
        )
        .unwrap();
    let coins = first.ledger.free(&key("coins")).unwrap().get();
    let other_coins = second.ledger.free(&key("coins")).unwrap().get();
    assert!(
        second
            .social
            .start_activity(
                &second.owner,
                &second.town,
                &mut second.ledger,
                &mut land,
                second.position,
                "gather",
                false
            )
            .is_err()
    );
    close(first.ledger.free(&key("coins")).unwrap().get(), coins);
    close(
        second.ledger.free(&key("coins")).unwrap().get(),
        other_coins,
    );
    assert!(
        first
            .social
            .start_activity(
                &first.owner,
                &first.town,
                &mut first.ledger,
                &mut land,
                first.position,
                "gather",
                true
            )
            .is_err()
    );
    first
        .social
        .start_activity(
            &first.owner,
            &first.town,
            &mut first.ledger,
            &mut land,
            first.position,
            "stop_activity",
            true,
        )
        .unwrap();
    let saved = land.encode().unwrap();
    assert!(FounderLand::decode(&WorldId::new("other-world").unwrap(), &saved).is_err());
    let mut f = Fixture::basic("overflow");
    let mut player = Player::default();
    player.worker_tools.insert("fine_axe".into(), u64::MAX);
    f.social = Social::restore(f.owner.clone(), player, BTreeMap::new()).unwrap();
    f.locals.push(Local {
        name: "Sylva".into(),
        village: None,
        key: "Sylva:5:6".into(),
        position: p(5., 6.),
    });
    let before = f.social.clone();
    let goods = f.ledger.clone();
    assert!(
        f.change(&json!({"action":"claim_local_gift","npc_index":0}))
            .is_err()
    );
    assert_eq!(before, f.social);
    assert_eq!(goods, f.ledger);
}
#[test]
fn strict_shop_price_and_shared_visit_move_preserve_imported_progress() {
    use lantern_host::legacy::NpcRoster;
    let mut f = Fixture::basic("one");
    f.locals.push(Local {
        name: "Bram".into(),
        village: None,
        key: "Bram:5:6".into(),
        position: p(5., 6.),
    });
    f.ledger = Ledger::new(
        f.owner.clone(),
        q(30. - 5e-10),
        f.ledger.stores().to_vec(),
        vec![],
    )
    .unwrap();
    assert!(
        f.change(&json!({"action":"buy_worker_tool","npc_index":0,"item":"steel_axe"}))
            .is_err()
    );
    let source=br#"{"local_visits":{"old":{"gift_claimed":true,"story_stage":2,"trade_remaining":7.5,"opaque":1.2500e-1},"middle":{"keep":1},"new":{"story_stage":0}},"private_count":123456789012345678901234567890}"#;
    let moved = vec![("old".into(), "new".into())];
    let output = NpcRoster::move_visit_keys(source, &moved).unwrap();
    let v: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(v["local_visits"]["new"]["gift_claimed"], true);
    assert_eq!(v["local_visits"]["new"]["story_stage"], 2);
    assert!(v["local_visits"].get("old").is_none());
    let text = String::from_utf8(output.clone()).unwrap();
    assert!(text.contains("1.2500e-1"));
    assert!(text.contains("123456789012345678901234567890"));
    assert_eq!(NpcRoster::move_visit_keys(&output, &moved).unwrap(), output);
}

#[test]
fn saved_activity_fields_duplicates_precision_and_foreign_scene_are_rejected() {
    let mut f = Fixture::basic("one");
    let saved = f.social.encode().unwrap();
    let mut v: Value = serde_json::from_str(&saved).unwrap();
    v["player"].as_object_mut().unwrap().remove("activity");
    assert!(Social::decode(&v.to_string(), &f.town).is_err());
    let duplicate = saved.replacen("\"visits\":{}", "\"visits\":{},\"visits\":{}", 1);
    assert_ne!(saved, duplicate);
    assert!(Social::decode(&duplicate, &f.town).is_err());
    let wide = saved.replace(
        "\"visits\":{}",
        "\"visits\":{\"Local:1:1\":{\"opaque\":123456789012345678901234567890}}",
    );
    assert!(Social::decode(&wide, &f.town).is_err());
    let other = WorldId::new("other-world").unwrap();
    let before = f.social.clone();
    assert!(
        f.social
            .change(
                &f.owner,
                &Context {
                    world: &other,
                    town: &f.town,
                    position: f.position,
                    locals: &[],
                    rocks: &[],
                    completed_roads: 0,
                    ground: &[]
                },
                &mut f.ledger,
                &mut f.pop,
                &json!({"action":"give_worker_tool","index":0,"item":"crafted_axe"})
            )
            .is_err()
    );
    assert_eq!(before, f.social);
    let mut player = Player::default();
    player.items.insert("future_item".into(), 1);
    assert!(Social::restore(f.owner.clone(), player, BTreeMap::new()).is_err());
}

use lantern_core::{ids::WorldId, units::Seconds};
use lantern_host::legacy::{
    ActiveVillageChunk, ChunkKey, OrdinaryVillageMetadata, SwampVillageMetadata, VillageLife,
};
use serde_json::{Value, json};
fn owner() -> WorldId {
    WorldId::new("villages").unwrap()
}
fn secs(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn value(v: &VillageLife) -> Value {
    serde_json::from_slice(&v.private_world_bytes()).unwrap()
}
fn restore(v: &Value) -> VillageLife {
    VillageLife::restore(owner(), &serde_json::to_vec(v).unwrap()).unwrap()
}
fn base() -> Value {
    json!({"towns":{"one":{"buildings":[],"player":{"x":1.,"y":1.},"local_visits":{"Ada:79:81":{"met":true,"gift_claimed":true,"story":3,"trade_remaining":7.5}}}},"shared_map":{"map_size":480,"villages":[{"name":"Reedhaven","biome":"b","x":80,"y":80,"layout_version":2,"life_version":1}],"npcs":[{"name":"Ada","role":"winter woodworker","x":79,"y":81,"village":"Reedhaven"}]}})
}
fn records(v: &Value) -> Vec<(ChunkKey, Vec<u8>)> {
    v.as_object()
        .map(|m| {
            m.iter()
                .map(|(key, v)| {
                    let mut pair = key.split(',');
                    (
                        ChunkKey {
                            x: pair.next().unwrap().parse().unwrap(),
                            y: pair.next().unwrap().parse().unwrap(),
                        },
                        serde_json::to_vec(v).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
fn active<'a>(
    owner: &'a WorldId,
    records: &'a [(ChunkKey, Vec<u8>)],
) -> Vec<ActiveVillageChunk<'a>> {
    records
        .iter()
        .map(|(key, bytes)| ActiveVillageChunk {
            world: owner,
            key: *key,
            private_record: bytes,
        })
        .collect()
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(
                a.len(),
                e.len(),
                "field count {path}: {actual} != {expected}"
            );
            for (k, v) in e {
                compare(
                    a.get(k).unwrap_or_else(|| panic!("missing {path}.{k}")),
                    v,
                    &format!("{path}.{k}"),
                );
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "length {path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            let tolerance =
                if path.contains(".routine.") && (path.ends_with(".x") || path.ends_with(".y")) {
                    1e-6
                } else if path.contains(".routine.")
                    && (path.ends_with(".wait") || path.ends_with(".elapsed"))
                {
                    1e-9
                } else {
                    0.
                };
            assert!((a - e).abs() <= tolerance, "number {path}: {a} != {e}");
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn actual_python_routes_work_wait_pause_collision_and_reconnect_traces() {
    let cases: Value =
        serde_json::from_slice(include_bytes!("fixtures/village_life/python.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 20);
    for case in cases.as_array().unwrap() {
        let world = owner();
        let mut life = restore(&case["before"]);
        let mut recs = records(&case["active"]);
        for (i, event) in case["events"].as_array().unwrap().iter().enumerate() {
            let op = event["op"].as_str().unwrap();
            match op {
                "advance" => life
                    .advance(
                        &world,
                        secs(event["seconds"].as_f64().unwrap()),
                        &active(&world, &recs),
                    )
                    .unwrap_or_else(|e| panic!("{} event {i}: {e:?}", case["label"])),
                "restock" => life
                    .advance_restocks(&world, secs(event["seconds"].as_f64().unwrap()))
                    .unwrap(),
                "active" => recs = records(&event["records"]),
                "visitor" | "building" | "block_next" => {
                    let mut v = value(&life);
                    let town = event["town"].as_str().unwrap();
                    if op == "visitor" {
                        v["towns"][town]["player"] = event["position"].clone();
                    } else {
                        let building = if op == "building" {
                            event["building"].clone()
                        } else {
                            json!({"kind":"home","x":event["target"][0],"y":event["target"][1],"footprint":[1,1]})
                        };
                        v["towns"][town]["buildings"]
                            .as_array_mut()
                            .unwrap()
                            .push(building);
                    }
                    life = restore(&v);
                }
                _ => panic!("unknown event"),
            }
            if !event["after"].is_null() {
                compare(
                    &value(&life),
                    &event["after"],
                    &format!("{} event {i}", case["label"]),
                );
            }
            if i % 17 == 0 {
                let saved = life.encode();
                life = VillageLife::decode(&world, &saved).unwrap();
                assert_eq!(
                    life.encode(),
                    saved,
                    "reconnect has no work or clock effects"
                );
            }
        }
        for r in case["routes"].as_array().unwrap() {
            let p = |v: &Value| [v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32];
            let result = life
                .route(
                    &world,
                    r["village"].as_u64().unwrap() as usize,
                    p(&r["start"]),
                    p(&r["goal"]),
                    &active(&world, &recs),
                )
                .unwrap();
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                r["path"],
                "{} route {r}",
                case["label"]
            );
        }
    }
}
#[test]
fn stable_anchor_current_position_pause_and_separate_restock_clock() {
    let mut life = restore(&base());
    let key = life.local_key(&owner(), 0).unwrap();
    life.advance(&owner(), secs(6.), &[]).unwrap();
    assert_ne!(life.position(&owner(), 0).unwrap().x.get(), 79.);
    assert_eq!(life.local_key(&owner(), 0).unwrap(), key);
    assert_eq!(
        value(&life)["towns"]["one"]["local_visits"][&key]["trade_remaining"],
        7.5
    );
    let mut v = value(&life);
    let pos = life.position(&owner(), 0).unwrap();
    v["towns"]["second"] = json!({"buildings":[],"player":{"x":pos.x.get(),"y":pos.y.get()}});
    life = restore(&v);
    life.advance(&owner(), secs(60.), &[]).unwrap();
    assert_eq!(life.position(&owner(), 0).unwrap(), pos);
    let saved = life.encode();
    let mut life = VillageLife::decode(&owner(), &saved).unwrap();
    assert_eq!(life.encode(), saved);
    life.advance_restocks(&owner(), secs(3.)).unwrap();
    let progress = &value(&life)["towns"]["one"]["local_visits"][&key];
    assert_eq!(progress["trade_remaining"], 4.5);
    assert_eq!(progress["story"], 3);
    assert_eq!(progress["gift_claimed"], true);
    life.advance_restocks(&owner(), secs(999.)).unwrap();
    assert_eq!(
        value(&life)["towns"]["one"]["local_visits"][&key]["trade_remaining"],
        0.
    );
}
#[test]
fn owner_world_switch_absent_local_and_active_geometry_are_guarded() {
    let mut life = restore(&base());
    let before = life.encode();
    let other = WorldId::new("other").unwrap();
    assert!(life.advance(&other, secs(1.), &[]).is_err());
    assert!(life.advance_restocks(&other, secs(3.)).is_err());
    assert!(life.position(&other, 0).is_err());
    assert!(life.position(&owner(), 999).is_err());
    assert!(VillageLife::decode(&other, &before).is_err());
    let world = owner();
    let foreign = ActiveVillageChunk {
        world: &other,
        key: ChunkKey { x: 1, y: 1 },
        private_record: br#"{"map_resources":[]}"#,
    };
    assert!(life.advance(&world, secs(1.), &[foreign]).is_err());
    let duplicate = || ActiveVillageChunk {
        world: &world,
        key: ChunkKey { x: 1, y: 1 },
        private_record: br#"{"map_resources":[]}"#,
    };
    assert!(
        life.advance(&world, secs(1.), &[duplicate(), duplicate()])
            .is_err()
    );
    assert_eq!(before, life.encode());
    let foreign = ActiveVillageChunk {
        world: &world,
        key: ChunkKey { x: 1, y: 1 },
        private_record: br#"{"map_resources":[{"x":90,"y":90}]}"#,
    };
    assert!(life.advance(&world, secs(1.), &[foreign]).is_err());
    assert_eq!(before, life.encode());
}
#[test]
fn raw_private_extensions_and_arbitrary_quantities_survive_ambient_save_reload() {
    let source=br#"{"towns":{"one":{"buildings":[],"food":123456789012345678901234567890,"local_visits":{"saved":{"trade_remaining":4.5,"gift":true,"story":2,"progress":1.2500e-1}}}},"shared_map":{"map_size":480,"villages":[{"name":"Reedhaven","x":80,"y":80,"layout_version":2,"life_version":1}],"npcs":[{"name":"Local","x":79,"y":81,"role":"fisher","village":"Reedhaven","extension":{"private":1.2300e-4}}]},"lanterns":{"elapsed":3.1415000},"private_token":"synthetic-only"}"#;
    let mut life = VillageLife::restore(owner(), source).unwrap();
    life.advance(&owner(), secs(0.5), &[]).unwrap();
    life.advance_restocks(&owner(), secs(3.)).unwrap();
    let saved = life.encode();
    let loaded = VillageLife::decode(&owner(), &saved).unwrap();
    assert_eq!(saved, loaded.encode());
    let result = String::from_utf8(loaded.private_world_bytes()).unwrap();
    for literal in [
        "123456789012345678901234567890",
        "1.2500e-1",
        "1.2300e-4",
        "3.1415000",
        "synthetic-only",
    ] {
        assert!(result.contains(literal));
    }
}
#[test]
fn malformed_future_state_rejects_without_exposing_private_values() {
    let mut life = restore(&base());
    life.advance(&owner(), secs(0.5), &[]).unwrap();
    let saved = life.encode();
    let mut v: Value = serde_json::from_slice(&saved).unwrap();
    v["version"] = json!(2);
    assert!(VillageLife::decode(&owner(), &serde_json::to_vec(&v).unwrap()).is_err());
    for bad in [json!(-1), json!("secret-invalid-clock")] {
        let mut v = base();
        v["towns"]["one"]["local_visits"]["Ada:79:81"]["trade_remaining"] = bad;
        let err = VillageLife::restore(owner(), &serde_json::to_vec(&v).unwrap())
            .err()
            .unwrap();
        assert!(!format!("{err:?}").contains("secret-invalid-clock"));
    }
    let mut v = value(&life);
    v["shared_map"]["npcs"][0]["routine"]["phase"] = json!("future");
    assert!(VillageLife::restore(owner(), &serde_json::to_vec(&v).unwrap()).is_err());
    assert!(VillageLife::restore(owner(), br#"{"towns":{},"towns":{}}"#).is_err());
    let mut v = base();
    v["shared_map"]["npcs"][0]["ambient_stop"] = json!([i32::MAX, 1]);
    assert!(VillageLife::restore(owner(), &serde_json::to_vec(&v).unwrap()).is_err());
    assert_eq!(life.encode(), saved);
}
#[test]
fn existing_migrations_transfer_both_towns_visits_before_routines_resume() {
    let mut v = base();
    v["shared_map"]["villages"][0]["swamp_version"] = json!(1);
    v["shared_map"]["npcs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"Aster","role":"basket weaver","x":70,"y":70,"village":"Reedhaven"}));
    v["towns"]["one"]["local_visits"]["Aster:70:70"] =
        json!({"met":true,"story":7,"gift_claimed":true,"trade_remaining":4.5});
    v["towns"]["two"] = json!({"buildings":[],"roads":[],"local_visits":{"Aster:70:70":{"story":2,"trade_remaining":3.}}});
    let ordinary = OrdinaryVillageMetadata::materialize(&serde_json::to_vec(&v).unwrap()).unwrap();
    let swamp = SwampVillageMetadata::materialize(ordinary.private_world_bytes()).unwrap();
    assert_eq!(swamp.cache_plan().village_ordinals, vec![0]);
    // This fixture has no runtime manager. ChunkLifecycle owns active/camera
    // cache effects; the existing R07 lifecycle tests verify those effects.
    let mut life = VillageLife::restore(owner(), swamp.private_world_bytes()).unwrap();
    let w = value(&life);
    let index = w["shared_map"]["npcs"]
        .as_array()
        .unwrap()
        .iter()
        .position(|n| n["name"] == "Aster")
        .unwrap();
    let key = life.local_key(&owner(), index).unwrap();
    assert_ne!(key, "Aster:70:70");
    assert_eq!(w["towns"]["one"]["local_visits"][&key]["story"], 7);
    assert_eq!(w["towns"]["two"]["local_visits"][&key]["story"], 2);
    life.advance(&owner(), secs(0.5), &[]).unwrap();
    assert_eq!(life.local_key(&owner(), index).unwrap(), key);
    let save = life.encode();
    assert_eq!(VillageLife::decode(&owner(), &save).unwrap().encode(), save);
}

#[test]
fn integral_float_anchors_keep_the_original_visit_key_spelling() {
    let mut v = base();
    v["shared_map"]["npcs"][0]["x"] = json!(79.0);
    v["shared_map"]["npcs"][0]["y"] = json!(81.0);
    let mut life = restore(&v);
    assert_eq!(life.local_key(&owner(), 0).unwrap(), "Ada:79.0:81.0");
    life.advance(&owner(), secs(0.5), &[]).unwrap();
    assert_eq!(life.local_key(&owner(), 0).unwrap(), "Ada:79.0:81.0");
}

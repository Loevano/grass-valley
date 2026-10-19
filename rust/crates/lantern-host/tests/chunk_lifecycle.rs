use lantern_core::ids::WorldId;
use lantern_host::{BoundaryError, legacy::*};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Deserialize)]
struct Oracle {
    payloads: Vec<Value>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    label: String,
    world: Box<RawValue>,
    saved: BTreeMap<String, Box<RawValue>>,
    active: BTreeMap<String, Box<RawValue>>,
    camera: BTreeMap<String, Box<RawValue>>,
    active_order: Vec<String>,
    initial: Value,
    actions: Vec<Value>,
    outputs: Vec<Value>,
}
fn oracle() -> Oracle {
    serde_json::from_slice(include_bytes!("fixtures/terrain/lifecycle.json")).unwrap()
}
fn key(text: &str) -> ChunkKey {
    let (x, y) = text.split_once(',').unwrap();
    ChunkKey {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
    }
}
fn text(key: ChunkKey) -> String {
    key.file_name().trim_end_matches(".json").to_string()
}
fn resolve(value: &Value, payloads: &[Value]) -> Value {
    match value {
        Value::Object(map) if map.len() == 1 && map.contains_key("fixture_ref") => resolve(
            &payloads[map["fixture_ref"].as_u64().unwrap() as usize],
            payloads,
        ),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), resolve(v, payloads)))
            .collect(),
        Value::Array(items) => items.iter().map(|v| resolve(v, payloads)).collect(),
        _ => value.clone(),
    }
}
fn equal(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            assert_eq!(a.as_f64(), b.as_f64(), "{path}")
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.len(),
                b.len(),
                "{path}: object keys actual {:?}, expected {:?}",
                a.keys(),
                b.keys()
            );
            for (k, v) in b {
                equal(
                    a.get(k).unwrap_or_else(|| panic!("{path}: missing {k}")),
                    v,
                    &format!("{path}.{k}"),
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: array lengths");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equal(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}
struct Copy {
    root: PathBuf,
    source: Vec<u8>,
    directory: PathBuf,
    owner: WorldId,
}
impl Copy {
    fn new(case: &Case) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-lifecycle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[derive(Deserialize)]
        struct World {
            shared_map: Seed,
        }
        #[derive(Deserialize)]
        struct Seed {
            terrain_seed: Box<RawValue>,
        }
        let world: World = serde_json::from_str(case.world.get()).unwrap();
        let seed = serde_json::from_str::<String>(world.shared_map.terrain_seed.get())
            .unwrap_or_else(|_| world.shared_map.terrain_seed.get().into());
        let directory = root.join(seed);
        fs::create_dir_all(&directory).unwrap();
        let source = format!(r#"{{"world":{},"tokens":{{}}}}"#, case.world.get()).into_bytes();
        for (k, v) in &case.saved {
            fs::write(directory.join(format!("{k}.json")), v.get()).unwrap();
        }
        Self {
            root,
            source,
            directory,
            owner: WorldId::new("legacy").unwrap(),
        }
    }
    fn import(&self) -> ImportedRegistry {
        import_with_chunk_copies(
            Some(&self.source),
            None,
            &[ChunkDirectory::new(&self.owner, &self.directory)],
        )
        .unwrap()
    }
    fn disk(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(&self.directory)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                let b = fs::read(&p).unwrap();
                (p, b)
            })
            .collect()
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn residents<'a>(
    records: &'a BTreeMap<String, Box<RawValue>>,
    owner: &'a WorldId,
) -> Vec<ResidentChunkCopy<'a>> {
    records
        .iter()
        .map(|(k, v)| ResidentChunkCopy::new(owner, key(k), v.get().as_bytes()))
        .collect()
}
fn state(owner: &ChunkLifecycle<'_>, records: bool) -> Value {
    let mut result = json!({"world":value(&owner.private_world_bytes()),"occupied":owner.occupied_tiles().unwrap(),"active":owner.active_keys().into_iter().map(text).collect::<Vec<_>>(),
        "camera":owner.camera_keys_lru().into_iter().map(text).collect::<Vec<_>>(),
        "desired":owner.desired().unwrap().into_iter().map(text).collect::<Vec<_>>()});
    result["desired"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|v| v.as_str().unwrap().to_string());
    if records {
        result["records"] = owner
            .active_keys()
            .into_iter()
            .map(|k| (text(k), value(owner.private_active_bytes(k).unwrap())))
            .collect();
    }
    result
}
#[test]
fn actual_python_lifecycle_traces_preserve_owned_edits_promotion_clocks_and_ordered_swamp_plans() {
    let oracle = oracle();
    assert_eq!(oracle.cases.len(), 33);
    for case in &oracle.cases {
        let copy = Copy::new(case);
        let before = copy.disk();
        let registry = copy.import();
        let report = registry.report();
        let active: Vec<_> = case
            .active_order
            .iter()
            .map(|k| ResidentChunkCopy::new(&copy.owner, key(k), case.active[k].get().as_bytes()))
            .collect();
        let camera = residents(&case.camera, &copy.owner);
        let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &active, &camera).unwrap();
        equal(
            &state(&owner, true),
            &resolve(&case.initial, &oracle.payloads),
            &format!("{} initial", case.label),
        );
        let mut memory = serde_json::Map::new();
        for (i, (action, expected)) in case.actions.iter().zip(&case.outputs).enumerate() {
            let op = action["op"].as_str().unwrap();
            let mut read = None;
            match op {
                "read" => {
                    read = Some(value(
                        &owner.read(key(action["key"].as_str().unwrap())).unwrap(),
                    ))
                }
                "activate" => owner.activate().unwrap(),
                "renew" => owner.renew_active().unwrap(),
                "clock" => owner
                    .set_clock(action["clock"].to_string().as_bytes())
                    .unwrap(),
                "move" => owner
                    .move_founder(
                        "synthetic-town",
                        action["x"].as_f64().unwrap(),
                        action["y"].as_f64().unwrap(),
                    )
                    .unwrap(),
                "edit" => {
                    let k = key(action["key"].as_str().unwrap());
                    let mut r = value(owner.private_active_bytes(k).unwrap());
                    r["map_resources"] = json!([action["resource"]]);
                    r["tree_stumps"] = json!([action["stump"]]);
                    let y = action["resource"]["y"].as_u64().unwrap() as usize % 40;
                    let x = action["resource"]["x"].as_u64().unwrap() as usize % 40;
                    let mut row = r["terrain"][y].as_str().unwrap().to_string();
                    row.replace_range(x..x + 1, "f");
                    r["terrain"][y] = json!(row);
                    r["private_edit"] = serde_json::from_str(
                        r#"{"quantity":123456789012345678901234567890,"opaque":".0625"}"#,
                    )
                    .unwrap();
                    owner.replace_active(k, r.to_string().as_bytes()).unwrap();
                }
                "flush" => owner.flush().unwrap(),
                "snapshot" => {
                    let keys: Vec<_> = action["keys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| key(v.as_str().unwrap()))
                        .collect();
                    let snapshot = owner.snapshot(&keys).unwrap();
                    read = Some(
                        snapshot
                            .keys()
                            .map(|k| (text(k), value(snapshot.private_record_bytes(k).unwrap())))
                            .collect(),
                    );
                    assert!(owner.snapshot_cache_len() <= owner.active_keys().len() + 49);
                }
                _ => panic!("unknown action"),
            }
            for intent in owner.take_write_intents() {
                assert_eq!(intent.world(), &copy.owner);
                memory.insert(text(intent.key()), value(intent.private_record_bytes()));
            }
            let mut actual = state(&owner, matches!(op, "activate" | "edit" | "renew"));
            actual["memory"] = Value::Object(memory.clone());
            if let Some(read) = read {
                actual["read"] = read;
            }
            equal(
                &actual,
                &resolve(expected, &oracle.payloads),
                &format!("{} step {i} {op}", case.label),
            );
        }
        assert_eq!(copy.disk(), before, "{} wrote source files", case.label);
        assert_eq!(registry.private_source_bytes().unwrap(), copy.source);
        assert_eq!(registry.report(), report);
        assert!(
            String::from_utf8(owner.private_world_bytes())
                .unwrap()
                .contains("123456789012345678901234567890")
        );
    }
}
fn assert_error<T>(result: Result<T, BoundaryError>) {
    match result {
        Ok(_) => panic!("expected rejection"),
        Err(error) => {
            let text = format!("{error:?}");
            assert!(!text.contains("SYNTHETIC_SECRET"));
            assert!(text.contains("$."));
        }
    }
}
#[test]
fn derived_caches_are_bounded_and_returned_bytes_do_not_alias_authoritative_records() {
    let oracle = oracle();
    let case = &oracle.cases[0];
    let copy = Copy::new(case);
    let registry = copy.import();
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    for i in 0..51 {
        owner
            .read(ChunkKey {
                x: i % 12,
                y: i / 12 + 2,
            })
            .unwrap();
    }
    assert_eq!(owner.camera_keys_lru().len(), 49);
    assert!(!owner.camera_keys_lru().contains(&ChunkKey { x: 0, y: 2 }));
    owner.read(ChunkKey { x: 2, y: 2 }).unwrap();
    assert_eq!(
        owner.camera_keys_lru().last(),
        Some(&ChunkKey { x: 2, y: 2 })
    );
    let active = owner.active_keys()[0];
    let before = owner.private_active_bytes(active).unwrap().to_vec();
    let snapshot = owner.snapshot(&[ChunkKey { x: 2, y: 2 }]).unwrap();
    let mut derived = snapshot.private_record_bytes(active).unwrap().to_vec();
    derived.fill(b' ');
    assert_eq!(owner.private_active_bytes(active).unwrap(), before);
    owner.snapshot(&[]).unwrap();
    assert_eq!(owner.snapshot_cache_len(), owner.active_keys().len());
    let mut read = owner.read(active).unwrap();
    read.fill(b' ');
    assert_eq!(owner.private_active_bytes(active).unwrap(), before);
    let mut edited = value(owner.private_active_bytes(active).unwrap());
    edited["private_edit"] = json!("owned");
    owner
        .replace_active(active, edited.to_string().as_bytes())
        .unwrap();
    owner.flush().unwrap();
    let writes = owner.take_write_intents();
    assert!(!writes.is_empty());
    owner.flush().unwrap();
    assert!(owner.take_write_intents().is_empty());
    assert_error(owner.snapshot(&[active; 50]));
}
#[test]
fn invalid_reads_edits_clock_movement_and_failed_activation_are_atomic() {
    let oracle = oracle();
    let case = &oracle.cases[0];
    let copy = Copy::new(case);
    let registry = copy.import();
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    owner.snapshot(&[key("2,5")]).unwrap();
    let before = state(&owner, true);
    let cached = owner.snapshot_cache_len();
    assert_error(owner.read(key("48,0")));
    assert_error(owner.snapshot(&[key("48,0")]));
    assert_error(owner.set_clock(b"1e999"));
    assert_error(owner.set_clock(b"-1"));
    assert_error(owner.move_founder("synthetic-town", f64::NAN, 0.0));
    assert_error(owner.move_founder("SYNTHETIC_SECRET", 0.0, 0.0));
    let active = owner.active_keys()[0];
    let mut bad = value(owner.private_active_bytes(active).unwrap());
    bad["industry_version"] = json!(999);
    assert_error(owner.replace_active(active, bad.to_string().as_bytes()));
    bad = value(owner.private_active_bytes(active).unwrap());
    bad["map_resources"][0]["kind"] = json!("SYNTHETIC_SECRET");
    assert_error(owner.replace_active(active, bad.to_string().as_bytes()));
    equal(&state(&owner, true), &before, "failed operations");
    assert_eq!(owner.snapshot_cache_len(), cached);
    assert!(owner.take_write_intents().is_empty());
    // A malformed saved record is read only after current active records would
    // flush. None of those staged flushes may publish after activation fails.
    let mut world: Value = serde_json::from_str(case.world.get()).unwrap();
    world["towns"]["synthetic-town"]["town_origin"] = json!({"x":0,"y":0});
    let source = format!(r#"{{"world":{},"tokens":{{}}}}"#, world);
    let mut saved = value(&owner.read(key("2,5")).unwrap());
    saved["map_resources"][0]["kind"] = json!("SYNTHETIC_SECRET");
    fs::write(copy.directory.join("2,5.json"), saved.to_string()).unwrap();
    let imported = import_with_chunk_copies(
        Some(source.as_bytes()),
        None,
        &[ChunkDirectory::new(&copy.owner, &copy.directory)],
    )
    .unwrap();
    let mut owner = ChunkLifecycle::prepare(&imported, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    owner.move_founder("synthetic-town", 80.0, 200.0).unwrap();
    let before = state(&owner, true);
    assert_error(owner.activate());
    equal(&state(&owner, true), &before, "failed activation");
    assert!(owner.take_write_intents().is_empty());
}

#[test]
fn owner_retains_private_scalar_lexemes_and_source_after_clock_and_edit_cycles() {
    let oracle = oracle();
    let mut copy = Copy::new(&oracle.cases[0]);
    copy.source = String::from_utf8_lossy(&copy.source)
        .replace("\"wood\":0.125", "\"wood\":1.2500e-1")
        .into_bytes();
    let registry = copy.import();
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    owner.set_clock(b"1.2500e-1").unwrap();
    let k = owner.active_keys()[0];
    let source = owner.private_active_bytes(k).unwrap().to_vec();
    let edited = String::from_utf8(source)
        .unwrap()
        .replace("\"clock\":0", "\"clock\":0.000e0");
    owner.replace_active(k, edited.as_bytes()).unwrap();
    owner.flush().unwrap();
    owner.snapshot(&[]).unwrap();
    let world = String::from_utf8(owner.private_world_bytes()).unwrap();
    assert!(world.contains("1.2500e-1"));
    assert!(world.contains("123456789012345678901234567890"));
    assert!(String::from_utf8_lossy(owner.private_active_bytes(k).unwrap()).contains("0.000e0"));
    assert_eq!(registry.private_source_bytes().unwrap(), copy.source);
    assert_error(owner.set_clock(b"1e309"));
}

#[test]
fn active_renew_is_transactional_and_preserves_private_extensions_and_numeric_lexemes() {
    let oracle = oracle();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.label == "active-renew-7-480")
        .unwrap();
    let copy = Copy::new(case);
    let registry = copy.import();
    let active = residents(&case.active, &copy.owner);
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &active, &[]).unwrap();
    let k = key("0,0");
    let mut record = value(owner.private_active_bytes(k).unwrap());
    record["map_resources"].as_array_mut().unwrap().push(serde_json::from_str(
        r#"{"kind":"tree","x":20,"y":10,"supply":4,"supply_max":0,"private":{"quantity":123456789012345678901234567890}}"#
    ).unwrap());
    let mut row = record["terrain"][10].as_str().unwrap().to_string();
    row.replace_range(20..21, "f");
    record["terrain"][10] = json!(row);
    owner
        .replace_active(k, record.to_string().as_bytes())
        .unwrap();
    owner.snapshot(&[]).unwrap();
    let before = state(&owner, true);
    let cached = owner.snapshot_cache_len();
    assert_error(owner.renew_active());
    equal(&state(&owner, true), &before, "failed renewal");
    assert_eq!(owner.snapshot_cache_len(), cached);
    assert!(owner.take_write_intents().is_empty());
    record["map_resources"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["supply_max"] = json!(8);
    record["private_lexeme"] = json!(0.125);
    record["private_exact"] = json!("EXACT_INTEGER");
    let text = record
        .to_string()
        .replace("\"private_lexeme\":0.125", "\"private_lexeme\":1.2500e-1")
        .replace("\"EXACT_INTEGER\"", "123456789012345678901234567890");
    owner.replace_active(k, text.as_bytes()).unwrap();
    let wood_before = value(&owner.private_world_bytes())["towns"].clone();
    owner.renew_active().unwrap();
    assert_eq!(owner.snapshot_cache_len(), 0);
    assert!(String::from_utf8_lossy(owner.private_active_bytes(k).unwrap()).contains("1.2500e-1"));
    assert!(
        String::from_utf8_lossy(owner.private_active_bytes(k).unwrap())
            .contains("123456789012345678901234567890")
    );
    assert_eq!(value(&owner.private_world_bytes())["towns"], wood_before);
    assert_eq!(
        value(&owner.private_world_bytes())["shared_map"]["chunk_clock"],
        json!(0)
    );
    owner.flush().unwrap();
    assert!(!owner.take_write_intents().is_empty());
    owner.flush().unwrap();
    assert!(owner.take_write_intents().is_empty());
    assert_eq!(registry.private_source_bytes().unwrap(), copy.source);
}

/// Recursive RawValue traversal avoids coercing huge clocks through Value.
fn exact_clock_value(bytes: &[u8]) -> Value {
    fn visit(raw: &RawValue) -> Value {
        if raw.get().starts_with('{') {
            let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_str(raw.get()).unwrap();
            fields
                .into_iter()
                .map(|(k, v)| {
                    let value = if k == "clock" || k == "chunk_clock" {
                        Value::String(v.get().into())
                    } else {
                        visit(&v)
                    };
                    (k, value)
                })
                .collect()
        } else if raw.get().starts_with('[') {
            let items: Vec<Box<RawValue>> = serde_json::from_str(raw.get()).unwrap();
            items.iter().map(|v| visit(v)).collect()
        } else {
            serde_json::from_str(raw.get()).unwrap()
        }
    }
    visit(&serde_json::from_slice::<Box<RawValue>>(bytes).unwrap())
}
#[test]
fn actual_python_exact_clock_lifecycle_traces() {
    #[derive(Deserialize)]
    struct Action {
        op: String,
        key: Option<String>,
        clock: Option<String>,
    }
    #[derive(Deserialize)]
    struct Output {
        read: Option<String>,
        active: BTreeMap<String, String>,
        memory: BTreeMap<String, String>,
        camera: Vec<String>,
        clock: String,
    }
    #[derive(Deserialize)]
    struct ClockCase {
        label: String,
        world: String,
        active: BTreeMap<String, String>,
        camera: BTreeMap<String, String>,
        actions: Vec<Action>,
        outputs: Vec<Output>,
    }
    let fixtures: Vec<ClockCase> =
        serde_json::from_slice(include_bytes!("fixtures/terrain/clock_lifecycle.json")).unwrap();
    assert_eq!(fixtures.len(), 9);
    for fixture in fixtures {
        let raw_map = |records: BTreeMap<String, String>| {
            records
                .into_iter()
                .map(|(k, v)| (k, RawValue::from_string(v).unwrap()))
                .collect()
        };
        let case = Case {
            label: fixture.label,
            world: RawValue::from_string(fixture.world).unwrap(),
            saved: BTreeMap::new(),
            active: raw_map(fixture.active),
            camera: raw_map(fixture.camera),
            active_order: vec!["0,0".into()],
            initial: Value::Null,
            actions: vec![],
            outputs: vec![],
        };
        let copy = Copy::new(&case);
        let before = copy.disk();
        let registry = copy.import();
        let active = residents(&case.active, &copy.owner);
        let camera = residents(&case.camera, &copy.owner);
        let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &active, &camera).unwrap();
        let mut memory = BTreeMap::new();
        for (i, (action, expected)) in fixture.actions.into_iter().zip(fixture.outputs).enumerate()
        {
            let label = format!("{} step {i}", case.label);
            match action.op.as_str() {
                "read" => {
                    let read = owner.read(key(action.key.as_deref().unwrap())).unwrap();
                    equal(
                        &exact_clock_value(&read),
                        &exact_clock_value(expected.read.as_ref().unwrap().as_bytes()),
                        &label,
                    );
                }
                "clock" => owner
                    .set_clock(action.clock.as_ref().unwrap().as_bytes())
                    .unwrap(),
                "flush" => owner.flush().unwrap(),
                _ => panic!("unexpected clock action"),
            }
            for (k, expected) in expected.active {
                equal(
                    &exact_clock_value(owner.private_active_bytes(key(&k)).unwrap()),
                    &exact_clock_value(expected.as_bytes()),
                    &label,
                );
            }
            for intent in owner.take_write_intents() {
                memory.insert(
                    text(intent.key()),
                    exact_clock_value(intent.private_record_bytes()),
                );
            }
            assert_eq!(memory.len(), expected.memory.len(), "{label}");
            for (k, expected) in expected.memory {
                equal(&memory[&k], &exact_clock_value(expected.as_bytes()), &label);
            }
            assert_eq!(
                owner
                    .camera_keys_lru()
                    .into_iter()
                    .map(text)
                    .collect::<Vec<_>>(),
                expected.camera,
                "{label}"
            );
            assert_eq!(
                exact_clock_value(&owner.private_world_bytes())["shared_map"]["chunk_clock"],
                expected.clock,
                "{label}"
            );
        }
        assert_eq!(copy.disk(), before);
        assert_eq!(registry.private_source_bytes().unwrap(), copy.source);
    }
}
#[test]
fn huge_clock_failures_are_atomic_and_active_reads_bypass_sleeping_arithmetic() {
    let case = oracle().cases.remove(0);
    let copy = Copy::new(&case);
    let registry = copy.import();
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    owner.take_write_intents();
    owner.read(key("2,5")).unwrap();
    let huge = format!("1{}", "0".repeat(400));
    owner.set_clock(huge.as_bytes()).unwrap();
    let world = owner.private_world_bytes();
    let camera = owner.camera_keys_lru();
    let active = owner.private_active_bytes(key("0,0")).unwrap().to_vec();
    assert_eq!(owner.read(key("0,0")).unwrap(), active);
    owner.replace_active(key("0,0"), &active).unwrap();
    assert_error(owner.read(key("2,5")));
    assert_error(owner.snapshot(&[key("2,5")]));
    assert_error(owner.set_clock(b"1e999"));
    assert_error(owner.set_clock(b"-1"));
    assert_eq!(owner.private_world_bytes(), world);
    assert_eq!(owner.camera_keys_lru(), camera);
    assert_eq!(owner.private_active_bytes(key("0,0")).unwrap(), active);
    assert!(owner.take_write_intents().is_empty());
    // Lexical float spelling and signed zero survive set/flush without inference
    // from the f64 value; a failed read above did not poison the camera cache.
    owner.set_clock(b"1.2500e2").unwrap();
    owner.read(key("2,5")).unwrap();
    assert!(
        std::str::from_utf8(&owner.read(key("2,5")).unwrap())
            .unwrap()
            .contains("\"clock\":1.2500e2")
    );
    owner.set_clock(b"-0.0").unwrap();
    assert!(
        std::str::from_utf8(&owner.private_world_bytes())
            .unwrap()
            .contains("\"chunk_clock\":-0.0")
    );
}

#[test]
fn prepared_village_migrations_feed_ambient_owner_without_mutating_chunk_copies() {
    let case = oracle()
        .cases
        .into_iter()
        .find(|c| c.label == "ordered-swamp-False")
        .unwrap();
    let copy = Copy::new(&case);
    let disk = copy.disk();
    let imported = copy.import();
    let active = residents(&case.active, &copy.owner);
    let camera = residents(&case.camera, &copy.owner);
    let chunks = ChunkLifecycle::prepare(&imported, &copy.owner, &active, &camera).unwrap();
    let prepared = chunks.private_world_bytes();
    let keys = chunks.active_keys();
    let records: Vec<_> = keys
        .iter()
        .map(|key| (*key, chunks.private_active_bytes(*key).unwrap().to_vec()))
        .collect();
    let active: Vec<_> = records
        .iter()
        .map(|(key, record)| ActiveVillageChunk {
            world: &copy.owner,
            key: *key,
            private_record: record,
        })
        .collect();
    let mut life = VillageLife::restore(copy.owner.clone(), &prepared).unwrap();
    life.advance(
        &copy.owner,
        lantern_core::units::Seconds::new(0.5).unwrap(),
        &active,
    )
    .unwrap();
    let save = life.encode();
    assert_eq!(
        VillageLife::decode(&copy.owner, &save).unwrap().encode(),
        save
    );
    assert_eq!(chunks.private_world_bytes(), prepared);
    assert_eq!(chunks.active_keys(), keys);
    for (key, bytes) in &records {
        assert_eq!(chunks.private_active_bytes(*key).unwrap(), bytes);
    }
    assert_eq!(copy.disk(), disk);
    assert_eq!(imported.private_source_bytes().unwrap(), copy.source);
}

#[test]
fn detached_save_freezes_active_records_and_keeps_every_sleeping_archive_without_acknowledging_io()
{
    let cases = oracle();
    let case = &cases.cases[0];
    let copy = Copy::new(case);
    let disk = copy.disk();
    let registry = copy.import();
    let mut owner = ChunkLifecycle::prepare(&registry, &copy.owner, &[], &[]).unwrap();
    owner.activate().unwrap();
    owner.set_clock(b"1.2500e-1").unwrap();
    owner.take_write_intents();
    let before = state(&owner, true);
    let snapshot = owner.capture_save();
    assert_eq!(state(&owner, true), before);
    let archive = std::thread::spawn(move || snapshot.encode().unwrap())
        .join()
        .unwrap();
    assert_eq!(state(&owner, true), before);
    assert!(owner.take_write_intents().is_empty());
    for chunk in registry.worlds()[0].chunks() {
        if !owner.active_keys().contains(&chunk.key()) {
            assert_eq!(&archive.records()[&chunk.key()], chunk.private_bytes());
        }
    }
    for key in owner.active_keys() {
        assert!(archive.records().contains_key(&key));
    }
    assert!(String::from_utf8_lossy(archive.private_world_bytes()).contains("1.2500e-1"));
    owner.set_clock(b"2.500e-1").unwrap();
    assert!(String::from_utf8_lossy(archive.private_world_bytes()).contains("1.2500e-1"));
    assert_eq!(copy.disk(), disk);
}

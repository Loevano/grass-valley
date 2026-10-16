use lantern_core::ids::WorldId;
use lantern_host::{BoundaryError, legacy::*};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

#[derive(Deserialize)]
struct Case {
    label: String,
    world: Box<RawValue>,
    saved: BTreeMap<String, Box<RawValue>>,
    active: BTreeMap<String, Box<RawValue>>,
    camera: BTreeMap<String, Box<RawValue>>,
    prepared_world: Box<RawValue>,
    prepared_active: BTreeMap<String, Box<RawValue>>,
    prepared_camera: BTreeMap<String, Box<RawValue>>,
    outputs: BTreeMap<String, Box<RawValue>>,
}
fn cases() -> Vec<Case> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/materialized.json")).unwrap()
}
fn key(text: &str) -> ChunkKey {
    let (x, y) = text.split_once(',').unwrap();
    ChunkKey {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
    }
}
struct Copy {
    root: PathBuf,
    directory: PathBuf,
    owner: WorldId,
}
impl Copy {
    fn new(case: &Case, registry: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-materialized-{}-{}",
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
        let (owner, source) = if registry {
            (
                "selected",
                format!(
                    r#"{{"worlds":{{"selected":{{"name":"synthetic","world":{},"tokens":{{}}}}}},"active_world":"selected"}}"#,
                    case.world.get()
                ),
            )
        } else {
            (
                "legacy",
                format!(r#"{{"world":{},"tokens":{{}}}}"#, case.world.get()),
            )
        };
        fs::write(root.join("source.json"), source).unwrap();
        for (key, record) in &case.saved {
            fs::write(directory.join(format!("{key}.json")), record.get()).unwrap();
        }
        Self {
            root,
            directory,
            owner: WorldId::new(owner).unwrap(),
        }
    }
    fn import(&self) -> ImportedRegistry {
        read_copies_with_chunks(
            &self.root.join("source.json"),
            None,
            &[ChunkDirectory::new(&self.owner, &self.directory)],
        )
        .unwrap()
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>, SystemTime)> {
    let mut entries: Vec<_> = fs::read_dir(root).unwrap().map(Result::unwrap).collect();
    entries.sort_by_key(|e| e.file_name());
    let mut output = vec![];
    for e in entries {
        let meta = e.metadata().unwrap();
        output.push((
            e.path(),
            meta.is_file().then(|| fs::read(e.path()).unwrap()),
            meta.modified().unwrap(),
        ));
        if meta.is_dir() {
            output.extend(snapshot(&e.path()));
        }
    }
    output
}
fn residents<'a>(case: &'a Case, owner: &'a WorldId) -> Vec<ResidentChunkCopy<'a>> {
    case.active
        .iter()
        .chain(case.camera.iter())
        .map(|(k, r)| ResidentChunkCopy::new(owner, key(k), r.get().as_bytes()))
        .collect()
}
fn assert_record(actual: &[u8], expected: &RawValue, label: &str) {
    assert_eq!(
        serde_json::from_slice::<Value>(actual).unwrap(),
        serde_json::from_str::<Value>(expected.get()).unwrap(),
        "{label}"
    );
    if actual != expected.get().as_bytes() {
        let offset = actual
            .iter()
            .zip(expected.get().as_bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.get().len()));
        panic!(
            "insertion order/scalars: {label} at {offset}: actual {:?}, expected {:?}",
            String::from_utf8_lossy(&actual[offset..actual.len().min(offset + 100)]),
            String::from_utf8_lossy(
                &expected.get().as_bytes()[offset..expected.get().len().min(offset + 100)]
            )
        );
    }
}
#[test]
fn actual_python_saved_first_metadata_overlays_and_seams_match_before_catchup() {
    let cases = cases();
    assert_eq!(cases.len(), 35);
    for (i, case) in cases.iter().enumerate() {
        let copy = Copy::new(case, i % 2 == 0);
        let before = snapshot(&copy.root);
        let imported = copy.import();
        let report = imported.report();
        let view = imported
            .materialize_chunked(&copy.owner, &residents(case, &copy.owner))
            .unwrap();
        assert_record(
            view.private_world_bytes(),
            &case.prepared_world,
            &case.label,
        );
        for (k, expected) in &case.outputs {
            let actual = view.read(key(k)).unwrap();
            assert_eq!(actual.key(), key(k));
            assert_record(
                actual.private_record_bytes(),
                expected,
                &format!("{} {k}", case.label),
            );
            assert_eq!(
                view.read(key(k)).unwrap().private_record_bytes(),
                actual.private_record_bytes()
            );
        }
        for (k, expected) in case
            .prepared_active
            .iter()
            .chain(case.prepared_camera.iter())
        {
            assert_record(
                view.read(key(k)).unwrap().private_record_bytes(),
                expected,
                &format!("resident {} {k}", case.label),
            );
        }
        assert_eq!(
            view.private_source_bytes(),
            imported.private_source_bytes().unwrap()
        );
        assert_eq!(imported.report(), report);
        assert_eq!(snapshot(&copy.root), before);
    }
}
#[test]
fn ordered_resident_plan_and_neighbor_precedence_are_observable() {
    let cases = cases();
    for case in cases
        .iter()
        .filter(|c| c.label.starts_with("ordered-resident"))
    {
        let copy = Copy::new(case, true);
        let imported = copy.import();
        let view = imported
            .materialize_chunked(&copy.owner, &residents(case, &copy.owner))
            .unwrap();
        assert_eq!(view.cache_plan().village_ordinals, vec![0, 1]);
        assert_ne!(case.active["2,5"].get(), case.prepared_active["2,5"].get());
        // A combined call would apply both overlapping villages against the same
        // original marker. The actual ensure plan must apply singletons in order.
        let prepared: Value = serde_json::from_slice(view.private_world_bytes()).unwrap();
        let villages = serde_json::to_vec(&prepared["shared_map"]["villages"]).unwrap();
        let combined = SwampOverlay::materialize(
            case.active["2,5"].get().as_bytes(),
            &villages,
            key("2,5"),
            480,
        )
        .unwrap();
        assert_ne!(
            combined.private_record_bytes(),
            view.read(key("2,5")).unwrap().private_record_bytes()
        );
    }
    let case = cases
        .iter()
        .find(|c| c.label == "resident-neighbor-precedence")
        .unwrap();
    let copy = Copy::new(case, false);
    let imported = copy.import();
    let warm = imported
        .materialize_chunked(&copy.owner, &residents(case, &copy.owner))
        .unwrap();
    let cold = imported.materialize_chunked(&copy.owner, &[]).unwrap();
    assert_ne!(
        warm.read(key("2,2")).unwrap().private_record_bytes(),
        cold.read(key("2,2")).unwrap().private_record_bytes()
    );
}
#[test]
fn water_thresholds_versions_and_post_overlay_neighbor_edits_are_distinguished() {
    let cases = cases();
    let count = |label: &str, k: &str, x: i64, y: i64| {
        let case = cases.iter().find(|c| c.label == label).unwrap();
        let copy = Copy::new(case, true);
        let imported = copy.import();
        let view = imported.materialize_chunked(&copy.owner, &[]).unwrap();
        let record: Value =
            serde_json::from_slice(view.read(key(k)).unwrap().private_record_bytes()).unwrap();
        assert!(record.get("clock").is_none());
        record["map_resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "water" && r["x"] == x && r["y"] == y)
            .count()
    };
    assert_eq!(count("seam-singleton", "2,2", 119, 119), 0);
    assert_eq!(count("seam-pair", "2,2", 119, 119), 0);
    assert_eq!(count("seam-corner-three", "2,2", 119, 119), 1);
    assert_eq!(count("v6-no-repair", "2,2", 119, 119), 1);
    assert_eq!(count("life-water-connects-over-corner", "2,2", 119, 119), 1);
    assert_eq!(
        count("neighbor-overlay-removes-connection", "2,2", 119, 100),
        0
    );
}
fn rejected<T>(result: Result<T, BoundaryError>) -> BoundaryError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("invalid materialization accepted"),
    }
}
#[test]
fn foreign_duplicate_and_malformed_copies_fail_without_mutation_or_private_diagnostics() {
    let mut cases = cases();
    let case = cases
        .iter_mut()
        .find(|c| c.label == "seam-singleton")
        .unwrap();
    let copy = Copy::new(case, true);
    let imported = copy.import();
    let before = snapshot(&copy.root);
    let foreign = WorldId::new("SYNTHETIC_SECRET").unwrap();
    assert!(
        !format!(
            "{:?}",
            rejected(imported.materialize_chunked(&foreign, &[]))
        )
        .contains("SYNTHETIC_")
    );
    let record = case.saved["2,2"].get().as_bytes();
    for copies in [
        vec![ResidentChunkCopy::new(&foreign, key("2,2"), record)],
        vec![
            ResidentChunkCopy::new(&copy.owner, key("2,2"), record),
            ResidentChunkCopy::new(&copy.owner, key("2,2"), record),
        ],
        vec![ResidentChunkCopy::new(&copy.owner, key("12,2"), record)],
        vec![ResidentChunkCopy::new(
            &copy.owner,
            key("2,2"),
            b"SYNTHETIC_SECRET",
        )],
    ] {
        let e = rejected(imported.materialize_chunked(&copy.owner, &copies));
        assert!(!format!("{e:?}").contains("SYNTHETIC_"));
    }
    for (pointer, value) in [
        ("/terrain_version", json!(6)),
        ("/swamp_version", json!(3)),
        ("/village_version", json!(4)),
    ] {
        let mut malformed: Value = serde_json::from_slice(record).unwrap();
        malformed[pointer.strip_prefix('/').unwrap()] = value;
        let bytes = serde_json::to_vec(&malformed).unwrap();
        assert!(
            imported
                .materialize_chunked(
                    &copy.owner,
                    &[ResidentChunkCopy::new(&copy.owner, key("2,2"), &bytes)]
                )
                .is_err()
        );
    }
    let view = imported.materialize_chunked(&copy.owner, &[]).unwrap();
    assert!(view.read(key("12,2")).is_err());
    assert_eq!(before, snapshot(&copy.root));
    // A neighbor is only sampled when a local <3 component needs a seam. A bad
    // relevant copied record rejects the whole read, never silently generates it.
    let path = copy.directory.join("3,2.json");
    let original = fs::read(&path).unwrap();
    let mut neighbor: Value = serde_json::from_slice(&original).unwrap();
    neighbor["terrain"][0] = json!("Z".repeat(40));
    fs::write(&path, serde_json::to_vec(&neighbor).unwrap()).unwrap();
    let broken = copy.import();
    let before = snapshot(&copy.root);
    let view = broken.materialize_chunked(&copy.owner, &[]).unwrap();
    let e = rejected(view.read(key("2,2")));
    assert!(!format!("{e:?}").contains("SYNTHETIC_"));
    assert_eq!(before, snapshot(&copy.root));
}
#[test]
fn retained_lexemes_clocks_cargo_and_private_extensions_are_not_rewritten() {
    let cases = cases();
    let case = cases.iter().find(|c| c.label == "saved-upgrade-7").unwrap();
    let copy = Copy::new(case, false);
    let path = copy.directory.join("2,5.json");
    let bytes = fs::read_to_string(&path)
        .unwrap()
        .replace("0.125", "1.25000e-1");
    fs::write(&path, &bytes).unwrap();
    let source_path = copy.root.join("source.json");
    let source = fs::read_to_string(&source_path)
        .unwrap()
        .replace("0.0625", "6.25000e-2");
    fs::write(&source_path, &source).unwrap();
    let before = snapshot(&copy.root);
    let imported = copy.import();
    let report = serde_json::to_string(&imported.report()).unwrap();
    let view = imported.materialize_chunked(&copy.owner, &[]).unwrap();
    let record = view.read(key("2,5")).unwrap();
    let text = std::str::from_utf8(record.private_record_bytes()).unwrap();
    for value in [
        "1.25000e-1",
        "123456789012345678901234567890",
        "SYNTHETIC_SECRET",
    ] {
        assert!(text.contains(value));
        assert!(!report.contains(value));
    }
    assert!(
        std::str::from_utf8(view.private_world_bytes())
            .unwrap()
            .contains("6.25000e-2")
    );
    let value: Value = serde_json::from_slice(record.private_record_bytes()).unwrap();
    assert_eq!(value["clock"], 0.125);
    assert_eq!(view.private_source_bytes(), source.as_bytes());
    assert_eq!(snapshot(&copy.root), before);
}

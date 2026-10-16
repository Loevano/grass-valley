use lantern_core::ids::WorldId;
use lantern_host::{BoundaryError, legacy::*};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

const SOURCE: &[u8] = include_bytes!("fixtures/chunked_registry.json");
const SEED: &str = "18446744073709551615";
const ZERO: ChunkKey = ChunkKey { x: 0, y: 0 };

struct Copies(PathBuf);
impl Copies {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-chunks-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for dir in [format!("current/{SEED}"), "old/text-seed".into()] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("registry.json"), SOURCE).unwrap();
        Self(root)
    }
    fn current(&self) -> PathBuf {
        self.0.join("current").join(SEED)
    }
    fn old(&self) -> PathBuf {
        self.0.join("old/text-seed")
    }
    fn import(
        &self,
        source: &[u8],
        required: &[ChunkKey],
    ) -> Result<ImportedRegistry, BoundaryError> {
        import_with_chunk_copies(
            Some(source),
            None,
            &[
                ChunkDirectory::new(&WorldId::new("current").unwrap(), &self.current())
                    .requiring(required),
                ChunkDirectory::new(&WorldId::new("old").unwrap(), &self.old()),
            ],
        )
    }
}
impl Drop for Copies {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn record(key: ChunkKey) -> Value {
    json!({
        "terrain": vec!["g".repeat(40); 40],
        "map_resources": [{"kind": "tree", "x": key.x * 40, "y": key.y * 40,
            "supply": 0.125, "supply_max": 10, "regeneration": 0.0625}],
        "tree_stumps": [{"x": key.x * 40 + 1, "y": key.y * 40 + 1, "remaining": 0.03125}],
        "clock": 0.125,
        "SYNTHETIC_PRIVATE_CHUNK_FIELD": {"credential": "SYNTHETIC_SECRET"}
    })
}
fn write_record(dir: &Path, key: ChunkKey, value: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    fs::write(dir.join(key.file_name()), &bytes).unwrap();
    bytes
}
fn reject(result: Result<ImportedRegistry, BoundaryError>) -> BoundaryError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("invalid chunk copy accepted"),
    }
}
fn changed(field: &str, value: Value) -> Vec<u8> {
    let mut source: Value = serde_json::from_slice(SOURCE).unwrap();
    source["worlds"]["current"]["world"]["shared_map"][field] = value;
    serde_json::to_vec(&source).unwrap()
}
// Capture every entry, file byte and mtime. Reading may change atime, which is not save mutation.
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>, SystemTime)> {
    let mut entries: Vec<_> = fs::read_dir(root).unwrap().map(Result::unwrap).collect();
    entries.sort_by_key(|e| e.file_name());
    let mut result = vec![];
    for entry in entries {
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        let bytes = metadata.is_file().then(|| fs::read(entry.path()).unwrap());
        result.push((entry.path(), bytes, metadata.modified().unwrap()));
        if metadata.is_dir() {
            result.extend(snapshot(&entry.path()));
        }
    }
    result
}

#[test]
fn all_active_and_evicted_copies_survive_without_catchup_or_private_output() {
    let copies = Copies::new();
    let far = ChunkKey { x: 11, y: 11 };
    let legacy_far = ChunkKey { x: 47, y: 47 };
    let a = write_record(&copies.current(), ZERO, &record(ZERO));
    let b = write_record(&copies.current(), far, &record(far));
    let c = write_record(&copies.old(), legacy_far, &record(legacy_far));
    let provision = copies.0.join("players.json");
    let provision_bytes = br#"{"current":{"second":{"name":"Synthetic second","town_id":"town","salt":"SYNTHETIC_PROVISION_SALT","pin_hash":"SYNTHETIC_PROVISION_HASH"}}}"#;
    fs::write(&provision, provision_bytes).unwrap();
    for path in [
        copies.0.join("registry.json"),
        provision.clone(),
        copies.current().join(ZERO.file_name()),
        copies.current().join(far.file_name()),
        copies.old().join(legacy_far.file_name()),
    ] {
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }
    let before = snapshot(&copies.0);
    let first = read_copies_with_chunks(
        &copies.0.join("registry.json"),
        Some(&provision),
        &[
            ChunkDirectory::new(&WorldId::new("old").unwrap(), &copies.old()),
            ChunkDirectory::new(&WorldId::new("current").unwrap(), &copies.current())
                .requiring(&[ZERO, far]),
        ],
    )
    .unwrap();
    let second = read_copies_with_chunks(
        &copies.0.join("registry.json"),
        Some(&provision),
        &[
            ChunkDirectory::new(&WorldId::new("current").unwrap(), &copies.current())
                .requiring(&[far, ZERO]),
            ChunkDirectory::new(&WorldId::new("old").unwrap(), &copies.old()),
        ],
    )
    .unwrap();
    assert_eq!(first.report(), second.report());
    assert_eq!(first.private_source_bytes(), Some(SOURCE));
    assert_eq!(
        first.private_provision_bytes(),
        Some(provision_bytes.as_slice())
    );
    let current = &first.worlds()[0];
    assert_eq!(current.layout(), LandscapeLayout::Chunked);
    assert_eq!(current.high_water(), 2);
    assert_eq!(
        current.towns()[0].buildings(),
        second.worlds()[0].towns()[0].buildings()
    );
    assert_eq!(
        current.towns()[0].villagers(),
        second.worlds()[0].towns()[0].villagers()
    );
    assert_eq!(current.chunks()[0].private_bytes(), a);
    assert_eq!(current.chunks()[1].key(), far);
    assert_eq!(current.chunks()[1].private_bytes(), b);
    assert_eq!(first.worlds()[1].chunks()[0].private_bytes(), c);
    assert_eq!(current.credentials().players.len(), 2);
    let report = first.report();
    let old = report.worlds[1].chunk_metadata.as_ref().unwrap();
    assert_eq!(
        (old.map_size, old.starter_offset, old.generator_version),
        (1920, 880, 1)
    );
    let meta = report.worlds[0].chunk_metadata.as_ref().unwrap();
    assert_eq!(
        (
            meta.map_size,
            meta.chunk_size,
            meta.starter_offset,
            meta.generator_version
        ),
        (480, 40, 160, 7)
    );
    assert_eq!(report.worlds[0].required_chunks_checked, 2);
    let output = format!("{} {report:?}", serde_json::to_string(&report).unwrap());
    for private in [
        SEED,
        "text-seed",
        "SYNTHETIC_",
        "regeneration",
        "pending_output",
        "9007199254740993",
    ] {
        assert!(
            !output.contains(private),
            "private value in inspection report"
        );
    }
    assert!(
        report.worlds[0]
            .chunk_coverage
            .contains("absent files unverified")
    );
    assert_eq!(before, snapshot(&copies.0));
}

#[test]
fn sparse_copies_are_explicit_and_missing_required_chunks_fail_without_generation() {
    let copies = Copies::new();
    let before = snapshot(&copies.0);
    let sparse = copies.import(SOURCE, &[]).unwrap();
    assert!(sparse.worlds().iter().all(|w| w.chunks().is_empty()));
    assert!(
        reject(copies.import(SOURCE, &[ZERO]))
            .path
            .ends_with("required[0]")
    );
    write_record(&copies.current(), ZERO, &record(ZERO));
    assert!(copies.import(SOURCE, &[ZERO]).is_ok());
    assert!(copies.import(SOURCE, &[ZERO, ZERO]).is_err());
    assert!(copies.import(SOURCE, &[ChunkKey { x: 12, y: 0 }]).is_err());
    fs::remove_file(copies.current().join(ZERO.file_name())).unwrap();
    assert!(copies.import(SOURCE, &[ZERO]).is_err());
    // Ignore the directory mtime changed by the test's deliberate create/remove.
    let after = snapshot(&copies.0);
    assert_eq!(
        before.iter().map(|(p, b, _)| (p, b)).collect::<Vec<_>>(),
        after.iter().map(|(p, b, _)| (p, b)).collect::<Vec<_>>()
    );
    assert!(import_bytes(Some(SOURCE), None).is_err());
    assert!(read_copies(&copies.0.join("registry.json"), None).is_err());
    let current = WorldId::new("current").unwrap();
    let directory = copies.current();
    let dirs = [ChunkDirectory::new(&current, &directory)];
    assert!(
        reject(import_with_chunk_copies(Some(SOURCE), None, &dirs))
            .path
            .ends_with(".chunks")
    );
    fs::remove_dir(copies.old()).unwrap();
    let before = snapshot(&copies.0);
    assert!(copies.import(SOURCE, &[]).is_err());
    assert_eq!(before, snapshot(&copies.0));
}

#[test]
fn owners_namespaces_and_directory_aliases_are_checked() {
    let copies = Copies::new();
    let current = WorldId::new("current").unwrap();
    let foreign = WorldId::new("foreign").unwrap();
    let dir = copies.current();
    for dirs in [
        vec![ChunkDirectory::new(&foreign, &dir)],
        vec![
            ChunkDirectory::new(&current, &dir),
            ChunkDirectory::new(&current, &dir),
        ],
        vec![ChunkDirectory::new(&current, &copies.0)],
    ] {
        assert!(import_with_chunk_copies(Some(SOURCE), None, &dirs).is_err());
    }
    let mut source: Value = serde_json::from_slice(SOURCE).unwrap();
    source["worlds"]["old"]["world"]["shared_map"]["terrain_seed"] = json!(SEED);
    let source = serde_json::to_vec(&source).unwrap();
    assert!(
        import_with_chunk_copies(
            Some(&source),
            None,
            &[
                ChunkDirectory::new(&current, &dir),
                ChunkDirectory::new(&WorldId::new("old").unwrap(), &dir),
            ]
        )
        .is_err()
    );
    assert!(
        import_with_chunk_copies(
            Some(include_bytes!("fixtures/ordinary_registry.json")),
            None,
            &[ChunkDirectory::new(&WorldId::new("a-world").unwrap(), &dir),]
        )
        .is_err()
    );
}

#[test]
fn metadata_rejects_unsupported_and_unsafe_references_before_reading_files() {
    let copies = Copies::new();
    for (field, value) in [
        ("chunk_size", json!(20)),
        ("map_size", json!(481)),
        ("chunked", json!(2)),
        ("chunked", json!("true")),
        ("starter_offset", json!(161)),
        ("starter_offset", json!(-40)),
        ("terrain_version", json!(8)),
        ("terrain_version", json!(0)),
        ("chunk_clock", json!(-0.1)),
        ("chunk_clock", json!("0")),
        ("terrain_seed", json!("../SYNTHETIC_SECRET")),
        ("terrain_seed", json!(".")),
        ("terrain_seed", json!("a\\b")),
        ("terrain_seed", json!("a\u{0}b")),
        ("terrain_seed", json!(1.25)),
        ("terrain_seed", json!(null)),
        ("terrain", json!([])),
        ("map_resources", json!([])),
        ("tree_stumps", json!([])),
    ] {
        let error = reject(copies.import(&changed(field, value), &[]));
        assert!(error.path.ends_with(field), "{error:?}");
        assert!(!format!("{error:?}").contains("SYNTHETIC_"));
    }
    let before = snapshot(&copies.0);
    assert!(copies.import(SOURCE, &[]).is_ok());
    assert_eq!(before, snapshot(&copies.0));
}

#[test]
fn malformed_records_fail_with_sanitized_errors_and_untouched_copies() {
    let copies = Copies::new();
    let mut variants = vec![];
    for (pointer, value) in [
        ("/terrain", json!(["g"])),
        ("/terrain/0", json!("g".repeat(41))),
        ("/map_resources/0/x", json!(40)),
        ("/map_resources/0/y", json!(-1)),
        ("/map_resources/0/x", json!(0.25)),
        ("/map_resources/0/kind", json!("")),
        ("/tree_stumps/0/x", json!(40)),
        ("/tree_stumps/0/remaining", json!(-0.125)),
        ("/clock", json!(-1)),
        ("/clock", json!("SYNTHETIC_SECRET")),
    ] {
        let mut value_record = record(ZERO);
        *value_record.pointer_mut(pointer).unwrap() = value;
        variants.push(serde_json::to_vec(&value_record).unwrap());
    }
    for (field, value) in [
        ("terrain_version", json!(8)),
        ("terrain_version", json!(6)),
        ("industry_version", json!(3)),
        ("chunked", json!(true)),
    ] {
        let mut value_record = record(ZERO);
        value_record[field] = value;
        variants.push(serde_json::to_vec(&value_record).unwrap());
    }
    let underflow = serde_json::to_string(&record(ZERO))
        .unwrap()
        .replace("0.03125", "-1e-999");
    variants.push(underflow.into_bytes());
    variants.extend([
        br#"{"SYNTHETIC_SECRET":1,"SYNTHETIC_SECRET":2}"#.to_vec(),
        br#"{"SYNTHETIC_SECRET":NaN}"#.to_vec(),
        br#"{"SYNTHETIC_SECRET":1e999}"#.to_vec(),
        b"{SYNTHETIC_SECRET".to_vec(),
    ]);
    for bytes in variants {
        fs::write(copies.current().join(ZERO.file_name()), bytes).unwrap();
        let before = snapshot(&copies.0);
        let error = reject(copies.import(SOURCE, &[]));
        assert!(error.path.starts_with("$.worlds[0].chunks[0]"));
        let output = format!("{error:?}");
        assert!(!output.contains("SYNTHETIC_"));
        assert!(!output.contains(copies.0.to_str().unwrap()));
        assert_eq!(before, snapshot(&copies.0));
    }
}

#[test]
fn unexpected_directory_entries_are_rejected_instead_of_skipped() {
    let copies = Copies::new();
    for name in [
        "00,0.json",
        "+0,0.json",
        "-1,0.json",
        "12,0.json",
        "0,12.json",
        "0,0.json.tmp",
        "SYNTHETIC_SECRET",
        "0,0,0.json",
    ] {
        let path = copies.current().join(name);
        fs::write(&path, b"SYNTHETIC_SECRET").unwrap();
        let before = snapshot(&copies.0);
        let error = reject(copies.import(SOURCE, &[]));
        assert!(!format!("{error:?}").contains("SYNTHETIC_"));
        assert_eq!(before, snapshot(&copies.0));
        fs::remove_file(path).unwrap();
    }
    let path = copies.current().join("0,0.json");
    fs::create_dir(&path).unwrap();
    assert!(copies.import(SOURCE, &[]).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_files_and_seed_directories_are_not_followed() {
    use std::os::unix::fs::symlink;
    let copies = Copies::new();
    let private = copies.0.join("SYNTHETIC_SECRET");
    fs::write(&private, serde_json::to_vec(&record(ZERO)).unwrap()).unwrap();
    let link = copies.current().join(ZERO.file_name());
    symlink(&private, &link).unwrap();
    let before = snapshot(&copies.0);
    assert!(copies.import(SOURCE, &[]).is_err());
    assert_eq!(before, snapshot(&copies.0));
    fs::remove_file(link).unwrap();
    fs::remove_dir(copies.current()).unwrap();
    symlink(copies.old(), copies.current()).unwrap();
    assert!(copies.import(SOURCE, &[]).is_err());
}

#[test]
fn seed_namespaces_preserve_python_integer_and_text_semantics() {
    let copies = Copies::new();
    for (raw, namespace) in [
        (
            "123456789012345678901234567890",
            "123456789012345678901234567890",
        ),
        ("-42", "-42"),
        ("-0", "0"),
        ("\"00042\"", "00042"),
    ] {
        let source = String::from_utf8(SOURCE.to_vec())
            .unwrap()
            .replace(SEED, raw);
        let dir = copies.0.join(namespace);
        fs::create_dir(&dir).unwrap();
        let imported = import_with_chunk_copies(
            Some(source.as_bytes()),
            None,
            &[
                ChunkDirectory::new(&WorldId::new("current").unwrap(), &dir),
                ChunkDirectory::new(&WorldId::new("old").unwrap(), &copies.old()),
            ],
        )
        .unwrap();
        assert_eq!(
            imported.worlds()[0]
                .chunk_metadata()
                .unwrap()
                .private_seed_namespace(),
            namespace
        );
        assert_eq!(imported.private_source_bytes(), Some(source.as_bytes()));
    }
}

#[test]
fn supported_generator_metadata_and_legacy_clock_absence_do_not_rewrite_chunks() {
    let copies = Copies::new();
    for version in 1..=7 {
        let mut chunk = record(ZERO);
        chunk["terrain_version"] = json!(version);
        chunk.as_object_mut().unwrap().remove("clock");
        let bytes = serde_json::to_string_pretty(&chunk)
            .unwrap()
            .replace("0.125", "1.25000e-1");
        // An unported large integer must remain valid without a lossy number parse.
        let bytes = bytes.replacen(
            "\"credential\":",
            "\"huge\": 123456789012345678901234567890, \"credential\":",
            1,
        );
        fs::write(copies.current().join(ZERO.file_name()), bytes.as_bytes()).unwrap();
        let source = changed("terrain_version", json!(version));
        let imported = copies.import(&source, &[ZERO]).unwrap();
        assert_eq!(
            imported.worlds()[0].chunks()[0].private_bytes(),
            bytes.as_bytes()
        );
        assert_eq!(
            imported.worlds()[0]
                .chunk_metadata()
                .unwrap()
                .report()
                .generator_version,
            version
        );
    }
}

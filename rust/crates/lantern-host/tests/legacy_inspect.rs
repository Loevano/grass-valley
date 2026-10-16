use lantern_host::legacy::import_bytes;
use serde_json::{Value, json};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

const SOURCE: &[u8] = include_bytes!("fixtures/migration_world.json");
const CHUNKED: &[u8] = include_bytes!("fixtures/chunked_registry.json");
const ORDINARY: &[u8] = include_bytes!("fixtures/ordinary_registry.json");

struct Copies(PathBuf);
impl Copies {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-inspect-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.json"), SOURCE).unwrap();
        Self(root)
    }
    fn source(&self) -> PathBuf {
        self.0.join("source.json")
    }
    fn inspect(&self, args: &[&OsStr]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lantern-host"))
            .args([
                OsStr::new("inspect-copy"),
                OsStr::new("--source"),
                self.source().as_os_str(),
            ])
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Copies {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn snapshot(path: &Path) -> Vec<(PathBuf, Option<Vec<u8>>, SystemTime)> {
    let mut entries: Vec<_> = fs::read_dir(path).unwrap().map(Result::unwrap).collect();
    entries.sort_by_key(|entry| entry.file_name());
    let mut result = vec![];
    for entry in entries {
        let metadata = fs::metadata(entry.path()).unwrap();
        result.push((
            entry.path(),
            metadata.is_file().then(|| fs::read(entry.path()).unwrap()),
            metadata.modified().unwrap(),
        ));
        if metadata.is_dir() {
            result.extend(snapshot(&entry.path()));
        }
    }
    result
}
fn no_private_output(output: &Output, copies: &Copies) {
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("SYNTHETIC_"), "private fixture data printed");
        assert!(
            !text.contains(copies.0.to_str().unwrap()),
            "private path printed"
        );
        assert!(!text.contains("123456789012345678901234567890"));
    }
}
fn reject_source(value: &Value) -> lantern_host::BoundaryError {
    match import_bytes(Some(&serde_json::to_vec(value).unwrap()), None) {
        Err(error) => error,
        Ok(_) => panic!("invalid reference accepted"),
    }
}

#[test]
fn report_describes_capabilities_pending_markers_and_all_opaque_payload_locations() {
    let first = import_bytes(Some(SOURCE), None).unwrap();
    let second = import_bytes(Some(SOURCE), None).unwrap();
    assert_eq!(first.report(), second.report());
    assert_eq!(first.private_source_bytes(), Some(SOURCE));
    let world = &first.worlds()[0];
    assert_eq!(world.high_water(), 3);
    assert_eq!(
        world.towns()[0].villagers(),
        second.worlds()[0].towns()[0].villagers()
    );
    let report = first.report();
    assert!(!report.capabilities.playable);
    assert!(!report.capabilities.migrations_applied);
    assert!(!report.capabilities.simulation_advanced);
    assert!(!report.capabilities.writes_enabled);
    assert!(
        report
            .capabilities
            .preserved_only
            .contains(&"task_route_trip_claim_references")
    );
    assert_eq!(report.worlds[0].validated_town_references, 3);
    assert_eq!(report.worlds[0].bound_entity_references, 1);
    assert!(
        report.worlds[0]
            .versions
            .iter()
            .any(|v| v.path.ends_with("inventory_version")
                && v.saved == 0
                && v.status.starts_with("older_marker"))
    );
    assert!(
        report.worlds[0]
            .versions
            .iter()
            .any(|v| v.path.ends_with("terrain_version")
                && v.saved == 1
                && v.status.starts_with("supported_generator"))
    );
    for path in [
        "$.fields[2]",
        "$.world.fields[4]",
        "$.world.shared_map.fields[8]",
        "$.world.towns[0].fields[10]",
    ] {
        assert!(
            report.preserved.iter().any(|p| p.path == path),
            "missing opaque field diagnostic at {path}"
        );
    }
    let raw: Value = serde_json::from_slice(first.private_source_bytes().unwrap()).unwrap();
    // Quantities stay in their independent persisted locations, without refunds,
    // completion, trip cancellation or an f64 rewrite of the opaque raw archive.
    let town = &raw["world"]["towns"]["first"];
    let total = town["wood"].as_f64().unwrap()
        + town["buildings"][0]["stored_goods"]["wood"]
            .as_f64()
            .unwrap()
        + town["villagers"][0]["pending_output"]["wood"]
            .as_f64()
            .unwrap()
        + town["villagers"][0]["logistics_trip"]["cargo"]["wood"]
            .as_f64()
            .unwrap()
        + town["player"]["inventory"]["wood"].as_f64().unwrap();
    assert_eq!(total, 12.359375);
    assert!(
        String::from_utf8_lossy(first.private_source_bytes().unwrap()).contains("1.2345600e-3")
    );
}

#[test]
fn foreign_and_ambiguous_persisted_town_and_queue_references_are_rejected() {
    let base: Value = serde_json::from_slice(SOURCE).unwrap();
    for (pointer, value, suffix) in [
        (
            "/world/towns/first/trade_offers/0/target_town",
            json!("foreign"),
            ".target_town",
        ),
        (
            "/world/towns/first/trade_offers/0/target_town",
            json!("first"),
            ".target_town",
        ),
        (
            "/world/towns/first/trade_offers/0/target_town",
            json!(3),
            ".target_town",
        ),
        ("/world/towns/first/trade_offers/0/id", json!(""), ".id"),
        (
            "/world/quarry/members",
            json!({"SYNTHETIC_FOREIGN": {}}),
            ".members[0]",
        ),
        ("/world/lanterns/members/second", json!(null), ".members[0]"),
        (
            "/world/towns/first/buildings/0/craft_queue",
            json!([{"id":"same"},{"id":"same"}]),
            ".id",
        ),
    ] {
        let mut source = base.clone();
        *source.pointer_mut(pointer).unwrap() = value;
        let error = reject_source(&source);
        assert!(error.path.ends_with(suffix), "{error:?}");
        assert!(!format!("{error:?}").contains("SYNTHETIC_"));
    }
    let mut source = base.clone();
    source["world"]["towns"]["first"]["trade_offers"] = json!([
        {"id":"same", "target_town":"second"}, {"id":"same", "target_town":"second"}
    ]);
    assert!(reject_source(&source).path.ends_with("[1].id"));
    // A target found only in a different registry world is still foreign.
    let mut registry: Value = serde_json::from_slice(ORDINARY).unwrap();
    registry["worlds"]["a-world"]["world"]["towns"]["z-town"]["trade_offers"] = json!([
        {"id":"opaque", "target_town":"a-town"}
    ]);
    assert!(reject_source(&registry).path.ends_with(".target_town"));
}

#[test]
fn cli_reports_are_repeatable_private_and_read_only_for_both_envelopes() {
    let copies = Copies::new();
    for source in [
        SOURCE,
        ORDINARY,
        include_bytes!("fixtures/legacy_world.json"),
    ] {
        fs::write(copies.source(), source).unwrap();
        let before = snapshot(&copies.0);
        let first = copies.inspect(&[]);
        let second = copies.inspect(&[]);
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        assert!(first.stderr.is_empty());
        assert_eq!(first.stdout, second.stdout);
        let report: Value = serde_json::from_slice(&first.stdout).unwrap();
        assert_eq!(report["readiness"], "preserved_only");
        assert_eq!(report["capabilities"]["simulation_advanced"], false);
        no_private_output(&first, &copies);
        assert_eq!(before, snapshot(&copies.0));
    }
}

#[test]
fn cli_provisioning_is_explicit_and_secrets_never_enter_reports() {
    let copies = Copies::new();
    fs::write(copies.source(), ORDINARY).unwrap();
    let provision = copies.0.join("private.players.json");
    fs::write(
        &provision,
        include_bytes!("fixtures/ordinary_registry.players.json"),
    )
    .unwrap();
    let before = snapshot(&copies.0);
    let explicit = copies.inspect(&[OsStr::new("--provision"), provision.as_os_str()]);
    assert!(explicit.status.success());
    no_private_output(&explicit, &copies);
    let explicit: Value = serde_json::from_slice(&explicit.stdout).unwrap();
    let plain: Value = serde_json::from_slice(&copies.inspect(&[]).stdout).unwrap();
    assert_eq!(plain["worlds"][0]["accounts"], 1);
    assert_eq!(explicit["worlds"][0]["accounts"], 2);
    assert_eq!(before, snapshot(&copies.0));
}

#[test]
fn cli_chunk_copies_and_required_keys_are_validated_without_generation() {
    let copies = Copies::new();
    fs::write(copies.source(), CHUNKED).unwrap();
    let current = copies.0.join("current/18446744073709551615");
    let old = copies.0.join("old/text-seed");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&old).unwrap();
    let chunk = json!({"terrain":vec!["g".repeat(40);40], "map_resources":[], "tree_stumps":[], "clock":0.125, "SYNTHETIC_CHUNK": "SYNTHETIC_SECRET"});
    fs::write(
        current.join("0,0.json"),
        serde_json::to_vec(&chunk).unwrap(),
    )
    .unwrap();
    let dirs = [
        OsStr::new("--chunks"),
        OsStr::new("current"),
        current.as_os_str(),
        OsStr::new("--chunks"),
        OsStr::new("old"),
        old.as_os_str(),
    ];
    let before = snapshot(&copies.0);
    let output = copies.inspect(&dirs);
    assert!(output.status.success());
    no_private_output(&output, &copies);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["worlds"][0]["chunk_files"], 1);
    assert!(
        report["worlds"][1]["chunk_coverage"]
            .as_str()
            .unwrap()
            .contains("unverified")
    );
    let mut args = dirs.to_vec();
    args.extend([
        OsStr::new("--require-chunk"),
        OsStr::new("current"),
        OsStr::new("0"),
        OsStr::new("0"),
    ]);
    assert!(copies.inspect(&args).status.success());
    *args.last_mut().unwrap() = OsStr::new("1");
    let failed = copies.inspect(&args);
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    no_private_output(&failed, &copies);
    assert_eq!(before, snapshot(&copies.0));
}

#[test]
fn cli_errors_never_echo_paths_contents_unknown_field_names_or_arguments() {
    let copies = Copies::new();
    for source in [
        br#"{"world":{"towns":{}},"tokens":{"SYNTHETIC_SECRET":"SYNTHETIC_FOREIGN"}}"#.as_slice(),
        br#"{"world":{"towns":{},"terrain_version":999},"tokens":{}}"#.as_slice(),
        br#"{"SYNTHETIC_SECRET":1,"SYNTHETIC_SECRET":2}"#.as_slice(),
        b"SYNTHETIC_INVALID_JSON".as_slice(),
        b"".as_slice(),
        CHUNKED,
    ] {
        fs::write(copies.source(), source).unwrap();
        let before = snapshot(&copies.0);
        let output = copies.inspect(&[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        no_private_output(&output, &copies);
        assert_eq!(before, snapshot(&copies.0));
    }
    for args in [
        vec!["inspect-copy"],
        vec!["inspect-copy", "--source"],
        vec!["inspect-copy", "--source", "SYNTHETIC_MISSING_PATH"],
        vec!["inspect-copy", "--SYNTHETIC_UNKNOWN_ARGUMENT"],
        vec!["inspect-copy", "--source", "x", "--source", "y"],
        vec![
            "inspect-copy",
            "--require-chunk",
            "SYNTHETIC_OWNER",
            "0",
            "0",
        ],
        vec![
            "inspect-copy",
            "--require-chunk",
            "SYNTHETIC_OWNER",
            "NaN",
            "0",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        no_private_output(&output, &copies);
    }
    let help = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
        .args(["inspect-copy", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn non_utf8_options_and_missing_paths_fail_without_panics() {
    use std::os::unix::ffi::OsStringExt;
    let copies = Copies::new();
    let source = copies
        .0
        .join(OsString::from_vec(b"copy-\xff.json".to_vec()));
    let output = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
        .args([
            OsStr::new("inspect-copy"),
            OsStr::new("--source"),
            source.as_os_str(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    no_private_output(&output, &copies);
    let output = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
        .arg("inspect-copy")
        .arg(OsString::from_vec(b"--\xff".to_vec()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    no_private_output(&output, &copies);
}

use lantern_core::{
    ids::{Owner, TownId, WorldId},
    model::Town,
    units::{Quantity, TerrainSeed},
};
use lantern_host::{
    persistence::{RegistrySnapshot, Store, TownState, WorldSnapshot},
    resources::Ledger,
    state::{ModelWorld, PrivateAccount, PrivateCredentials},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "lantern-save-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lantern-host"))
        .args(args)
        .output()
        .unwrap()
}
fn fixture(n: f64) -> RegistrySnapshot {
    let id = WorldId::new("SYNTHETIC_PRIVATE_WORLD").unwrap();
    let owner = Owner {
        world: id.clone(),
        town: TownId::new("private-town").unwrap(),
    };
    let town = Town::restore(owner.clone(), vec![], vec![]).unwrap();
    let credentials = PrivateCredentials {
        tokens: BTreeMap::from([("SYNTHETIC_TOKEN".into(), owner.town.clone())]),
        players: BTreeMap::from([(
            "private-key".into(),
            PrivateAccount {
                name: "SYNTHETIC_ACCOUNT".into(),
                town_id: owner.town.clone(),
                salt: "SYNTHETIC_SALT".into(),
                pin_hash: "SYNTHETIC_HASH".into(),
            },
        )]),
    };
    let world = ModelWorld::new(id, TerrainSeed(1), 0, vec![town], credentials).unwrap();
    let state =
        TownState::empty(Ledger::new(owner, Quantity::new(n).unwrap(), vec![], vec![]).unwrap());
    RegistrySnapshot::capture(
        Some(world.id()),
        vec![WorldSnapshot::capture(&world, &[state])],
    )
}
fn private(output: &Output, temp: &Scratch) {
    for b in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(b);
        assert!(!text.contains("SYNTHETIC_"));
        assert!(!text.contains(temp.0.to_str().unwrap()));
        assert!(!text.contains("private-key"));
    }
}
fn tree(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = vec![];
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap())
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if entry.file_type().unwrap().is_dir() {
            result.extend(tree(&entry.path()));
        } else {
            result.push((entry.path(), fs::read(entry.path()).unwrap()));
        }
    }
    result
}
#[test]
fn native_cli_copy_inspect_recovery_and_pending_cleanup_are_explicit_and_private() {
    let temp = Scratch::new();
    let source = temp.0.join("source");
    let mut store = Store::create(&source).unwrap();
    store.commit(fixture(1.)).unwrap();
    store.commit(fixture(2.)).unwrap();
    drop(store);
    let before = tree(&source);
    let copied = temp.0.join("copied");
    let output = run(&[
        OsStr::new("save-convert"),
        OsStr::new("--source-format"),
        OsStr::new("native"),
        OsStr::new("--source"),
        source.as_os_str(),
        OsStr::new("--destination"),
        copied.as_os_str(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    private(&output, &temp);
    assert_eq!(tree(&source), before);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source_generation"], json!(2));
    assert_eq!(report["source_changed"], json!(false));
    let output = run(&[
        OsStr::new("save-inspect"),
        OsStr::new("--source"),
        source.as_os_str(),
    ]);
    assert!(output.status.success());
    private(&output, &temp);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["selected_generation"], json!(2));
    fs::remove_file(source.join("generation-00000000000000000002/00000000.bin")).unwrap();
    let damaged = tree(&source);
    let recovered = temp.0.join("recovered");
    let output = run(&[
        OsStr::new("save-recover"),
        OsStr::new("--source"),
        source.as_os_str(),
        OsStr::new("--destination"),
        recovered.as_os_str(),
    ]);
    assert!(output.status.success());
    private(&output, &temp);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source_generation"], json!(1));
    assert_eq!(report["skipped_generations"], json!([2]));
    assert_eq!(tree(&source), damaged);
    let pending = source.join("pending-00000000000000000003");
    fs::create_dir(&pending).unwrap();
    fs::write(pending.join("partial"), b"partial").unwrap();
    let output = run(&[
        OsStr::new("save-cleanup"),
        OsStr::new("--source"),
        source.as_os_str(),
    ]);
    assert!(!output.status.success());
    assert!(pending.exists());
    let output = run(&[
        OsStr::new("save-cleanup"),
        OsStr::new("--source"),
        source.as_os_str(),
        OsStr::new("--pending-only"),
    ]);
    assert!(output.status.success());
    private(&output, &temp);
    assert!(!pending.exists());
    assert_eq!(tree(&source), damaged);
}
#[test]
fn python_archive_cli_roundtrip_preserves_credentials_and_never_claims_playability() {
    let temp = Scratch::new();
    let source = temp.0.join("source.json");
    let provision = temp.0.join("provision.json");
    let bytes = include_bytes!("fixtures/ordinary_registry.json");
    let accounts = include_bytes!("fixtures/ordinary_registry.players.json");
    fs::write(&source, bytes).unwrap();
    fs::write(&provision, accounts).unwrap();
    let archive = temp.0.join("archive");
    let output = run(&[
        OsStr::new("save-convert"),
        OsStr::new("--source-format"),
        OsStr::new("python-archive"),
        OsStr::new("--source"),
        source.as_os_str(),
        OsStr::new("--provision"),
        provision.as_os_str(),
        OsStr::new("--destination"),
        archive.as_os_str(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    private(&output, &temp);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"], json!("preserved-only"));
    assert_eq!(report["playable_native_import"], json!(false));
    let before = tree(&archive);
    let output = run(&[
        OsStr::new("save-inspect"),
        OsStr::new("--source-format"),
        OsStr::new("python-archive"),
        OsStr::new("--source"),
        archive.as_os_str(),
    ]);
    assert!(output.status.success());
    private(&output, &temp);
    let restored = temp.0.join("restored");
    let output = run(&[
        OsStr::new("save-restore-python"),
        OsStr::new("--source"),
        archive.as_os_str(),
        OsStr::new("--destination"),
        restored.as_os_str(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    private(&output, &temp);
    assert_eq!(fs::read(restored.join("savegame.json")).unwrap(), bytes);
    assert_eq!(
        fs::read(restored.join("savegame.players.json")).unwrap(),
        accounts
    );
    assert_eq!(tree(&archive), before);
    assert_eq!(fs::read(source).unwrap(), bytes);
}
#[test]
fn cli_rejects_missing_ambiguous_paths_unsupported_playable_conversion_and_reverse_export() {
    let temp = Scratch::new();
    let source = temp.0.join("source.json");
    fs::write(&source, include_bytes!("fixtures/legacy_world.json")).unwrap();
    let target = temp.0.join("target");
    for args in [
        vec!["save-convert", "--source-format", "python"],
        vec!["save-convert"],
        vec!["save-recover", "--generation", "0"],
        vec!["save-recover", "--generation", "01"],
        vec![
            "save-convert",
            "--source-format",
            "native",
            "--source-format",
            "native",
        ],
    ] {
        let mut args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        args.extend([
            OsString::from("--source"),
            source.clone().into(),
            OsString::from("--destination"),
            target.clone().into(),
        ]);
        let refs = args.iter().map(|s| s.as_os_str()).collect::<Vec<_>>();
        let output = run(&refs);
        assert!(!output.status.success());
        private(&output, &temp);
        assert!(!target.exists());
    }
    let output = run(&[
        OsStr::new("save-export"),
        OsStr::new("--source"),
        source.as_os_str(),
        OsStr::new("--destination"),
        target.as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    private(&output, &temp);
    assert!(!target.exists());
}

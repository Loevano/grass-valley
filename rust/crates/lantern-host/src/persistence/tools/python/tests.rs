use super::*;
use crate::persistence::Store;
use std::sync::atomic::{AtomicU64, Ordering};
const ORDINARY: &[u8] = include_bytes!("../../../../tests/fixtures/ordinary_registry.json");
const LEGACY: &[u8] = include_bytes!("../../../../tests/fixtures/legacy_world.json");
const CHUNKED: &[u8] = include_bytes!("../../../../tests/fixtures/chunked_registry.json");
const PROVISION: &[u8] =
    include_bytes!("../../../../tests/fixtures/ordinary_registry.players.json");
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-python-archive-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    let mut entries = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap())
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if e.file_type().unwrap().is_dir() {
            out.extend(snapshot(&e.path()));
        } else {
            out.push((e.path(), fs::read(e.path()).unwrap()));
        }
    }
    out
}
#[test]
fn ordinary_and_legacy_archives_restore_exact_source_and_private_accounts() {
    for source in [ORDINARY, LEGACY] {
        let temp = Scratch::new();
        let input = temp.0.join("input");
        fs::create_dir(&input).unwrap();
        let file = input.join("source.json");
        fs::write(&file, source).unwrap();
        let provision = input.join("provision.json");
        fs::write(&provision, PROVISION).unwrap();
        let before = snapshot(&input);
        let archive_path = temp.0.join("archive");
        let report = archive(&file, Some(&provision), &[], &archive_path).unwrap();
        assert!(report.worlds > 0);
        assert!(report.provisioning_present);
        assert_eq!(snapshot(&input), before);
        assert!(Store::open_read_only(&archive_path).is_err());
        let archive_before = snapshot(&archive_path);
        assert_eq!(inspect(&archive_path).unwrap(), report);
        let output = temp.0.join("restored");
        assert_eq!(restore(&archive_path, &output).unwrap(), report);
        assert_eq!(fs::read(output.join("savegame.json")).unwrap(), source);
        assert_eq!(
            fs::read(output.join("savegame.players.json")).unwrap(),
            PROVISION
        );
        assert_eq!(snapshot(&archive_path), archive_before);
    }
}
fn chunks(temp: &Scratch) -> (PathBuf, Vec<ChunkSource>) {
    let source = temp.0.join("source.json");
    fs::write(&source, CHUNKED).unwrap();
    let current = temp.0.join("current/18446744073709551615");
    let old = temp.0.join("old/text-seed");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&old).unwrap();
    let record = serde_json::json!({"terrain_version":7,"terrain":vec!["g".repeat(40);40],"map_resources":[],"tree_stumps":[],"clock":9007199254740993_u64,"extension":"SYNTHETIC_SECRET"});
    fs::write(
        current.join("0,0.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    let mut legacy_record = record;
    legacy_record["terrain_version"] = serde_json::json!(1);
    fs::write(
        old.join("1,1.json"),
        serde_json::to_vec(&legacy_record).unwrap(),
    )
    .unwrap();
    (
        source,
        vec![
            ChunkSource {
                world: WorldId::new("current").unwrap(),
                directory: current,
                required: vec![ChunkKey { x: 0, y: 0 }],
            },
            ChunkSource {
                world: WorldId::new("old").unwrap(),
                directory: old,
                required: vec![ChunkKey { x: 1, y: 1 }],
            },
        ],
    )
}
#[test]
fn current_and_sleeping_legacy_external_chunks_are_complete_and_missing_files_reject() {
    let temp = Scratch::new();
    let (source, sources) = chunks(&temp);
    let path = temp.0.join("archive");
    let report = archive(&source, None, &sources, &path).unwrap();
    assert_eq!(report.chunks, 2);
    let restored = temp.0.join("restored");
    restore(&path, &restored).unwrap();
    for c in &sources {
        let seed = c.directory.file_name().unwrap();
        for key in &c.required {
            assert_eq!(
                fs::read(
                    restored
                        .join("savegame_chunks")
                        .join(seed)
                        .join(key.file_name())
                )
                .unwrap(),
                fs::read(c.directory.join(key.file_name())).unwrap()
            );
        }
    }
    fs::remove_file(sources[1].directory.join("1,1.json")).unwrap();
    assert!(archive(&source, None, &sources, &temp.0.join("bad-copy")).is_err());
    assert!(!temp.0.join("bad-copy").exists());
    fs::remove_file(path.join("00000001.bin")).unwrap();
    assert!(inspect(&path).is_err());
    assert!(restore(&path, &temp.0.join("bad-restore")).is_err());
    assert!(!temp.0.join("bad-restore").exists());
}
#[test]
fn malformed_future_duplicate_and_alias_inputs_never_create_a_destination() {
    let temp = Scratch::new();
    let source = temp.0.join("source.json");
    for bytes in [b"{".as_slice(), br#"{"world":{},"world":{}}"#.as_slice()] {
        fs::write(&source, bytes).unwrap();
        assert!(archive(&source, None, &[], &temp.0.join("bad")).is_err());
        assert!(!temp.0.join("bad").exists());
    }
    fs::write(&source, ORDINARY).unwrap();
    assert!(archive(&source, None, &[], &source).is_err());
    let (source, sources) = chunks(&temp);
    assert!(
        archive(
            &source,
            None,
            &sources,
            &sources[0].directory.join("nested")
        )
        .is_err()
    );
    assert!(!sources[0].directory.join("nested").exists());
    let mut future: serde_json::Value = serde_json::from_slice(CHUNKED).unwrap();
    future["worlds"]["current"]["world"]["shared_map"]["terrain_version"] = serde_json::json!(8);
    fs::write(&source, serde_json::to_vec(&future).unwrap()).unwrap();
    assert!(archive(&source, None, &sources, &temp.0.join("future")).is_err());
    assert!(!temp.0.join("future").exists());
}
#[test]
fn interrupted_archive_writes_are_never_misread_as_ready_archives() {
    for failure in [
        Checkpoint::Created,
        Checkpoint::FileWritten(0),
        Checkpoint::FilesSynced,
        Checkpoint::ManifestSynced,
        Checkpoint::Ready,
    ] {
        let temp = Scratch::new();
        let source = temp.0.join("source.json");
        fs::write(&source, ORDINARY).unwrap();
        let path = temp.0.join("archive");
        let result = archive_inner(&source, None, &[], &path, |phase| {
            if phase == failure {
                Err(crate::persistence::Error::Io(
                    std::io::ErrorKind::StorageFull,
                ))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        if failure == Checkpoint::Ready {
            inspect(&path).unwrap();
        } else {
            assert!(inspect(&path).is_err());
        }
        assert_eq!(fs::read(&source).unwrap(), ORDINARY);
    }
}

#[test]
fn abrupt_archive_exit_at_each_chunk_and_publication_boundary_preserves_inputs() {
    let temp = Scratch::new();
    let (source, sources) = chunks(&temp);
    let original = fs::read(&source).unwrap();
    let before = sources
        .iter()
        .map(|c| snapshot(&c.directory))
        .collect::<Vec<_>>();
    let mut count = 0;
    archive_inner(&source, None, &sources, &temp.0.join("probe"), |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    for phase in 0..count {
        let target = temp.0.join(format!("crashed-{phase}"));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::tools::python::tests::archive_crash_child",
            ])
            .env("LANTERN_TEST_ARCHIVE_SOURCE", &source)
            .env("LANTERN_TEST_ARCHIVE_TARGET", &target)
            .env("LANTERN_TEST_ARCHIVE_PHASE", phase.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if phase + 1 == count {
            inspect(&target).unwrap();
        } else {
            assert!(inspect(&target).is_err());
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(
            sources
                .iter()
                .map(|c| snapshot(&c.directory))
                .collect::<Vec<_>>(),
            before
        );
    }
}
#[test]
fn archive_crash_child() {
    let Some(source) = std::env::var_os("LANTERN_TEST_ARCHIVE_SOURCE") else {
        return;
    };
    let source = PathBuf::from(source);
    let parent = source.parent().unwrap();
    let sources = vec![
        ChunkSource {
            world: WorldId::new("current").unwrap(),
            directory: parent.join("current/18446744073709551615"),
            required: vec![ChunkKey { x: 0, y: 0 }],
        },
        ChunkSource {
            world: WorldId::new("old").unwrap(),
            directory: parent.join("old/text-seed"),
            required: vec![ChunkKey { x: 1, y: 1 }],
        },
    ];
    let target = std::env::var_os("LANTERN_TEST_ARCHIVE_TARGET").unwrap();
    let phase: usize = std::env::var("LANTERN_TEST_ARCHIVE_PHASE")
        .unwrap()
        .parse()
        .unwrap();
    let mut current = 0;
    archive_inner(&source, None, &sources, &PathBuf::from(target), |_| {
        if current == phase {
            std::process::exit(73)
        }
        current += 1;
        Ok(())
    })
    .unwrap();
    panic!("missing archive phase");
}

#[test]
fn interrupted_python_restore_never_publishes_registry_before_chunks_are_complete() {
    let temp = Scratch::new();
    let (source, sources) = chunks(&temp);
    let path = temp.0.join("archive");
    archive(&source, None, &sources, &path).unwrap();
    let before = snapshot(&path);
    for (index, phase) in [
        RestoreCheckpoint::Created,
        RestoreCheckpoint::DataSynced,
        RestoreCheckpoint::RegistryPublished,
        RestoreCheckpoint::Ready,
    ]
    .into_iter()
    .enumerate()
    {
        let target = temp.0.join(format!("restore-{index}"));
        assert!(
            restore_inner(&path, &target, |p| if p == phase {
                Err(crate::persistence::Error::Io(
                    std::io::ErrorKind::StorageFull,
                ))
            } else {
                Ok(())
            })
            .is_err()
        );
        if matches!(
            phase,
            RestoreCheckpoint::RegistryPublished | RestoreCheckpoint::Ready
        ) {
            assert_eq!(fs::read(target.join("savegame.json")).unwrap(), CHUNKED);
            for c in &sources {
                for key in &c.required {
                    assert_eq!(
                        fs::read(
                            target
                                .join("savegame_chunks")
                                .join(c.directory.file_name().unwrap())
                                .join(key.file_name())
                        )
                        .unwrap(),
                        fs::read(c.directory.join(key.file_name())).unwrap()
                    );
                }
            }
        } else {
            assert!(!target.join("savegame.json").exists());
        }
        assert_eq!(snapshot(&path), before);
    }
}

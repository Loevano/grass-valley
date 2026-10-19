use super::*;
use super::{snapshot::ChunkBlob, store::Checkpoint};
use crate::{
    dto::encode_save,
    legacy::{ChunkKey, TerrainArchive},
    resources::{
        CargoKind, HeldCargo, Ledger, PendingOwner, ResourceKey, Stock, StoragePolicy,
        Store as ResourceStore,
    },
    state::{ModelWorld, PrivateAccount, PrivateCredentials},
};
use lantern_core::{
    ids::{IdAllocator, Owner, TownId, WorldId},
    model::{ActorState, Building, Cargo, ConstructionState, Town},
    python_random::PythonRandom,
    units::{Position, Quantity, Seconds, TerrainSeed, Tile},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lantern-generations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn root(&self) -> PathBuf {
        self.0.join("rust-data")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn stock(n: f64) -> Stock {
    Stock::from_entries([(ResourceKey::parse("wood").unwrap(), q(n))]).unwrap()
}
fn fixtures(coins: f64) -> (ModelWorld, TownState) {
    let id = WorldId::new("synthetic-world").unwrap();
    let owner = Owner {
        world: id.clone(),
        town: TownId::new("town").unwrap(),
    };
    let mut ids = IdAllocator::new(id.clone());
    let hall = ids.allocate(&owner).unwrap();
    let store = ids.allocate(&owner).unwrap();
    let actor = ids.allocate(&owner).unwrap();
    let town = Town::restore(
        owner.clone(),
        vec![
            Building {
                id: hall.clone(),
                location: Tile { x: 0, y: 0 },
                construction: ConstructionState::Complete,
            },
            Building {
                id: store.clone(),
                location: Tile { x: 4, y: 0 },
                construction: ConstructionState::Complete,
            },
        ],
        vec![ActorState::new(
            actor.clone(),
            Position::new(2.375, 0.125).unwrap(),
            Cargo::from_entries([("fish".into(), q(0.125))]).unwrap(),
        )],
    )
    .unwrap();
    let mut stores = vec![
        ResourceStore::new(hall, StoragePolicy::new("hall")),
        ResourceStore::new(store.clone(), StoragePolicy::new("storehouse")),
    ];
    stores[0].stored = stock(12.375);
    stores[1]
        .pending
        .insert(PendingOwner::Worker(actor.clone()), stock(0.375));
    let mut ledger = Ledger::new(
        owner.clone(),
        q(coins),
        stores,
        vec![HeldCargo {
            actor: actor.clone(),
            source: store,
            kind: CargoKind::Collect,
            goods: stock(0.625),
        }],
    )
    .unwrap();
    ledger.restore_caps(BTreeMap::from([(
        ResourceKey::parse("wood").unwrap(),
        q(0.),
    )]));
    ledger.restore_market_sales(q(12.125));
    ledger.credit_coins(q(0.25)).unwrap();
    let mut credentials = PrivateCredentials::default();
    credentials
        .tokens
        .insert("SYNTHETIC_TOKEN".into(), owner.town.clone());
    credentials.players.insert(
        "synthetic".into(),
        PrivateAccount {
            name: "Synthetic".into(),
            town_id: owner.town,
            salt: "SYNTHETIC_SALT".into(),
            pin_hash: "SYNTHETIC_HASH".into(),
        },
    );
    let world = ModelWorld::new(
        id,
        TerrainSeed(7),
        ids.high_water(),
        vec![town],
        credentials,
    )
    .unwrap();
    let mut state = TownState::empty(ledger);
    let mut worker = crate::progression::Worker::new("hauling").unwrap();
    worker
        .set_practice(
            "hauling",
            lantern_core::units::Practice::new(3.125).unwrap(),
        )
        .unwrap();
    worker.set_talents(BTreeMap::new()).unwrap();
    state.workers.insert(actor, worker);
    (world, state)
}
fn terrain(stamp: f64) -> TerrainArchive {
    let payload = json!({"towns":{},"shared_map":{"chunked":true,"chunk_size":40,"map_size":480,"terrain_seed":7,"starter_offset":0,"terrain_version":7,"chunk_clock":9007199254740993_u64}});
    let record = json!({"terrain_version":7,"terrain":vec!["g".repeat(40);40],"map_resources":[],"tree_stumps":[],"clock":9007199254740993_u64,"private_extension":"synthetic","stamp":stamp});
    TerrainArchive::decode(
        &serde_json::to_vec(&payload).unwrap(),
        vec![
            (
                ChunkKey { x: 0, y: 0 },
                serde_json::to_vec(&record).unwrap(),
            ),
            (
                ChunkKey { x: 2, y: 2 },
                serde_json::to_vec(&record).unwrap(),
            ),
        ],
    )
    .unwrap()
}
fn snapshot(coins: f64) -> RegistrySnapshot {
    let (world, state) = fixtures(coins);
    let mut stream = PythonRandom::from_integer("777").unwrap();
    for _ in 0..23 {
        stream.random();
    }
    let random = BTreeMap::from([("fishing".into(), stream)]);
    RegistrySnapshot::capture(
        Some(world.id()),
        vec![
            WorldSnapshot::capture(&world, &[state])
                .with_archive(&terrain(coins))
                .with_clocks(Seconds::new(1.375).unwrap(), Seconds::new(0.125).unwrap())
                .with_random_streams(&random),
        ],
    )
}
fn assert_chunk_stamp(recovery: &Recovery) {
    let registry = recovery.registry.as_ref().unwrap();
    let stamp = coins(recovery) - 0.25;
    for bytes in registry.worlds[0]
        .terrain
        .as_ref()
        .unwrap()
        .records()
        .values()
    {
        let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(value["stamp"], json!(stamp));
    }
}
fn coins(recovery: &Recovery) -> f64 {
    recovery.registry.as_ref().unwrap().worlds[0].towns[0]
        .ledger
        .free(&ResourceKey::parse("coins").unwrap())
        .unwrap()
        .get()
}
#[test]
fn whole_generation_roundtrip_includes_accounts_chunks_fractional_owners_and_rng() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    assert!(store.recover().unwrap().generation.is_none());
    assert_eq!(store.commit(snapshot(4.)).unwrap(), Generation(1));
    drop(store);
    let store = Store::open(&temp.root()).unwrap();
    let mut recovered = store.recover().unwrap();
    assert_eq!(recovered.generation, Some(Generation(1)));
    assert!(recovered.skipped.is_empty());
    assert_eq!(coins(&recovered), 4.25);
    let world = &mut recovered.registry.as_mut().unwrap().worlds[0];
    let (expected, state) = fixtures(4.);
    assert_eq!(
        encode_save(world.model.model()).unwrap(),
        encode_save(&expected).unwrap()
    );
    assert_eq!(world.towns[0].ledger, state.ledger);
    assert_eq!(world.towns[0].workers, state.workers);
    assert_eq!(world.clocks.map(|s| s.get()), [1.375, 0.125]);
    assert_eq!(
        world.terrain.as_ref().unwrap().records(),
        terrain(4.).records()
    );
    assert_eq!(
        world.terrain.as_ref().unwrap().private_world_bytes(),
        terrain(4.).private_world_bytes()
    );
    let mut expected = PythonRandom::from_integer("777").unwrap();
    for _ in 0..23 {
        expected.random();
    }
    for _ in 0..1000 {
        assert_eq!(
            world.random.get_mut("fishing").unwrap().random(),
            expected.random()
        );
    }
}
#[test]
fn failing_every_write_sync_and_publication_boundary_keeps_a_complete_generation() {
    let temp = Scratch::new();
    let mut probe = Store::create(&temp.root()).unwrap();
    let mut phases = vec![];
    probe
        .commit_inner(snapshot(2.), |p| {
            phases.push(p);
            Ok(())
        })
        .unwrap();
    drop(probe);
    for phase in phases {
        let temp = Scratch::new();
        let mut store = Store::create(&temp.root()).unwrap();
        store.commit(snapshot(1.)).unwrap();
        let result = store.commit_inner(snapshot(2.), |p| {
            if p == phase {
                Err(Error::Io(std::io::ErrorKind::StorageFull))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err(), "{phase:?}");
        drop(store);
        let mut store = Store::open(&temp.root()).unwrap();
        let recovered = store.recover().unwrap();
        let published = matches!(phase, Checkpoint::Published | Checkpoint::RootSynced);
        assert_eq!(
            coins(&recovered),
            if published { 2.25 } else { 1.25 },
            "{phase:?}"
        );
        assert!(recovered.skipped.is_empty());
        assert_chunk_stamp(&recovered);
        store.commit(snapshot(3.)).unwrap();
        assert_eq!(coins(&store.recover().unwrap()), 3.25);
    }
}
#[test]
fn process_exit_at_each_boundary_releases_lock_and_never_mixes_chunks() {
    let temp = Scratch::new();
    let mut probe = Store::create(&temp.root()).unwrap();
    let mut count = 0;
    probe
        .commit_inner(snapshot(2.), |_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    drop(probe);
    for phase in 0..count {
        let temp = Scratch::new();
        let mut store = Store::create(&temp.root()).unwrap();
        store.commit(snapshot(1.)).unwrap();
        drop(store);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence::tests::crash_writer_child"])
            .env("LANTERN_TEST_CRASH_ROOT", temp.root())
            .env("LANTERN_TEST_CRASH_PHASE", phase.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let store = Store::open(&temp.root()).unwrap();
        let recovered = store.recover().unwrap();
        assert!(matches!(coins(&recovered), 1.25 | 2.25));
        assert_chunk_stamp(&recovered);
        assert_eq!(
            recovered.registry.as_ref().unwrap().worlds[0]
                .terrain
                .as_ref()
                .unwrap()
                .records()
                .len(),
            2
        );
    }
}
#[test]
fn crash_writer_child() {
    let Some(root) = std::env::var_os("LANTERN_TEST_CRASH_ROOT") else {
        return;
    };
    let phase: usize = std::env::var("LANTERN_TEST_CRASH_PHASE")
        .unwrap()
        .parse()
        .unwrap();
    let mut store = Store::open(&PathBuf::from(root)).unwrap();
    let mut current = 0;
    store
        .commit_inner(snapshot(2.), |_| {
            if current == phase {
                std::process::exit(73)
            }
            current += 1;
            Ok(())
        })
        .unwrap();
    panic!("missing phase");
}
#[test]
fn corruption_missing_chunks_and_unexpected_files_recover_previous_or_fail_closed() {
    for damaged in [
        "00000000.bin",
        "00000001.bin",
        "manifest.json",
        "extra",
        "truncated",
        "manifest-version",
    ] {
        let temp = Scratch::new();
        let mut store = Store::create(&temp.root()).unwrap();
        store.commit(snapshot(1.)).unwrap();
        store.commit(snapshot(2.)).unwrap();
        let path = temp
            .root()
            .join("generation-00000000000000000002")
            .join(damaged);
        if damaged == "extra" {
            fs::write(path, b"unexpected").unwrap();
        } else if damaged == "truncated" {
            let path = temp
                .root()
                .join("generation-00000000000000000002/00000001.bin");
            let bytes = fs::read(&path).unwrap();
            fs::write(path, &bytes[..bytes.len() / 2]).unwrap();
        } else if damaged == "manifest-version" {
            let path = temp
                .root()
                .join("generation-00000000000000000002/manifest.json");
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            manifest["version"] = json!(99);
            fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
        let recovered = store.recover().unwrap();
        assert_eq!(coins(&recovered), 1.25);
        assert_eq!(recovered.skipped, vec![Generation(2)]);
        fs::remove_file(
            temp.root()
                .join("generation-00000000000000000001/00000000.bin"),
        )
        .unwrap();
        assert!(store.recover().is_err());
    }
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    store.commit(snapshot(2.)).unwrap();
    let file = temp
        .root()
        .join("generation-00000000000000000002/00000001.bin");
    let mut bytes = fs::read(&file).unwrap();
    bytes[100] ^= 1;
    fs::write(file, bytes).unwrap();
    assert_eq!(coins(&store.recover().unwrap()), 1.25);
}
#[test]
fn root_safety_os_lock_permissions_and_unpublished_complete_directories() {
    let temp = Scratch::new();
    fs::write(temp.0.join("savegame.json"), b"SOURCE_UNTOUCHED").unwrap();
    assert!(Store::create(&temp.0).is_err());
    assert!(Store::open(&temp.0).is_err());
    assert_eq!(
        fs::read(temp.0.join("savegame.json")).unwrap(),
        b"SOURCE_UNTOUCHED"
    );
    let mut store = Store::create(&temp.root()).unwrap();
    assert!(matches!(Store::open(&temp.root()), Err(Error::Busy)));
    store.commit(snapshot(1.)).unwrap();
    fs::rename(
        temp.root().join("generation-00000000000000000001"),
        temp.root().join("pending-00000000000000000001"),
    )
    .unwrap();
    assert!(store.recover().unwrap().generation.is_none());
    assert_eq!(store.commit(snapshot(2.)).unwrap(), Generation(2));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.root()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(
                temp.root()
                    .join("generation-00000000000000000002/00000000.bin")
            )
            .unwrap()
            .permissions()
            .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn malformed_aggregate_references_and_versions_are_rejected_before_disk() {
    for mutation in 0..7 {
        let mut bundle = snapshot(1.).encode().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bundle.registry).unwrap();
        match mutation {
            0 => value["version"] = json!(999),
            1 => value["active"] = json!("missing"),
            2 => value["worlds"][0]["towns"] = json!([]),
            3 => value["worlds"][0]["towns"][0]["ledger"]["version"] = json!(99),
            4 => value["worlds"][0]["random"][0][1] = json!("future"),
            5 => value["worlds"][0]["random"][0][3] = json!(625),
            6 => value["worlds"][0]["clocks"][0] = json!(-1),
            _ => unreachable!(),
        }
        bundle.registry = serde_json::to_vec(&value).unwrap();
        assert!(bundle.restore().is_err(), "{mutation}");
    }
    let mut bundle = snapshot(1.).encode().unwrap();
    bundle.chunks.pop();
    assert!(bundle.restore().is_err());
    let mut bundle = snapshot(1.).encode().unwrap();
    bundle.chunks.push(ChunkBlob {
        world: 0,
        key: ChunkKey { x: 9, y: 9 },
        bytes: vec![],
    });
    assert!(bundle.restore().is_err());
    let (world, state) = fixtures(1.);
    let missing =
        RegistrySnapshot::capture(Some(world.id()), vec![WorldSnapshot::capture(&world, &[])]);
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    assert!(store.commit(missing).is_err());
    assert!(store.recover().unwrap().generation.is_none());
    assert!(
        RegistrySnapshot::capture(
            None,
            vec![WorldSnapshot::capture(&world, &[state.clone(), state])]
        )
        .encode()
        .is_err()
    );
}
#[test]
fn background_writer_is_bounded_nonblocking_and_shutdown_drains_accepted_snapshots() {
    let temp = Scratch::new();
    let store = Store::create(&temp.root()).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut first = true;
    let service = SaveService::start_inner(store, move |store, snapshot| {
        store.commit_inner(snapshot, |phase| {
            if first && phase == Checkpoint::Written(0) {
                first = false;
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
            Ok(())
        })
    });
    let (mut world, state) = fixtures(1.);
    let ticket = service
        .submit(RegistrySnapshot::capture(
            Some(world.id()),
            vec![WorldSnapshot::capture(&world, &[state])],
        ))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ticket.try_result().unwrap(), None);
    let town = world.towns()[0].owner().town.clone();
    world.allocate::<lantern_core::ids::Actor>(&town).unwrap();
    let ticket2 = service.submit(snapshot(2.)).unwrap();
    assert!(matches!(service.submit(snapshot(3.)), Err(Error::Busy)));
    release_tx.send(()).unwrap();
    assert_eq!(ticket.wait().unwrap(), Generation(1));
    service.flush().unwrap();
    assert_eq!(ticket2.wait().unwrap(), Generation(2));
    let _ = service.submit(snapshot(4.)).unwrap();
    service.shutdown().unwrap();
    assert_eq!(
        coins(&Store::open(&temp.root()).unwrap().recover().unwrap()),
        4.25
    );
}
#[test]
fn flush_and_shutdown_report_failed_and_dropped_tickets() {
    let temp = Scratch::new();
    let store = Store::create(&temp.root()).unwrap();
    let service = SaveService::start_inner(store, |store, snapshot| {
        store.commit_inner(snapshot, |phase| {
            if phase == Checkpoint::StageSynced {
                Err(Error::Io(std::io::ErrorKind::StorageFull))
            } else {
                Ok(())
            }
        })
    });
    let ticket = service.submit(snapshot(1.)).unwrap();
    assert!(ticket.wait().is_err());
    assert!(service.flush().is_err());
    let _ = service.submit(snapshot(2.)).unwrap();
    assert!(service.shutdown().is_err());
    let store = Store::open(&temp.root()).unwrap();
    assert!(store.recover().unwrap().generation.is_none());
}
#[test]
fn registry_preserves_multiple_worlds_active_selection_and_empty_registry() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    let (world, state) = fixtures(1.);
    let other = ModelWorld::new(
        WorldId::new("inactive").unwrap(),
        TerrainSeed(5),
        0,
        vec![],
        PrivateCredentials::default(),
    )
    .unwrap();
    store
        .commit(RegistrySnapshot::capture(
            Some(world.id()),
            vec![
                WorldSnapshot::capture(&world, &[state]),
                WorldSnapshot::capture(&other, &[]),
            ],
        ))
        .unwrap();
    let recovered = store.recover().unwrap().registry.unwrap();
    assert_eq!(recovered.worlds.len(), 2);
    assert_eq!(recovered.active.unwrap(), *world.id());
    store
        .commit(RegistrySnapshot::capture(None, vec![]))
        .unwrap();
    let recovered = store.recover().unwrap().registry.unwrap();
    assert!(recovered.active.is_none());
    assert!(recovered.worlds.is_empty());
}

#[test]
fn aggregate_admits_population_social_and_livestock_without_duplicate_worker_authority() {
    let (world, mut state) = fixtures(1.);
    let town = &world.towns()[0];
    let (actor, worker) = state
        .workers
        .iter()
        .next()
        .map(|(a, w)| (a.clone(), w.clone()))
        .unwrap();
    state.workers.clear();
    state.population = Some(
        crate::population::Population::restore(
            town,
            false,
            vec![crate::population::Resident {
                actor: actor.clone(),
                name: "Synthetic resident".into(),
                worker,
                assignment: None,
            }],
            crate::population::History::default(),
        )
        .unwrap(),
    );
    state.social = Some(
        crate::social::Social::restore(
            town.owner().clone(),
            crate::social::Player::default(),
            BTreeMap::new(),
        )
        .unwrap(),
    );
    state.livestock =
        Some(crate::livestock::Livestock::admit(town, &state.ledger, vec![], 1).unwrap());
    let bundle = RegistrySnapshot::capture(
        Some(world.id()),
        vec![WorldSnapshot::capture(&world, &[state.clone()])],
    )
    .encode()
    .unwrap();
    let restored = bundle.restore().unwrap();
    let saved = &restored.worlds[0].towns[0];
    assert_eq!(saved.population, state.population);
    assert_eq!(saved.social, state.social);
    assert_eq!(saved.livestock, state.livestock);
    state
        .workers
        .insert(actor, crate::progression::Worker::new("hauling").unwrap());
    assert!(
        RegistrySnapshot::capture(None, vec![WorldSnapshot::capture(&world, &[state])])
            .encode()
            .is_err()
    );
}

#[test]
fn aggregate_reader_preserves_supported_older_model_versions_and_rejects_invalid_ledger_links() {
    for version in [1, 2, 3] {
        let mut bundle = snapshot(1.).encode().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bundle.registry).unwrap();
        value["worlds"][0]["model"]["version"] = json!(version);
        if version == 1 {
            for actor in value["worlds"][0]["model"]["towns"][0]["actors"]
                .as_array_mut()
                .unwrap()
            {
                actor.as_object_mut().unwrap().remove("facing");
                actor.as_object_mut().unwrap().remove("travel");
            }
        }
        bundle.registry = serde_json::to_vec(&value).unwrap();
        let restored = bundle.restore().unwrap();
        assert_eq!(restored.worlds[0].towns[0].ledger, fixtures(1.).1.ledger);
    }
    for field in ["source", "pending", "quantity"] {
        let mut bundle = snapshot(1.).encode().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bundle.registry).unwrap();
        let ledger = &mut value["worlds"][0]["towns"][0]["ledger"];
        match field {
            "source" => ledger["held"][0][1] = json!(999),
            "pending" => ledger["stores"][1]["pending"][0][0] = json!(999),
            "quantity" => ledger["coins"] = json!(-1),
            _ => unreachable!(),
        }
        bundle.registry = serde_json::to_vec(&value).unwrap();
        assert!(bundle.restore().is_err());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_generation_files_and_roots_are_never_followed() {
    use std::os::unix::fs::symlink;
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    store.commit(snapshot(2.)).unwrap();
    symlink(temp.root(), temp.0.join("alias")).unwrap();
    assert!(Store::open(&temp.0.join("alias")).is_err());
    let file = temp
        .root()
        .join("generation-00000000000000000002/00000001.bin");
    fs::remove_file(&file).unwrap();
    symlink(
        temp.root()
            .join("generation-00000000000000000001/00000001.bin"),
        file,
    )
    .unwrap();
    let recovered = store.recover().unwrap();
    assert_eq!(coins(&recovered), 1.25);
    assert_eq!(recovered.skipped, vec![Generation(2)]);
}

#[test]
fn private_provision_archives_are_exact_and_malformed_objects_never_publish() {
    let bytes = br#"{ "synthetic-world": {"synthetic-account":{"salt":"SYNTHETIC_SALT","pin_hash":"SYNTHETIC_HASH"}}, "private_extension":9007199254740993 }"#;
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store
        .commit(snapshot(1.).with_provision_archive(bytes))
        .unwrap();
    assert_eq!(
        store
            .recover()
            .unwrap()
            .registry
            .unwrap()
            .private_provision_bytes()
            .unwrap(),
        bytes
    );
    for invalid in [
        b"[]".as_slice(),
        br#"{"x":1,"x":2}"#.as_slice(),
        b"{bad".as_slice(),
    ] {
        assert!(
            store
                .commit(snapshot(2.).with_provision_archive(invalid))
                .is_err()
        );
        assert_eq!(store.recover().unwrap().generation, Some(Generation(1)));
    }
}

#[test]
fn terrain_archives_validate_legacy_generators_and_current_resource_fields_without_catchup() {
    for version in 1..=7 {
        let payload = json!({"towns":{},"shared_map":{"chunked":true,"chunk_size":40,"map_size":480,"terrain_seed":7,"starter_offset":0,"terrain_version":version,"chunk_clock":1.25}});
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut record = json!({"terrain_version":version,"terrain":vec!["g".repeat(40);40],"map_resources":[{"kind":"tree","x":1,"y":1,"supply":0.125,"supply_max":10.,"regeneration":1.}],"tree_stumps":[],"clock":0.125});
        let archive = TerrainArchive::decode(
            &bytes,
            vec![(
                ChunkKey { x: 0, y: 0 },
                serde_json::to_vec(&record).unwrap(),
            )],
        )
        .unwrap();
        let restored: serde_json::Value =
            serde_json::from_slice(&archive.records()[&ChunkKey { x: 0, y: 0 }]).unwrap();
        assert_eq!(restored["map_resources"][0]["supply"], json!(0.125));
        record["map_resources"][0]["supply"] = json!(-1);
        assert!(
            TerrainArchive::decode(
                &bytes,
                vec![(
                    ChunkKey { x: 0, y: 0 },
                    serde_json::to_vec(&record).unwrap()
                )]
            )
            .is_err()
        );
    }
}

fn file_tree(path: &std::path::Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = vec![];
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        if meta.is_dir() {
            out.extend(file_tree(&entry.path()));
        } else {
            out.push((
                entry.path(),
                fs::read(entry.path()).unwrap(),
                meta.modified().unwrap(),
            ));
        }
    }
    out
}
#[test]
fn native_copy_and_selected_recovery_validate_without_mutating_source() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    store.commit(snapshot(2.)).unwrap();
    drop(store);
    let before = file_tree(&temp.root());
    let target = temp.0.join("converted");
    let report = tools::convert_native(&temp.root(), &target).unwrap();
    assert_eq!(report.source_generation, Generation(2));
    assert!(report.skipped.is_empty());
    assert_eq!(file_tree(&temp.root()), before);
    let copied = Store::open_read_only(&target).unwrap();
    assert_eq!(coins(&copied.recover().unwrap()), 2.25);
    drop(copied);
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(target.join("generation-00000000000000000001/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["version"], json!(2));
    assert_eq!(
        manifest["source"]["manifest_sha256"],
        json!(report.source_manifest_sha256)
    );
    let selected = temp.0.join("rollback");
    let report = tools::recover_native(&temp.root(), &selected, Some(Generation(1))).unwrap();
    assert_eq!(report.source_generation, Generation(1));
    assert_eq!(
        coins(&Store::open_read_only(&selected).unwrap().recover().unwrap()),
        1.25
    );
    assert_eq!(file_tree(&temp.root()), before);
}
#[test]
fn native_conversion_refuses_corrupt_latest_but_recovery_copies_whole_previous_generation() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    store.commit(snapshot(2.)).unwrap();
    drop(store);
    fs::remove_file(
        temp.root()
            .join("generation-00000000000000000002/00000001.bin"),
    )
    .unwrap();
    let before = file_tree(&temp.root());
    assert!(tools::convert_native(&temp.root(), &temp.0.join("conversion")).is_err());
    assert!(!temp.0.join("conversion").exists());
    let report = tools::recover_native(&temp.root(), &temp.0.join("recovered"), None).unwrap();
    assert_eq!(report.skipped, vec![Generation(2)]);
    let copied = Store::open_read_only(&temp.0.join("recovered")).unwrap();
    let restored = copied.recover().unwrap();
    assert_eq!(coins(&restored), 1.25);
    assert_chunk_stamp(&restored);
    assert_eq!(file_tree(&temp.root()), before);
    assert!(
        tools::recover_native(
            &temp.root(),
            &temp.0.join("invalid-selection"),
            Some(Generation(2))
        )
        .is_err()
    );
    assert!(!temp.0.join("invalid-selection").exists());
}
#[test]
fn native_copy_refuses_aliases_overlap_existing_destinations_and_live_writers() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    assert!(tools::convert_native(&temp.root(), &temp.0.join("busy-copy")).is_err());
    assert!(!temp.0.join("busy-copy").exists());
    drop(store);
    for target in [temp.root(), temp.root().join("nested"), temp.0.clone()] {
        assert!(tools::convert_native(&temp.root(), &target).is_err());
    }
    assert!(!temp.root().join("nested").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(temp.root(), temp.0.join("alias")).unwrap();
        assert!(tools::convert_native(&temp.root(), &temp.0.join("alias/nested")).is_err());
        symlink(temp.0.join("missing"), temp.0.join("dangling")).unwrap();
        assert!(tools::convert_native(&temp.root(), &temp.0.join("dangling")).is_err());
    }
}
#[test]
fn incomplete_native_destination_is_never_opened_as_an_empty_or_partially_converted_save() {
    let temp = Scratch::new();
    let mut store = Store::create_copy(&temp.root()).unwrap();
    assert!(Store::open_read_only(&temp.root()).is_err());
    store.commit(snapshot(1.)).unwrap();
    assert!(Store::open_read_only(&temp.root()).is_err());
    store.finish_copy().unwrap();
    drop(store);
    assert_eq!(
        coins(
            &Store::open_read_only(&temp.root())
                .unwrap()
                .recover()
                .unwrap()
        ),
        1.25
    );
}
#[test]
fn cleanup_removes_only_pending_work_and_read_only_handles_cannot_commit_or_create_locks() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    store
        .commit_inner(snapshot(2.), |phase| {
            if phase == Checkpoint::Written(1) {
                Err(Error::Io(std::io::ErrorKind::StorageFull))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    drop(store);
    let committed = fs::read(
        temp.root()
            .join("generation-00000000000000000001/manifest.json"),
    )
    .unwrap();
    assert_eq!(tools::cleanup_pending(&temp.root()).unwrap(), 1);
    assert_eq!(
        fs::read(
            temp.root()
                .join("generation-00000000000000000001/manifest.json")
        )
        .unwrap(),
        committed
    );
    let before = file_tree(&temp.root());
    let mut readonly = Store::open_read_only(&temp.root()).unwrap();
    assert!(readonly.commit(snapshot(3.)).is_err());
    drop(readonly);
    assert_eq!(file_tree(&temp.root()), before);
    fs::remove_file(temp.root().join(".writer.lock")).unwrap();
    assert!(Store::open_read_only(&temp.root()).is_err());
    assert!(!temp.root().join(".writer.lock").exists());
}

#[test]
fn copy_interruption_at_every_boundary_never_changes_source_or_exposes_partial_destination() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    drop(store);
    let before = file_tree(&temp.root());
    let mut checkpoints = vec![];
    tools::copy_with_checkpoints(&temp.root(), &temp.0.join("probe"), None, false, |phase| {
        checkpoints.push(phase);
        Ok(())
    })
    .unwrap();
    for (index, phase) in checkpoints.into_iter().enumerate() {
        let target = temp.0.join(format!("failed-copy-{index}"));
        let result = tools::copy_with_checkpoints(&temp.root(), &target, None, false, |p| {
            if p == phase {
                Err(Error::Io(std::io::ErrorKind::StorageFull))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        if phase == tools::CopyCheckpoint::Ready {
            assert_eq!(
                coins(&Store::open_read_only(&target).unwrap().recover().unwrap()),
                1.25
            );
        } else {
            assert!(Store::open_read_only(&target).is_err());
        }
        assert_eq!(file_tree(&temp.root()), before);
    }
}
#[test]
fn abrupt_copy_exit_keeps_source_exact_and_requires_completed_destination_publication() {
    let temp = Scratch::new();
    let mut store = Store::create(&temp.root()).unwrap();
    store.commit(snapshot(1.)).unwrap();
    drop(store);
    let mut count = 0;
    tools::copy_with_checkpoints(&temp.root(), &temp.0.join("probe"), None, false, |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    let before = file_tree(&temp.root());
    for phase in 0..count {
        let target = temp.0.join(format!("crashed-copy-{phase}"));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence::tests::copy_crash_child"])
            .env("LANTERN_TEST_COPY_SOURCE", temp.root())
            .env("LANTERN_TEST_COPY_TARGET", &target)
            .env("LANTERN_TEST_COPY_PHASE", phase.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if phase + 1 == count {
            assert_eq!(
                coins(&Store::open_read_only(&target).unwrap().recover().unwrap()),
                1.25
            );
        } else {
            assert!(Store::open_read_only(&target).is_err());
        }
        assert_eq!(file_tree(&temp.root()), before);
    }
}
#[test]
fn copy_crash_child() {
    let Some(source) = std::env::var_os("LANTERN_TEST_COPY_SOURCE") else {
        return;
    };
    let target = std::env::var_os("LANTERN_TEST_COPY_TARGET").unwrap();
    let phase: usize = std::env::var("LANTERN_TEST_COPY_PHASE")
        .unwrap()
        .parse()
        .unwrap();
    let mut current = 0;
    tools::copy_with_checkpoints(
        &PathBuf::from(source),
        &PathBuf::from(target),
        None,
        false,
        |_| {
            if current == phase {
                std::process::exit(73)
            }
            current += 1;
            Ok(())
        },
    )
    .unwrap();
    panic!("missing copy checkpoint");
}

#[test]
fn native_tools_copy_supported_legacy_model_versions_without_rewriting_private_facts() {
    for version in [1, 2, 3] {
        let temp = Scratch::new();
        let mut bundle = snapshot(1.).encode().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bundle.registry).unwrap();
        value["worlds"][0]["model"]["version"] = json!(version);
        if version == 1 {
            for actor in value["worlds"][0]["model"]["towns"][0]["actors"]
                .as_array_mut()
                .unwrap()
            {
                actor.as_object_mut().unwrap().remove("facing");
                actor.as_object_mut().unwrap().remove("travel");
            }
        }
        bundle.registry = serde_json::to_vec(&value).unwrap();
        let expected = bundle.registry.clone();
        let mut store = Store::create(&temp.root()).unwrap();
        store.commit_bundle(bundle, |_| Ok(())).unwrap();
        drop(store);
        let target = temp.0.join("copy");
        tools::convert_native(&temp.root(), &target).unwrap();
        assert_eq!(
            fs::read(target.join("generation-00000000000000000001/00000000.bin")).unwrap(),
            expected
        );
    }
}

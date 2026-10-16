use lantern_core::units::Position;
use lantern_host::legacy::{ChunkKey, PrivateFounderWorldCopy};
use serde_json::{Value, json};
fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/founder/world_python.json")).unwrap()
}
#[test]
fn actual_python_shared_independent_and_chunk_recovery_vectors() {
    for case in vectors()["cases"].as_array().unwrap() {
        let source = serde_json::to_vec(&case["source"]).unwrap();
        let records: Vec<_> = case["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| {
                let (x, y) = record[0].as_str().unwrap().split_once(',').unwrap();
                (
                    ChunkKey {
                        x: x.parse().unwrap(),
                        y: y.parse().unwrap(),
                    },
                    serde_json::to_vec(&record[1]).unwrap(),
                )
            })
            .collect();
        let refs: Vec<_> = records.iter().map(|(k, r)| (*k, r.as_slice())).collect();
        let copy = PrivateFounderWorldCopy::with_active_chunks(&source, "a", &refs).unwrap();
        let state = copy.state().unwrap();
        let inputs: Vec<_> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32))
            .collect();
        let result = copy.walk_batch(&inputs).unwrap();
        for (actual, key) in [
            (result.state.position.x.get(), "x"),
            (result.state.position.y.get(), "y"),
        ] {
            assert!(
                (actual - case["expected"][key].as_f64().unwrap()).abs() < 1e-12,
                "{} {key}: {actual} != {}",
                case["name"],
                case["expected"][key]
            );
        }
        assert_eq!(
            i64::from(result.state.facing),
            case["expected"]["facing"].as_i64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(result.clear_activity, inputs.iter().any(|v| *v != (0, 0)));
        for (p, open) in case["probes"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["open"].as_array().unwrap())
        {
            assert_eq!(
                copy.walkable(
                    Position::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()).unwrap()
                )
                .unwrap(),
                open.as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(copy.state().unwrap(), state);
        assert_eq!(copy.private_bytes(), source);
    }
}
#[test]
fn world_batch_rejects_before_recovery_and_private_errors_are_redacted() {
    let case = &vectors()["cases"][0];
    let source = serde_json::to_vec(&case["source"]).unwrap();
    let copy = PrivateFounderWorldCopy::decode(&source, "a").unwrap();
    let initial = copy.state().unwrap();
    for batch in [vec![], vec![(0, 0); 6], vec![(0, 0), (2, 0)]] {
        assert!(copy.walk_batch(&batch).is_err());
    }
    assert_eq!(copy.state().unwrap(), initial);
    assert_eq!(copy.private_bytes(), source);
    let mut bad = case["source"].clone();
    bad["shared_map"]["chunked"] = json!({"private-secret":"do not leak"});
    let error = PrivateFounderWorldCopy::decode(&serde_json::to_vec(&bad).unwrap(), "a")
        .err()
        .unwrap();
    assert!(!format!("{error:?}").contains("private-secret"));
    assert!(!format!("{error:?}").contains("do not leak"));
    assert!(PrivateFounderWorldCopy::decode(&source, "private-town-name").is_err());
}

#[test]
fn actual_command_lifecycle_batch_activates_on_disposable_owner_only() {
    use lantern_core::ids::WorldId;
    use lantern_host::legacy::{
        ChunkDirectory, ChunkLifecycle, ResidentChunkCopy, import_with_chunk_copies,
    };
    use std::fs;
    let vectors: Value =
        serde_json::from_str(include_str!("fixtures/founder/lifecycle_python.json")).unwrap();
    for (i, case) in vectors["cases"].as_array().unwrap().iter().enumerate() {
        let root = std::env::temp_dir().join(format!("lantern-founder-{}-{i}", std::process::id()));
        let directory = root.join("00042");
        fs::create_dir_all(&directory).unwrap();
        for (k, r) in case["saved"].as_object().unwrap() {
            fs::write(directory.join(format!("{k}.json")), r.to_string()).unwrap();
        }
        let source = json!({"world":case["world"],"tokens":{}})
            .to_string()
            .into_bytes();
        let world = WorldId::new("legacy").unwrap();
        let registry = import_with_chunk_copies(
            Some(&source),
            None,
            &[ChunkDirectory::new(&world, &directory)],
        )
        .unwrap();
        let key = |text: &str| {
            let (x, y) = text.split_once(',').unwrap();
            ChunkKey {
                x: x.parse().unwrap(),
                y: y.parse().unwrap(),
            }
        };
        let active: Vec<_> = case["active"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, r)| (key(k), r.to_string().into_bytes()))
            .collect();
        let camera: Vec<_> = case["camera"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, r)| (key(k), r.to_string().into_bytes()))
            .collect();
        let active_refs: Vec<_> = active
            .iter()
            .map(|(k, r)| ResidentChunkCopy::new(&world, *k, r))
            .collect();
        let camera_refs: Vec<_> = camera
            .iter()
            .map(|(k, r)| ResidentChunkCopy::new(&world, *k, r))
            .collect();
        let mut owner =
            ChunkLifecycle::prepare(&registry, &world, &active_refs, &camera_refs).unwrap();
        let before = owner.private_world_bytes();
        let keys = owner.active_keys();
        let cameras = owner.camera_keys_lru();
        let records: Vec<_> = keys
            .iter()
            .map(|k| owner.private_active_bytes(*k).unwrap().to_vec())
            .collect();
        let inputs: Vec<_> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32))
            .collect();
        let result = owner
            .preview_founder_batch("synthetic-town", &inputs)
            .unwrap();
        assert!(
            (result.state.position.x.get() - case["expected"]["x"].as_f64().unwrap()).abs() < 1e-12,
            "case {i}: {:?} expected {}",
            result,
            case["expected"]
        );
        assert!(
            (result.state.position.y.get() - case["expected"]["y"].as_f64().unwrap()).abs() < 1e-12
        );
        assert_eq!(
            i64::from(result.state.facing),
            case["expected"]["facing"].as_i64().unwrap()
        );
        assert_eq!(owner.private_world_bytes(), before);
        assert_eq!(owner.active_keys(), keys);
        assert_eq!(owner.camera_keys_lru(), cameras);
        assert_eq!(
            keys.iter()
                .map(|k| owner.private_active_bytes(*k).unwrap().to_vec())
                .collect::<Vec<_>>(),
            records
        );
        assert_eq!(owner.snapshot_cache_len(), 0);
        assert!(owner.take_write_intents().is_empty());
        assert!(
            owner
                .preview_founder_batch("synthetic-town", &[(1, 0), (5, 0)])
                .is_err()
        );
        assert_eq!(owner.private_world_bytes(), before);
        // Active records supply resources while the camera-only rock is excluded.
        let scene = owner.founder_scene("synthetic-town").unwrap();
        assert!(!scene.walkable(Position::new(42., 20.).unwrap()).unwrap());
        assert!(
            !scene.walkable(Position::new(120., 120.).unwrap()).unwrap(),
            "camera terrain is unavailable"
        );
        assert_eq!(
            source,
            json!({"world":case["world"],"tokens":{}})
                .to_string()
                .into_bytes()
        );
        if i == 0 {
            // Active records bypass sleeping arithmetic. Crossing x=40 makes
            // the SECOND command load 2,0, whose float clock cannot subtract a
            // Python integer beyond binary64 range. This proves per-command
            // activation and that a late load error cannot publish first-step
            // movement, camera promotion, pending writes, or cache mutations.
            let huge = format!("1{}", "0".repeat(400));
            owner.set_clock(huge.as_bytes()).unwrap();
            let before_error = owner.private_world_bytes();
            assert!(
                owner
                    .preview_founder_batch("synthetic-town", &[(1, 0)])
                    .is_ok()
            );
            assert!(
                owner
                    .preview_founder_batch("synthetic-town", &[(1, 0), (1, 0)])
                    .is_err()
            );
            assert_eq!(owner.private_world_bytes(), before_error);
            assert_eq!(owner.active_keys(), keys);
            assert_eq!(owner.camera_keys_lru(), cameras);
            assert!(owner.take_write_intents().is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn actual_python_search_terrain_and_bounds_queries() {
    use lantern_core::{spatial::search_terrain_open, units::Tile};
    for case in vectors()["search"].as_array().unwrap() {
        let terrain = match case["cell"].as_str() {
            Some("absent") => None,
            Some(s) => Some(s.chars().next()),
            None => Some(None),
        };
        let bounds = case["bounds"].as_array().map(|b| {
            (
                b[0].as_i64().unwrap() as i32,
                b[1].as_i64().unwrap() as i32,
                b[2].as_i64().unwrap() as i32,
                b[3].as_i64().unwrap() as i32,
            )
        });
        assert_eq!(
            search_terrain_open(Tile { x: 1, y: 1 }, 4, bounds, terrain),
            case["open"].as_bool().unwrap()
        );
    }
}

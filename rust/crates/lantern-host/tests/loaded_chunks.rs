use lantern_core::{ids::WorldId, units::Tile};
use lantern_host::{BoundaryError, legacy::*};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Deserialize)]
struct Golden {
    seed: Box<RawValue>,
    version: u32,
    key: String,
    size: u32,
    sha256: String,
    record: Value,
}
impl Golden {
    fn key(&self) -> ChunkKey {
        let (x, y) = self.key.split_once(',').unwrap();
        ChunkKey {
            x: x.parse().unwrap(),
            y: y.parse().unwrap(),
        }
    }
}
fn goldens() -> Vec<Golden> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/chunks.json")).unwrap()
}
struct Copy(PathBuf);
impl Copy {
    fn new(golden: &Golden) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lantern-loaded-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let namespace = serde_json::from_str::<String>(golden.seed.get())
            .unwrap_or_else(|_| golden.seed.get().into());
        let dir = root.join(namespace);
        fs::create_dir_all(&dir).unwrap();
        let source = format!(
            r#"{{"world":{{"towns":{{}},"shared_map":{{"chunked":1,"chunk_size":40,"map_size":{},"terrain_seed":{},"terrain_version":{},"chunk_clock":99999}}}},"tokens":{{}}}}"#,
            golden.size,
            golden.seed.get(),
            golden.version
        );
        fs::write(root.join("source.json"), source).unwrap();
        Self(dir)
    }
    fn write(&self, key: ChunkKey, record: &Value) {
        fs::write(
            self.0.join(key.file_name()),
            serde_json::to_vec(record).unwrap(),
        )
        .unwrap();
    }
    fn import(&self) -> ImportedRegistry {
        read_copies_with_chunks(
            &self.0.parent().unwrap().join("source.json"),
            None,
            &[ChunkDirectory::new(
                &WorldId::new("legacy").unwrap(),
                &self.0,
            )],
        )
        .unwrap()
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}
fn rejected(result: Result<&LoadedChunk<'_>, BoundaryError>) -> BoundaryError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("unsupported record loaded"),
    }
}

#[test]
fn real_python_golden_records_load_without_generation_or_semantic_changes() {
    let cases = goldens();
    assert_eq!(cases.len(), 58);
    assert_eq!(
        cases[0].sha256,
        "3b82ec1f2b33a643495c6ddd76af88b19f1e5f343232999fc4846ccfad5b1a4a"
    );
    assert_eq!(
        cases[1].sha256,
        "6480ee4ae8b77f5ad1fc84b8c1c8b97f3bc789f26bf707cf476df51a2eedb2d1"
    );
    for case in &cases {
        let copy = Copy::new(case);
        let key = case.key();
        copy.write(key, &case.record);
        let archive = copy.import();
        let world = &archive.worlds()[0];
        let mut reader = ChunkReader::new(world).unwrap();
        let record = reader.read(key).unwrap();
        assert_eq!(record.key(), key);
        assert_eq!(record.private_bytes(), world.chunks()[0].private_bytes());
        assert_eq!(record.clock(), Some(0.0)); // never advance to shared chunk_clock
        assert!(record.stumps().is_empty());
        let rows: Vec<_> = case.record["terrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(record.terrain(), rows);
        let expected = case.record["map_resources"].as_array().unwrap();
        assert_eq!(record.resources().len(), expected.len());
        for (loaded, raw) in record.resources().iter().zip(expected) {
            assert_eq!(
                loaded.tile(),
                Tile {
                    x: raw["x"].as_i64().unwrap() as i32,
                    y: raw["y"].as_i64().unwrap() as i32
                }
            );
            assert_eq!(loaded.kind(), raw["kind"].as_str().unwrap());
            assert_eq!(
                loaded.species(),
                raw.get("species").map(|v| v.as_str().unwrap())
            );
            assert_eq!(loaded.supply(), None);
            assert_eq!(loaded.supply_max(), None);
            assert_eq!(loaded.regeneration(), None);
        }
        for (y, row) in rows.iter().enumerate() {
            for (x, code) in row.chars().enumerate() {
                assert_eq!(
                    reader.tile(Tile {
                        x: key.x as i32 * 40 + x as i32,
                        y: key.y as i32 * 40 + y as i32
                    }),
                    LoadedTile::Saved(code)
                );
            }
        }
        reader.unload(key);
        assert_eq!(
            reader.tile(Tile {
                x: key.x as i32 * 40,
                y: key.y as i32 * 40
            }),
            LoadedTile::Unloaded
        );
        assert_eq!(reader.read(key).unwrap().terrain(), rows);
    }
}

#[test]
fn boundaries_seams_missing_chunks_and_lru_eviction_are_explicit() {
    let cases = goldens();
    let selected: Vec<_> = cases.iter().filter(|g| g.version == 7).collect();
    assert_eq!(selected.len(), 50);
    let copy = Copy::new(selected[0]);
    for case in &selected {
        copy.write(case.key(), &case.record);
    }
    let archive = copy.import();
    let mut reader = ChunkReader::new(&archive.worlds()[0]).unwrap();
    for tile in [
        Tile { x: -1, y: 0 },
        Tile { x: 0, y: -1 },
        Tile { x: 480, y: 0 },
        Tile { x: 0, y: 480 },
    ] {
        assert_eq!(reader.tile(tile), LoadedTile::OutsideWorld);
    }
    assert_eq!(reader.tile(Tile { x: 479, y: 479 }), LoadedTile::Unloaded);
    assert!(
        rejected(reader.read(ChunkKey { x: 12, y: 0 }))
            .path
            .ends_with(".key")
    );
    let missing = ChunkKey { x: 11, y: 11 };
    let error = rejected(reader.read(missing));
    assert!(format!("{error:?}").contains("generation and starter reconstruction unported"));
    assert_eq!(reader.loaded_keys().count(), 0);
    // Both sides of a real generated seam retain their own saved tile/water data.
    for key in [ChunkKey { x: 0, y: 0 }, ChunkKey { x: 1, y: 0 }] {
        reader.read(key).unwrap();
    }
    for x in [39, 40] {
        let case = selected
            .iter()
            .find(|g| g.key() == ChunkKey { x: x / 40, y: 0 })
            .unwrap();
        for y in 0..40 {
            let code =
                case.record["terrain"][y].as_str().unwrap().as_bytes()[x as usize % 40] as char;
            assert_eq!(
                reader.tile(Tile {
                    x: x as i32,
                    y: y as i32
                }),
                LoadedTile::Saved(code)
            );
        }
    }
    for (key, tile) in [
        (ChunkKey { x: 3, y: 0 }, Tile { x: 159, y: 23 }),
        (ChunkKey { x: 4, y: 0 }, Tile { x: 160, y: 23 }),
    ] {
        assert!(
            reader
                .read(key)
                .unwrap()
                .resources()
                .iter()
                .any(|r| r.kind() == "water" && r.tile() == tile)
        );
    }
    // Reset the views, then demonstrate true LRU (a cache hit refreshes recency).
    for key in reader.loaded_keys().collect::<Vec<_>>() {
        reader.unload(key);
    }
    for case in &selected[..49] {
        reader.read(case.key()).unwrap();
    }
    reader.read(selected[0].key()).unwrap();
    reader.read(selected[49].key()).unwrap();
    let keys = reader.loaded_keys().collect::<Vec<_>>();
    assert_eq!(keys.len(), MAX_LOADED_CHUNKS);
    assert!(keys.contains(&selected[0].key()));
    assert!(!keys.contains(&selected[1].key()));
    assert_eq!(
        reader.read(selected[1].key()).unwrap().terrain()[0],
        selected[1].record["terrain"][0].as_str().unwrap()
    );
    let keys = reader.loaded_keys().collect::<Vec<_>>();
    assert!(!keys.contains(&selected[2].key()));
    let before = keys;
    assert!(reader.read(missing).is_err());
    assert_eq!(reader.loaded_keys().collect::<Vec<_>>(), before);
    assert_eq!(archive.worlds()[0].chunks().len(), 50);
}

#[test]
fn saved_edits_stumps_optional_clocks_and_private_extensions_survive_reload() {
    let cases = goldens();
    let case = &cases[6];
    let copy = Copy::new(case);
    let key = case.key();
    let mut edited = case.record.clone();
    let x = key.x * 40;
    let y = key.y * 40;
    edited["terrain"][0] = json!("v".repeat(40));
    edited["map_resources"] = json!([
        {"kind":"tree", "species":"oak", "x":x, "y":y, "supply":0.125, "supply_max":8.0, "regeneration":0.0625},
        {"kind":"SYNTHETIC_PRIVATE_KIND", "x":x+1, "y":y}
    ]);
    edited["tree_stumps"] = json!([{"x":x+2,"y":y,"remaining":0.03125}]);
    edited.as_object_mut().unwrap().remove("clock");
    copy.write(key, &edited);
    let path = copy.0.join(key.file_name());
    let bytes = String::from_utf8(fs::read(&path).unwrap())
        .unwrap()
        .replacen(
            "{",
            "{\"SYNTHETIC_PRIVATE_EXTENSION\":123456789012345678901234567890,",
            1,
        )
        .replace("0.125", "1.25000e-1");
    fs::write(&path, &bytes).unwrap();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let archive = copy.import();
    let mut reader = ChunkReader::new(&archive.worlds()[0]).unwrap();
    for _ in 0..2 {
        let loaded = reader.read(key).unwrap();
        assert_eq!(loaded.resources().len(), 2); // never regenerated/repaired
        assert_eq!(loaded.resources()[0].supply(), Some(0.125));
        assert_eq!(loaded.resources()[0].supply_max(), Some(8.0));
        assert_eq!(loaded.resources()[0].regeneration(), Some(0.0625));
        assert_eq!(loaded.resources()[1].kind(), "SYNTHETIC_PRIVATE_KIND");
        assert_eq!(loaded.resources()[1].supply(), None);
        assert_eq!(
            loaded.stumps()[0].tile(),
            Tile {
                x: x as i32 + 2,
                y: y as i32
            }
        );
        assert_eq!(loaded.stumps()[0].remaining(), 0.03125);
        assert_eq!(loaded.clock(), None);
        assert_eq!(loaded.private_bytes(), bytes.as_bytes());
        assert_eq!(
            reader.tile(Tile {
                x: x as i32,
                y: y as i32
            }),
            LoadedTile::Saved('v')
        );
        reader.unload(key);
    }
    assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    let report = serde_json::to_string(&archive.report()).unwrap();
    assert!(!report.contains("SYNTHETIC_PRIVATE"));
    assert!(!report.contains("1.25000e-1"));
}

#[test]
fn unsupported_decoding_fails_without_corrupting_existing_views_or_archive() {
    let cases = goldens();
    let case = &cases[6];
    for (field, value) in [
        ("supply", json!(-1)),
        ("supply_max", json!("SYNTHETIC_SECRET")),
        ("regeneration", json!(null)),
        ("species", json!(42)),
    ] {
        let copy = Copy::new(case);
        let mut edited = case.record.clone();
        edited["map_resources"][0][field] = value;
        copy.write(case.key(), &edited);
        copy.write(cases[7].key(), &cases[7].record);
        let archive = copy.import(); // R06 preserves, R07 explicitly rejects typed fields.
        let before = archive.worlds()[0]
            .chunks()
            .iter()
            .find(|r| r.key() == case.key())
            .unwrap()
            .private_bytes()
            .to_vec();
        let mut reader = ChunkReader::new(&archive.worlds()[0]).unwrap();
        reader.read(cases[7].key()).unwrap();
        let error = rejected(reader.read(case.key()));
        assert!(error.path.ends_with(field));
        assert!(!format!("{error:?}").contains("SYNTHETIC_SECRET"));
        assert_eq!(
            reader.loaded_keys().collect::<Vec<_>>(),
            vec![cases[7].key()]
        );
        assert_eq!(
            reader.read(cases[7].key()).unwrap().terrain()[0],
            cases[7].record["terrain"][0].as_str().unwrap()
        );
        assert_eq!(
            archive.worlds()[0]
                .chunks()
                .iter()
                .find(|r| r.key() == case.key())
                .unwrap()
                .private_bytes(),
            before
        );
    }
    for code in ["?", "é"] {
        let copy = Copy::new(case);
        let mut edited = case.record.clone();
        edited["terrain"][0] = json!(code.repeat(40));
        copy.write(case.key(), &edited);
        let archive = copy.import();
        let mut reader = ChunkReader::new(&archive.worlds()[0]).unwrap();
        assert!(
            rejected(reader.read(case.key()))
                .path
                .ends_with("terrain[0]")
        );
    }
    let archive = import_bytes(Some(br#"{"world":{"towns":{}},"tokens":{}}"#), None).unwrap();
    assert!(ChunkReader::new(&archive.worlds()[0]).is_err());
}

#[test]
fn unexplored_raw_chunks_use_saved_version_seed_and_boundaries_without_starter_fallback() {
    for golden in goldens().into_iter().take(7) {
        let copy = Copy::new(&golden);
        // Copy helper omits starter_offset, preserving the legacy default 880.
        let archive = copy.import();
        let world = &archive.worlds()[0];
        let key = golden.key();
        let RawChunk::Generated(generated) = world.raw_chunk(key).unwrap() else {
            panic!("unexpected saved record")
        };
        assert_eq!(json!(generated.terrain), golden.record["terrain"]);
        let resources: Vec<_> = generated
            .resources
            .iter()
            .map(|r| json!({"kind":r.kind,"x":r.x,"y":r.y}))
            .collect();
        assert_eq!(json!(resources), golden.record["map_resources"]);
        assert!(world.raw_chunk(ChunkKey { x: 12, y: 0 }).is_err());
        assert!(world.chunks().is_empty());
        // Independent source selection has no traversal or cache state.
        let RawChunk::Generated(again) = world.raw_chunk(key).unwrap() else {
            panic!("unexpected saved record")
        };
        assert!(generated == again);
    }
    let mut golden = goldens().remove(0);
    golden.size = 1920;
    let copy = Copy::new(&golden);
    let key = ChunkKey { x: 22, y: 22 };
    let archive = copy.import();
    let error = archive.worlds()[0].raw_chunk(key).err().unwrap();
    assert_eq!(
        error.kind,
        lantern_host::ErrorKind::Invalid("starter reconstruction unported")
    );
    // Saved edits take precedence even inside the starter region.
    let mut record = golden.record.clone();
    record["map_resources"] = json!([]);
    copy.write(key, &record);
    let archive = copy.import();
    let RawChunk::Saved(saved) = archive.worlds()[0].raw_chunk(key).unwrap() else {
        panic!("saved chunk regenerated")
    };
    assert_eq!(
        serde_json::from_slice::<Value>(saved.private_bytes()).unwrap(),
        record
    );
}

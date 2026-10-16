use lantern_core::{
    ids::WorldId,
    python_random::PythonRandom,
    seed_material::{SeedSource, gameplay_seed_material_v1},
    terrain::{GenerationError, generate_chunk},
    units::TerrainSeed,
};
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/terrain/parity.json")).unwrap()
}
fn seed_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn assert_chunk(case: &Value) {
    let (x, y) = case["key"].as_str().unwrap().split_once(',').unwrap();
    let chunk = generate_chunk(
        case.get("namespace")
            .and_then(Value::as_str)
            .unwrap_or(&seed_text(&case["seed"])),
        x.parse().unwrap(),
        y.parse().unwrap(),
        case["version"].as_u64().unwrap() as u32,
        case["size"].as_u64().unwrap() as u32,
    )
    .unwrap();
    let resources: Vec<Value> = chunk
        .resources
        .iter()
        .map(|r| {
            let mut v = json!({"kind":r.kind,"x":r.x,"y":r.y});
            if let Some(species) = r.species {
                v["species"] = json!(species);
            }
            v
        })
        .collect();
    let context = format!(
        "seed {} v{} key {} size {}",
        case["seed"], case["version"], case["key"], case["size"]
    );
    assert_eq!(json!(chunk.terrain), case["record"]["terrain"], "{context}");
    let expected = case["record"]["map_resources"].as_array().unwrap();
    for (i, (actual, expected)) in resources.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "{context} resource {i}");
    }
    assert_eq!(resources.len(), expected.len(), "{context}");
}
#[test]
fn all_real_python_cells_and_ordered_resources_match() {
    let f = fixture();
    let chunks = f["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 168);
    for case in chunks {
        assert_chunk(case);
    }
    #[derive(serde::Deserialize)]
    struct Original {
        seed: Box<serde_json::value::RawValue>,
    }
    let bytes = include_bytes!("fixtures/terrain/chunks.json");
    let originals: Vec<Original> = serde_json::from_slice(bytes).unwrap();
    let mut old: Value = serde_json::from_slice(bytes).unwrap();
    for (case, original) in old.as_array_mut().unwrap().iter_mut().zip(originals) {
        case["namespace"] = json!(
            serde_json::from_str::<String>(original.seed.get())
                .unwrap_or_else(|_| original.seed.get().into())
        );
        assert_chunk(case);
    }
}
#[test]
fn vectors_distinguish_every_generator_change_and_include_special_resources() {
    let f = fixture();
    let cases = f["chunks"].as_array().unwrap();
    for version in 1..6 {
        assert!(
            cases
                .iter()
                .filter(|c| c["seed"] == 42 && c["version"] == version)
                .any(|c| cases.iter().any(|n| n["seed"] == 42
                    && n["version"] == version + 1
                    && n["key"] == c["key"]
                    && n["record"] != c["record"]))
        );
    }
    assert!(cases.iter().any(|c| {
        c["record"]["map_resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["species"] == "palm")
    }));
}
#[test]
fn python_integer_text_rng_draws_twists_rejection_and_shuffle_match() {
    for case in fixture()["rngs"].as_array().unwrap() {
        let mut rng = if !case["integer"].as_bool().unwrap() {
            let text = case["namespace"].as_str().unwrap();
            PythonRandom::from_bytes(text.as_bytes())
        } else {
            PythonRandom::from_integer(case["namespace"].as_str().unwrap()).unwrap()
        };
        for draw in case["draws"].as_array().unwrap() {
            assert_eq!(rng.random(), draw.as_f64().unwrap());
        }
        for (i, draw) in case["bits"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                rng.getrandbits([0, 1, 5, 32, 33, 64][i % 6]),
                Some(draw.as_u64().unwrap())
            );
        }
        for (i, draw) in case["below"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                rng.randrange(0, [1, 2, 3, 17, (1i64 << 40) + 1][i % 5]),
                Some(draw.as_i64().unwrap())
            );
        }
        let mut shuffled: Vec<_> = (0..20).collect();
        rng.shuffle(&mut shuffled);
        assert_eq!(json!(shuffled), case["shuffled"]);
        for choice in case["choices"].as_array().unwrap() {
            assert_eq!(rng.choice(&["a", "b", "c"]).copied(), choice.as_str());
        }
    }
    assert!(PythonRandom::from_integer("1.5").is_none());
}
#[test]
fn d05_gameplay_byte_seed_reference_draws_match_without_adopting_gameplay_streams() {
    for (seed, world, subsystem, draws) in [
        (
            SeedSource::Unsigned(TerrainSeed(0)),
            "legacy",
            "economy",
            [
                6330906786906379701,
                11366415972692216933,
                11378455115612559467,
            ],
        ),
        (
            SeedSource::Unsigned(TerrainSeed(u64::MAX)),
            "世界",
            "residents",
            [
                14625236343822065082,
                18176823067106685384,
                16569424490260922686,
            ],
        ),
        (
            SeedSource::Text("0007"),
            "legacy",
            "production",
            [
                18143875931135448615,
                12762892325570383143,
                1493117759899797826,
            ],
        ),
    ] {
        let bytes =
            gameplay_seed_material_v1(seed, &WorldId::new(world).unwrap(), subsystem).unwrap();
        let mut rng = PythonRandom::from_bytes(&bytes);
        for draw in draws {
            assert_eq!(rng.getrandbits(64), Some(draw));
        }
    }
}
#[test]
fn generation_rejects_unsupported_versions_and_world_edges_before_work() {
    for version in [0, 8, u32::MAX] {
        assert!(matches!(
            generate_chunk("42", 0, 0, version, 480),
            Err(GenerationError::UnsupportedVersion)
        ));
    }
    assert!(matches!(
        generate_chunk("42", 0, 0, 7, 120),
        Err(GenerationError::UnsupportedSize)
    ));
    assert!(matches!(
        generate_chunk("42", 12, 0, 7, 480),
        Err(GenerationError::OutsideWorld)
    ));
    assert!(matches!(
        generate_chunk("42", 0, 48, 1, 1920),
        Err(GenerationError::OutsideWorld)
    ));
}

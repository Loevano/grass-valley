use lantern_core::{
    ids::WorldId,
    legacy_map::{MapSeed, generate_map},
    terrain::GenerationError,
};
use lantern_host::legacy::{HydratedLandscape, LandscapeLayout, import_bytes};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/terrain/maps.json")).unwrap()
}
fn resources(landscape: &HydratedLandscape) -> Vec<Value> {
    landscape
        .resources()
        .iter()
        .map(|r| {
            let mut result = json!({"kind":r.kind(), "x":r.tile().x, "y":r.tile().y});
            if let Some(species) = r.species() {
                result["species"] = json!(species);
            }
            for (field, value) in [
                ("supply", r.supply()),
                ("supply_max", r.supply_max()),
                ("regeneration", r.regeneration()),
            ] {
                if let Some(value) = value {
                    result[field] = json!(value);
                }
            }
            result
        })
        .collect()
}
fn envelope(landscape: Value, shared: bool) -> Value {
    let first = json!({"buildings":[], "villagers":[]});
    let second = json!({"buildings":[], "villagers":[]});
    if shared {
        // Town-local stale terrain must be ignored, just as bind_map does.
        json!({"world":{"shared_map":landscape,"towns":{"first": first,"second":dict_override(second)}},"tokens":{}})
    } else {
        let mut town = landscape;
        town["buildings"] = json!([]);
        town["villagers"] = json!([]);
        json!({"world":{"towns":{"first":town}},"tokens":{}})
    }
}
fn dict_override(mut town: Value) -> Value {
    town["terrain"] = json!(["v"]);
    town["map_resources"] = json!([]);
    town
}
fn world_id() -> WorldId {
    WorldId::new("legacy").unwrap()
}

#[test]
fn maps_match_all_real_python_rows_and_ordered_resources() {
    let fixture = fixture();
    let maps = fixture["maps"].as_array().unwrap();
    assert_eq!(maps.len(), 35);
    for case in maps {
        let text = case["seed"].as_str().unwrap();
        let seed = if case["seed_kind"] == "integer" {
            MapSeed::Integer(text)
        } else {
            MapSeed::Text(text)
        };
        let map = generate_map(case["size"].as_u64().unwrap() as u32, seed).unwrap();
        let actual: Vec<Value> = map
            .resources
            .iter()
            .map(|r| {
                let mut item = json!({"kind":r.kind,"x":r.x,"y":r.y});
                if let Some(species) = r.species {
                    item["species"] = json!(species);
                }
                item
            })
            .collect();
        assert_eq!(json!(map.terrain), case["terrain"], "seed {text}");
        assert_eq!(json!(actual), case["map_resources"], "seed {text}");
    }
    assert!(generate_map(0, MapSeed::Integer("0")) == Err(GenerationError::UnsupportedSize));
    assert!(generate_map(1921, MapSeed::Integer("0")) == Err(GenerationError::UnsupportedSize));
    assert!(generate_map(12, MapSeed::Integer("1.5")) == Err(GenerationError::UnsupportedSeed));
    assert!(
        generate_map(120, MapSeed::Integer("42")).unwrap()
            != generate_map(120, MapSeed::Text("42")).unwrap()
    );
}

#[test]
fn deterministic_hydration_matches_python_without_resource_or_clock_changes() {
    for case in fixture()["hydration"].as_array().unwrap() {
        for shared in [false, true] {
            let source = serde_json::to_vec(&envelope(case["source"].clone(), shared)).unwrap();
            let archive = import_bytes(Some(&source), None).unwrap();
            let before = archive.report();
            let hydrated = archive.hydrate_nonchunked(&world_id()).unwrap();
            assert_eq!(
                hydrated.layout(),
                if shared {
                    LandscapeLayout::Shared
                } else {
                    LandscapeLayout::Independent
                }
            );
            assert_eq!(hydrated.landscapes().len(), 1);
            let map = &hydrated.landscapes()[0];
            assert_eq!(json!(map.terrain()), case["expected"]["terrain"]);
            assert_eq!(map.regenerated_ground(), case["mode"] != "saved");
            assert_eq!(map.generator_version(), 7);
            let mut expected = case["expected"]["map_resources"][0].clone();
            expected.as_object_mut().unwrap().remove("extension");
            assert_eq!(resources(map), vec![expected]);
            assert_eq!(map.stumps().len(), 1);
            assert_eq!(map.stumps()[0].remaining(), 0.0625);
            for town in archive.worlds()[0].towns() {
                assert!(std::ptr::eq(hydrated.for_town(town.owner()).unwrap(), map));
            }
            assert_eq!(hydrated.private_source_bytes(), source);
            assert_eq!(archive.private_source_bytes().unwrap(), source);
            assert_eq!(archive.report(), before);
            // Each hydration owns independent decoded views; no authoritative sharing across loads.
            let again = archive.hydrate_nonchunked(&world_id()).unwrap();
            assert!(!std::ptr::eq(&again.landscapes()[0], map));
            assert!(!archive.report().capabilities.simulation_advanced);
        }
    }
}

#[test]
fn independent_towns_and_registry_worlds_keep_their_own_saved_landscapes() {
    let mut independent = envelope(fixture()["hydration"][0]["source"].clone(), false);
    let mut other = independent["world"]["towns"]["first"].clone();
    other["terrain"] = json!(vec!["vvvvvvvvvvvv"; 12]);
    other["map_resources"] = json!([]);
    independent["world"]["towns"]["second"] = other;
    let shared = envelope(fixture()["hydration"][3]["source"].clone(), true);
    let root = json!({"worlds":{"one":{"name":"one","world":independent["world"],"tokens":{}},"two":{"name":"two","world":shared["world"],"tokens":{}}},"active_world":"two"});
    let source = serde_json::to_vec(&root).unwrap();
    let archive = import_bytes(Some(&source), None).unwrap();
    let a = archive
        .hydrate_nonchunked(archive.worlds()[0].id())
        .unwrap();
    let b = archive
        .hydrate_nonchunked(archive.worlds()[1].id())
        .unwrap();
    assert_eq!(a.layout(), LandscapeLayout::Independent);
    assert_eq!(a.landscapes().len(), 2);
    assert_eq!(a.landscapes()[1].terrain(), vec!["vvvvvvvvvvvv"; 12]);
    assert_eq!(b.landscapes()[0].map_size(), 120);
    let towns = archive.worlds()[0].towns();
    assert!(!std::ptr::eq(
        a.for_town(towns[0].owner()).unwrap(),
        a.for_town(towns[1].owner()).unwrap()
    ));
    assert!(a.for_town(archive.worlds()[1].towns()[0].owner()).is_none());
    assert_eq!(archive.private_source_bytes().unwrap(), source);
}

#[test]
fn source_extensions_and_exact_numeric_lexemes_survive_without_default_insertion() {
    let source = br#"{"world":{"towns":{"first":{"buildings":[],"villagers":[],"map_size":1,"terrain":["v"],"map_resources":[{"kind":"unported","x":0,"y":0,"supply":1.2300e-3,"extension":123456789012345678901234567890}],"extension":{"kept":true}}}},"tokens":{}}"#;
    let archive = import_bytes(Some(source), None).unwrap();
    let result = archive.hydrate_nonchunked(&world_id()).unwrap();
    let map = &result.landscapes()[0];
    assert_eq!(map.terrain(), ["v"]);
    assert_eq!(map.generator_version(), 1);
    assert!(!map.regenerated_ground());
    assert_eq!(map.resources()[0].supply(), Some(0.00123));
    assert_eq!(map.resources()[0].supply_max(), None);
    assert_eq!(map.resources()[0].regeneration(), None);
    assert!(map.stumps().is_empty());
    assert_eq!(result.private_source_bytes(), source);
}

#[test]
fn absent_size_defaults_to_twelve_and_text_seed_is_never_a_chunk_namespace() {
    let source = envelope(
        json!({"terrain_seed":"É🌲/seed:","map_resources":[]}),
        false,
    );
    let source = serde_json::to_vec(&source).unwrap();
    let archive = import_bytes(Some(&source), None).unwrap();
    let hydrated = archive.hydrate_nonchunked(&world_id()).unwrap();
    let expected = generate_map(12, MapSeed::Text("É🌲/seed:")).unwrap();
    assert_eq!(hydrated.landscapes()[0].terrain(), expected.terrain);
    assert!(hydrated.landscapes()[0].resources().is_empty());
}

#[test]
fn unsupported_inputs_fail_without_changing_the_archive() {
    let base = fixture()["hydration"][0]["source"].clone();
    let mut failures = Vec::new();
    for (field, value) in [
        ("map_size", json!(0)),
        ("map_size", json!(1921)),
        ("terrain", json!(vec!["bad-width"; 12])),
        ("terrain", json!(vec!["????????????"; 12])),
        ("map_resources", json!([{"kind":"tree","x":12,"y":0}])),
        (
            "map_resources",
            json!([{"kind":"tree","x":0,"y":0,"supply":-1}]),
        ),
        (
            "map_resources",
            json!([{"kind":"tree","x":0,"y":0,"species":null}]),
        ),
        ("tree_stumps", json!([{"x":0,"y":-1,"remaining":1}])),
    ] {
        let mut source = base.clone();
        source[field] = value;
        failures.push(source);
    }
    for seed in [Value::Null, json!(true), json!(1.5), json!({}), json!([])] {
        let mut source = base.clone();
        source.as_object_mut().unwrap().remove("terrain");
        source["terrain_seed"] = seed;
        failures.push(source);
    }
    for field in ["terrain_seed", "map_resources"] {
        let mut source = base.clone();
        source.as_object_mut().unwrap().remove("terrain");
        source.as_object_mut().unwrap().remove(field);
        failures.push(source);
    }
    for landscape in failures {
        let source = serde_json::to_vec(&envelope(landscape, false)).unwrap();
        let archive = import_bytes(Some(&source), None).unwrap();
        assert!(archive.hydrate_nonchunked(&world_id()).is_err());
        assert_eq!(archive.private_source_bytes().unwrap(), source);
    }
    let archive = import_bytes(Some(include_bytes!("fixtures/legacy_world.json")), None).unwrap();
    assert!(
        archive
            .hydrate_nonchunked(&WorldId::new("unknown").unwrap())
            .is_err()
    );
}

#[test]
fn hydration_retains_all_versions_and_arbitrary_precision_integer_seed_semantics() {
    let fixture = fixture();
    let expected = fixture["maps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["seed_kind"] == "integer"
                && case["seed"].as_str().unwrap().starts_with('-')
                && case["size"] == 120
        })
        .unwrap();
    let decimal = expected["seed"].as_str().unwrap();
    for version in 1..=7 {
        let source = format!(
            r#"{{"world":{{"towns":{{"first":{{"buildings":[],"villagers":[],"terrain_seed":{decimal},"map_size":120,"terrain_version":{version},"map_resources":[]}}}}}},"tokens":{{}}}}"#
        );
        let archive = import_bytes(Some(source.as_bytes()), None).unwrap();
        let hydrated = archive.hydrate_nonchunked(&world_id()).unwrap();
        let rows: Vec<String> = expected["terrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().replace('v', "g"))
            .collect();
        assert_eq!(hydrated.landscapes()[0].terrain(), rows);
        assert_eq!(hydrated.landscapes()[0].generator_version(), version);
        assert_eq!(hydrated.private_source_bytes(), source.as_bytes());
    }
    for version in [0, 8, 4294967297u64] {
        let source = envelope(
            json!({"terrain_version":version,"terrain":["g"],"map_size":1,"map_resources":[]}),
            false,
        );
        assert!(import_bytes(Some(&serde_json::to_vec(&source).unwrap()), None).is_err());
    }
}

#[test]
fn chunked_hydration_is_explicitly_refused_without_loading_or_generating() {
    use lantern_host::legacy::{ChunkDirectory, import_with_chunk_copies};
    let root =
        std::env::temp_dir().join(format!("lantern-nonchunked-refusal-{}", std::process::id()));
    let directory = root.join("42");
    std::fs::create_dir_all(&directory).unwrap();
    let source = br#"{"world":{"towns":{},"shared_map":{"chunked":1,"chunk_size":40,"map_size":480,"terrain_seed":42,"terrain_version":7}},"tokens":{}}"#;
    let archive = import_with_chunk_copies(
        Some(source),
        None,
        &[ChunkDirectory::new(&world_id(), &directory)],
    )
    .unwrap();
    assert!(archive.hydrate_nonchunked(&world_id()).is_err());
    assert!(archive.worlds()[0].chunks().is_empty());
    assert_eq!(archive.private_source_bytes().unwrap(), source);
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}

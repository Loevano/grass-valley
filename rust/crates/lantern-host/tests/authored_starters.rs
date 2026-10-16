use lantern_core::{legacy_map::MapSeed, starter::generate_starter, terrain::GenerationError};
use serde_json::{Value, json};

fn fixtures() -> Vec<Value> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/starters.json")).unwrap()
}

#[test]
fn real_python_starter_ground_resources_and_site_order_match_all_versions() {
    let cases = fixtures();
    assert_eq!(cases.len(), 49);
    for case in cases {
        let text = case["seed"].as_str().unwrap();
        let seed = if case["seed_kind"] == "integer" {
            MapSeed::Integer(text)
        } else {
            MapSeed::Text(text)
        };
        let version = case["version"].as_u64().unwrap() as u32;
        let actual = generate_starter(seed, version).unwrap();
        let context = format!("seed {text:?} kind {} version {version}", case["seed_kind"]);
        assert_eq!(
            json!(actual.landscape.terrain),
            case["terrain"],
            "{context}"
        );
        let resources: Vec<_> = actual
            .landscape
            .resources
            .iter()
            .map(|r| {
                let mut value = json!({"kind":r.kind,"x":r.x,"y":r.y});
                if let Some(species) = r.species {
                    value["species"] = json!(species);
                }
                value
            })
            .collect();
        assert_eq!(
            resources.len(),
            case["map_resources"].as_array().unwrap().len(),
            "{context}"
        );
        for (index, (a, b)) in resources
            .iter()
            .zip(case["map_resources"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(a, b, "{context} resource {index}");
        }
        let sites: Vec<_> = actual
            .sites
            .iter()
            .map(|s| json!({"x":s.x,"y":s.y}))
            .collect();
        assert_eq!(json!(sites), case["sites"], "{context}");
        for (i, a) in actual.sites.iter().enumerate() {
            for b in &actual.sites[i + 1..] {
                let (dx, dy) = (
                    i64::from(a.x) - i64::from(b.x),
                    i64::from(a.y) - i64::from(b.y),
                );
                assert!(dx * dx + dy * dy >= 40 * 40);
            }
            assert_eq!(
                actual.landscape.terrain[a.y as usize].as_bytes()[a.x as usize],
                if version >= 6 { b'm' } else { b'g' }
            );
        }
        assert_eq!(
            actual
                .landscape
                .resources
                .iter()
                .filter(|r| r.kind == "chest")
                .count(),
            3
        );
    }
}

#[test]
fn seed_types_are_explicit_and_integer_namespaces_are_canonical() {
    for version in 1..=7 {
        let integer = generate_starter(MapSeed::Integer("42"), version).unwrap();
        let text = generate_starter(MapSeed::Text("42"), version).unwrap();
        assert_ne!(integer.sites, text.sites);
        let leading = generate_starter(MapSeed::Integer("00042"), version).unwrap();
        assert_eq!(integer.sites, leading.sites);
        assert!(integer.landscape == leading.landscape);
        let zero = generate_starter(MapSeed::Integer("0"), version).unwrap();
        let negative_zero = generate_starter(MapSeed::Integer("-000"), version).unwrap();
        assert!(zero.landscape == negative_zero.landscape);
    }
    for version in [0, 8, u32::MAX] {
        assert!(matches!(
            generate_starter(MapSeed::Integer("42"), version),
            Err(GenerationError::UnsupportedVersion)
        ));
    }
    for seed in ["", "-", "+42", "4.2", "one"] {
        assert!(matches!(
            generate_starter(MapSeed::Integer(seed), 7),
            Err(GenerationError::UnsupportedSeed)
        ));
    }
}

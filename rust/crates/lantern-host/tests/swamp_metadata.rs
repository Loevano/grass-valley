use lantern_host::legacy::{OrdinaryVillageMetadata, SwampVillageMetadata};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Case {
    label: String,
    before: Box<serde_json::value::RawValue>,
    after: Box<serde_json::value::RawValue>,
    repeated: Box<serde_json::value::RawValue>,
    plan: Vec<usize>,
}
fn cases() -> Vec<Case> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/swamp_metadata.json")).unwrap()
}
#[test]
fn actual_python_worlds_exact_order_repeats_and_runtime_effect_plans() {
    let cases = cases();
    assert_eq!(cases.len(), 264);
    for case in cases {
        let source = case.before.get().as_bytes();
        let output = SwampVillageMetadata::materialize(source)
            .unwrap_or_else(|e| panic!("{}: {e:?}", case.label));
        assert_eq!(output.private_source_bytes(), source);
        assert_eq!(
            std::str::from_utf8(output.private_world_bytes()).unwrap(),
            case.after.get(),
            "{}",
            case.label
        );
        assert_eq!(
            output.cache_plan().village_ordinals,
            case.plan,
            "{}",
            case.label
        );
        let repeated = SwampVillageMetadata::materialize(output.private_world_bytes()).unwrap();
        assert_eq!(
            std::str::from_utf8(repeated.private_world_bytes()).unwrap(),
            case.repeated.get(),
            "repeat {}",
            case.label
        );
        assert!(repeated.cache_plan().village_ordinals.is_empty());
    }
}
#[test]
fn ordinary_then_swamp_matches_actual_python_phase_boundary() {
    for case in cases()
        .into_iter()
        .filter(|c| c.label.starts_with("ordinary-chain"))
    {
        // Before is the actual ordinary output, making composition a checked
        // explicit boundary rather than an implicit ImportedWorld upgrade.
        let ordinary = OrdinaryVillageMetadata::materialize(case.before.get().as_bytes()).unwrap();
        let output = SwampVillageMetadata::materialize(ordinary.private_world_bytes()).unwrap();
        assert_eq!(
            std::str::from_utf8(output.private_world_bytes()).unwrap(),
            case.after.get(),
            "{}",
            case.label
        );
    }
}
#[test]
fn source_extensions_quantities_and_transferred_progress_lexemes_survive() {
    let source = br#" {"towns":{"secret-key":{"local_visits":{"Aster:70:190":{"trade_remaining":1.2500e-1,"quantity":123456789012345678901234567890,"extension":-0.00},"tail":{}},"credential":"synthetic-only"}},"shared_map":{"map_size":480,"villages":[{"name":"Reedhaven","biome":"b","x":80,"y":200,"layout_version":2,"life_version":1,"swamp_version":1}],"npcs":[{"name":"Aster","x":70,"y":190,"village":"Reedhaven","extension":1.2300e-04,"routine":{"x":70,"y":190}},{"name":"Fen","x":79,"y":201,"village":"Reedhaven","routine":{"x":80.00,"y":200.000,"extension":2.500e-1}}]},"private":{"credential":"synthetic-only"}} "#;
    let output = SwampVillageMetadata::materialize(source).unwrap();
    assert_eq!(output.private_source_bytes(), source);
    let text = std::str::from_utf8(output.private_world_bytes()).unwrap();
    for lexeme in [
        "1.2500e-1",
        "123456789012345678901234567890",
        "-0.00",
        "1.2300e-04",
        "80.00",
        "200.000",
        "2.500e-1",
        "synthetic-only",
    ] {
        assert!(text.contains(lexeme), "{lexeme}");
    }
    let value: Value = serde_json::from_slice(output.private_world_bytes()).unwrap();
    assert!(
        value["towns"]["secret-key"]["local_visits"]
            .get("Aster:70:190")
            .is_none()
    );
    assert_eq!(
        value["shared_map"]["npcs"][1]["routine"]["origin"],
        json!([80.0, 200.0])
    );
}
#[test]
fn malformed_or_future_noop_metadata_rejects_without_private_identifiers() {
    let base = json!({"towns":{},"shared_map":{"map_size":480,"villages":[{"name":"Reedhaven","biome":"b","x":80,"y":200,"layout_version":2,"life_version":1,"swamp_version":2}],"npcs":[]}});
    let mut cases = Vec::new();
    for (field, value) in [
        ("layout_version", json!(3)),
        ("life_version", json!(2)),
        ("swamp_version", json!(3)),
        ("name", json!("private-secret")),
        ("biome", json!("private-secret")),
        ("x", json!(-1)),
        ("swamp_version", json!(null)),
    ] {
        let mut w = base.clone();
        w["shared_map"]["villages"][0][field] = value;
        cases.push(w);
    }
    for state in [
        json!(true),
        json!([]),
        json!({"x":"private-secret","y":200}),
        json!({"x":1e100,"y":0}),
        json!({"x":0,"y":0,"path":[[1]]}),
        json!({"x":0,"y":0,"origin":null}),
        json!({"x":0,"y":0,"wait":-0.1}),
        json!({"x":0,"y":0,"phase":3}),
    ] {
        let mut w = base.clone();
        w["shared_map"]["npcs"] = json!([{"name":"private-secret","x":79,"y":201,"routine":state}]);
        cases.push(w);
    }
    for item in [
        json!({"x":0}),
        json!({"x":0,"y":0,"footprint":null}),
        json!({"x":0,"y":0,"footprint":[1,"private-secret"]}),
    ] {
        let mut w = base.clone();
        w["towns"] = json!({"private-secret":{"buildings":[item]}});
        cases.push(w);
    }
    let mut w = base.clone();
    w["towns"] = json!({"private-secret":{"local_visits":[]}});
    cases.push(w);
    for marker in [json!(0), json!(8), json!("private-secret")] {
        let mut w = base.clone();
        w["shared_map"]["terrain_version"] = marker;
        cases.push(w);
    }
    let mut w = base.clone();
    w["_chunks"] = json!({});
    cases.push(w);
    let mut w = base.clone();
    w.as_object_mut().unwrap().remove("shared_map");
    cases.push(w);
    let mut w = base.clone();
    w["shared_map"]["npcs"] = json!([{"name":"private-secret","x":0,"y":0,"ambient_stop":[1]}]);
    cases.push(w);
    for w in cases {
        let err = match SwampVillageMetadata::materialize(&serde_json::to_vec(&w).unwrap()) {
            Err(e) => e,
            Ok(_) => panic!("malformed no-op metadata accepted"),
        };
        assert!(err.path.starts_with("$.swamp_village_metadata"));
        assert!(!format!("{err:?}").contains("private-secret"));
    }
}

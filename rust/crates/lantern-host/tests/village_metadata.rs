use lantern_host::legacy::OrdinaryVillageMetadata;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Case {
    label: String,
    before: Value,
    after: Value,
    repeated: Value,
}
fn cases() -> Vec<Case> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/village_metadata.json")).unwrap()
}
#[test]
fn actual_python_ordinary_phase_matches_complete_worlds_and_repeat_behavior() {
    let cases = cases();
    assert_eq!(cases.len(), 105);
    for case in cases {
        let source = serde_json::to_vec(&case.before).unwrap();
        let output = OrdinaryVillageMetadata::materialize(&source).unwrap();
        assert_eq!(output.private_source_bytes(), source);
        assert_eq!(
            serde_json::from_slice::<Value>(output.private_world_bytes()).unwrap(),
            case.after,
            "{}",
            case.label
        );
        let repeated = OrdinaryVillageMetadata::materialize(output.private_world_bytes()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(repeated.private_world_bytes()).unwrap(),
            case.repeated,
            "repeat {}",
            case.label
        );
        assert_eq!(source, serde_json::to_vec(&case.before).unwrap());
    }
}
#[test]
fn retained_private_fields_numeric_lexemes_and_source_are_lossless() {
    let source = br#" {"towns":{"private-town":{"visits":{"old-key":1.2500e-1},"stock":123456789012345678901234567890}},"shared_map":{"map_size":480,"starter_offset":160,"villages":[{"name":"Reedhaven","biome":"b","x":80,"y":200,"layout_version":2,"life_version":1,"swamp_version":1}],"npcs":[{"name":"Fen","x":79,"y":201,"village":"Reedhaven","routine":{"fraction":-0.00,"home":[80,201]},"dialogue":"old"}]},"private":{"credential":"synthetic-only","quantity":1.2300e-04}} "#;
    let output = OrdinaryVillageMetadata::materialize(source).unwrap();
    let bytes = std::str::from_utf8(output.private_world_bytes()).unwrap();
    for lexeme in [
        "1.2500e-1",
        "123456789012345678901234567890",
        "-0.00",
        "1.2300e-04",
        "synthetic-only",
        "old-key",
    ] {
        assert!(bytes.contains(lexeme));
    }
    assert_eq!(output.private_source_bytes(), source);
    let value: Value = serde_json::from_slice(output.private_world_bytes()).unwrap();
    assert_eq!(value["shared_map"]["villages"][0]["swamp_version"], 1);
    assert_eq!(
        value["shared_map"]["npcs"][0]["routine"]["home"],
        json!([80, 201])
    );
    assert!(
        value["shared_map"]["npcs"][0]["dialogue"]
            .as_str()
            .unwrap()
            .contains("raised cottages")
    );
}
#[test]
fn future_versions_and_malformed_noop_metadata_fail_at_sanitized_paths() {
    let base = json!({"towns":{},"shared_map":{"map_size":480,"villages":[{"name":"Sunwell","biome":"d","x":80,"y":200,"layout_version":2,"life_version":1,"swamp_version":2}],"npcs":[]}});
    let mut inputs = Vec::new();
    for (field, value) in [
        ("layout_version", json!(3)),
        ("life_version", json!(2)),
        ("swamp_version", json!(3)),
        ("name", json!("private-secret")),
        ("biome", json!("private-secret")),
        ("x", json!(-1)),
        ("y", json!(480)),
        ("layout_version", json!(null)),
    ] {
        let mut input = base.clone();
        input["shared_map"]["villages"][0][field] = value;
        inputs.push(input);
    }
    for version in [json!(0), json!(8), json!("private-secret")] {
        let mut input = base.clone();
        input["shared_map"]["terrain_version"] = version;
        inputs.push(input);
    }
    for item in [
        json!({"x":1,"y":1,"footprint":[1]}),
        json!({"x":1,"y":1,"footprint":[1,"private-secret"]}),
        json!({"x":1,"y":1,"footprint":null}),
        json!({"x":1}),
    ] {
        let mut input = base.clone();
        input["towns"] = json!({"private-secret":{"buildings":[item]}});
        inputs.push(input);
    }
    let mut input = base.clone();
    input["shared_map"]["npcs"] = json!([{"name":"private-secret","x":1,"y":1,"village":{}}]);
    inputs.push(input);
    let mut input = base.clone();
    input["_chunks"] = json!({});
    inputs.push(input);
    let mut input = base.clone();
    input.as_object_mut().unwrap().remove("shared_map");
    inputs.push(input);
    for input in inputs {
        let err = match OrdinaryVillageMetadata::materialize(&serde_json::to_vec(&input).unwrap()) {
            Err(err) => err,
            Ok(_) => panic!("malformed metadata accepted"),
        };
        assert!(err.path.starts_with("$.ordinary_village_metadata"));
        assert!(!format!("{err:?}").contains("private-secret"));
    }
}

#[test]
fn python_object_and_array_insertion_order_is_retained() {
    #[derive(Deserialize)]
    struct OrderedCase {
        label: String,
        before: Box<serde_json::value::RawValue>,
        after: Box<serde_json::value::RawValue>,
    }
    let cases: Vec<OrderedCase> =
        serde_json::from_slice(include_bytes!("fixtures/terrain/village_metadata.json")).unwrap();
    for case in cases {
        let output = OrdinaryVillageMetadata::materialize(case.before.get().as_bytes()).unwrap();
        assert_eq!(
            std::str::from_utf8(output.private_world_bytes()).unwrap(),
            case.after.get(),
            "{}",
            case.label
        );
    }
}

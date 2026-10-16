use lantern_host::{
    BoundaryError,
    legacy::{ChunkKey, OrdinaryVillageOverlay},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Golden {
    villages: Value,
    key: String,
    size: u32,
    before: Value,
    after: Value,
}
fn goldens() -> Vec<Golden> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/village_overlays.json")).unwrap()
}
fn key(text: &str) -> ChunkKey {
    let (x, y) = text.split_once(',').unwrap();
    ChunkKey {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
    }
}
fn materialize(
    case: &Golden,
    before: &Value,
    villages: &Value,
) -> Result<OrdinaryVillageOverlay, BoundaryError> {
    OrdinaryVillageOverlay::materialize(
        &serde_json::to_vec(before).unwrap(),
        &serde_json::to_vec(villages).unwrap(),
        key(&case.key),
        case.size,
    )
}
fn error(result: Result<OrdinaryVillageOverlay, BoundaryError>) -> BoundaryError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("unsupported input accepted"),
    }
}

#[test]
fn ordinary_phase_matches_all_real_python_cells_and_ordered_resources() {
    let cases = goldens();
    assert_eq!(cases.len(), 150);
    for (index, case) in cases.iter().enumerate() {
        let before = serde_json::to_vec(&case.before).unwrap();
        let villages = serde_json::to_vec(&case.villages).unwrap();
        let view =
            OrdinaryVillageOverlay::materialize(&before, &villages, key(&case.key), case.size)
                .unwrap();
        let actual: Value = serde_json::from_slice(view.private_record_bytes()).unwrap();
        // Full equality includes all terrain cells, list ordering, quantities,
        // species/palms, landmarks, animals, clocks, stumps and extensions.
        assert_eq!(actual, case.after, "case {index}");
        assert_eq!(view.private_source_bytes(), before);
        let again = OrdinaryVillageOverlay::materialize(
            view.private_record_bytes(),
            &villages,
            key(&case.key),
            case.size,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(again.private_record_bytes()).unwrap(),
            actual,
            "idempotence {index}"
        );
        // Each call owns its record; materialization never edits either input.
        assert_eq!(before, serde_json::to_vec(&case.before).unwrap());
        assert_eq!(villages, serde_json::to_vec(&case.villages).unwrap());
    }
}

#[test]
fn unknown_extensions_and_exact_scalar_spellings_survive_on_retained_items() {
    let case = &goldens()[0];
    let mut before = case.before.clone();
    let items = before["map_resources"].as_array_mut().unwrap();
    items.push(json!({"kind":"custom_resource","x":119,"y":119,"supply":0.25,"unknown":{"secret":"extension"}}));
    let mut bytes = serde_json::to_string(&before).unwrap();
    bytes.pop();
    bytes.push_str(r#", "private_big":12345678901234567890123456789012345678901234567890,"private_float":1.2300e-04}"#);
    bytes = bytes.replace("\"supply\":0.25", "\"supply\":2.500e-1");
    let view = OrdinaryVillageOverlay::materialize(
        bytes.as_bytes(),
        &serde_json::to_vec(&case.villages).unwrap(),
        key(&case.key),
        case.size,
    )
    .unwrap();
    let output = std::str::from_utf8(view.private_record_bytes()).unwrap();
    assert!(output.contains("12345678901234567890123456789012345678901234567890"));
    assert!(output.contains("1.2300e-04"));
    assert!(output.contains("2.500e-1"));
    assert!(output.contains("custom_resource"));
    assert!(output.contains("extension"));
    assert_eq!(view.private_source_bytes(), bytes.as_bytes());
}

#[test]
fn completed_marker_preserves_saved_edits_and_life_quantities() {
    let case = goldens()
        .into_iter()
        .find(|case| case.before["village_version"] == 3 && case.villages[0]["life_version"] == 1)
        .unwrap();
    let mut before = case.before.clone();
    before["terrain"][0] = json!("v".repeat(40));
    before["map_resources"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["kind"] != "water");
    for item in before["map_resources"].as_array_mut().unwrap() {
        item["supply"] = json!(0.0625);
        item["unknown"] = json!({"saved":true});
    }
    let view = materialize(&case, &before, &case.villages).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(view.private_record_bytes()).unwrap(),
        before
    );
}

#[test]
fn relevant_unsupported_versions_metadata_and_record_shapes_fail_without_values_in_diagnostics() {
    let case = &goldens()[0];
    for (field, value) in [
        ("village_version", json!(4)),
        ("swamp_version", json!(3)),
        ("terrain_version", json!(8)),
        ("village_version", json!("private-secret")),
        ("clock", json!(-0.125)),
    ] {
        let mut before = case.before.clone();
        before[field] = value;
        let err = error(materialize(case, &before, &case.villages));
        assert!(err.path.starts_with("$.ordinary_village_overlay"));
        assert!(!format!("{err:?}").contains("private-secret"));
    }
    for (field, value) in [
        ("layout_version", json!(3)),
        ("layout_version", json!(0)),
        ("life_version", json!(2)),
        ("swamp_version", json!(3)),
        ("name", json!("private-secret")),
        ("biome", json!("d")),
        ("x", json!(-1)),
        ("y", json!(i32::MAX)),
    ] {
        let mut villages = case.villages.clone();
        villages[0][field] = value;
        // Check even when this record is already fully overlaid.
        let mut before = case.before.clone();
        before["village_version"] = json!(3);
        let err = error(materialize(case, &before, &villages));
        assert!(
            err.path
                .starts_with("$.ordinary_village_overlay.villages[0]")
        );
        assert!(!format!("{err:?}").contains("private-secret"));
    }
    let mut before = case.before.clone();
    before["terrain"][0] = json!("g".repeat(39));
    assert!(
        error(materialize(case, &before, &case.villages))
            .path
            .ends_with("terrain[0]")
    );
    let mut before = case.before.clone();
    before["map_resources"][0]["x"] = json!(0);
    assert!(
        error(materialize(case, &before, &case.villages))
            .path
            .contains("map_resources[0]")
    );
    let mut before = case.before.clone();
    before["map_resources"][0]["supply"] = json!(-0.1);
    assert!(
        error(materialize(case, &before, &case.villages))
            .path
            .ends_with("supply")
    );
    assert!(
        OrdinaryVillageOverlay::materialize(b"{}", b"[]", ChunkKey { x: u32::MAX, y: 0 }, 480)
            .is_err()
    );
    assert!(OrdinaryVillageOverlay::materialize(b"{}", b"[]", key(&case.key), 120).is_err());
    assert!(
        OrdinaryVillageOverlay::materialize(
            b"{\"terrain\":[],\"terrain\":[]}",
            b"[]",
            key(&case.key),
            480
        )
        .is_err()
    );
}

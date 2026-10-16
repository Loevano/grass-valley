use lantern_host::{
    BoundaryError,
    legacy::{ChunkKey, SwampOverlay},
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
    again: Value,
}
fn goldens() -> Vec<Golden> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/organic_swamp.json")).unwrap()
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
) -> Result<SwampOverlay, BoundaryError> {
    SwampOverlay::materialize(
        &serde_json::to_vec(before).unwrap(),
        &serde_json::to_vec(villages).unwrap(),
        key(&case.key),
        case.size,
    )
}
#[test]
fn versioned_swamp_matches_real_python_full_cells_ordered_resources_and_saved_edits() {
    let cases = goldens();
    assert_eq!(cases.len(), 100);
    for (index, case) in cases.iter().enumerate() {
        let view = materialize(case, &case.before, &case.villages).unwrap();
        let actual: Value = serde_json::from_slice(view.private_record_bytes()).unwrap();
        assert_eq!(actual, case.after, "case {index}");
        assert_eq!(
            view.private_source_bytes(),
            serde_json::to_vec(&case.before).unwrap()
        );
        let again = materialize(case, &actual, &case.villages).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(again.private_record_bytes()).unwrap(),
            if case.again.is_null() {
                &case.after
            } else {
                &case.again
            }
            .clone(),
            "repeat Python materialization {index}"
        );
    }
}
#[test]
fn retained_exact_scalars_and_source_bytes_survive() {
    let case = &goldens()[0];
    let mut bytes = serde_json::to_string(&case.before)
        .unwrap()
        .replace("\"supply\":0.125", "\"supply\":1.2500e-1");
    bytes.pop();
    bytes.push_str(r#", "big":1234567890123456789012345678901234567890,"float":1.2300e-04}"#);
    let view = SwampOverlay::materialize(
        bytes.as_bytes(),
        &serde_json::to_vec(&case.villages).unwrap(),
        key(&case.key),
        case.size,
    )
    .unwrap();
    let output = std::str::from_utf8(view.private_record_bytes()).unwrap();
    assert!(output.contains("1234567890123456789012345678901234567890"));
    assert!(output.contains("1.2300e-04"));
    assert!(output.contains("1.2500e-1"));
    assert_eq!(view.private_source_bytes(), bytes.as_bytes());
}
fn rejected(result: Result<SwampOverlay, BoundaryError>) -> BoundaryError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("unsupported input accepted"),
    }
}
#[test]
fn unknown_metadata_fails_even_for_completed_records() {
    let case = &goldens()[0];
    for (field, value) in [
        ("swamp_version", json!(3)),
        ("swamp_version", json!("private-secret")),
        ("village_version", json!(4)),
        ("terrain_version", json!(8)),
        ("clock", json!(-1)),
    ] {
        let mut before = case.before.clone();
        before[field] = value;
        let err = rejected(materialize(case, &before, &case.villages));
        assert!(err.path.starts_with("$.swamp_overlay"));
        assert!(!format!("{err:?}").contains("private-secret"));
    }
    for (field, value) in [
        ("swamp_version", json!(3)),
        ("layout_version", json!(3)),
        ("life_version", json!(2)),
        ("name", json!("private-secret")),
        ("biome", json!("i")),
        ("x", json!(-1)),
    ] {
        let mut villages = case.villages.clone();
        villages[0][field] = value;
        let mut before = case.before.clone();
        before["swamp_version"] = json!(2);
        let err = rejected(materialize(case, &before, &villages));
        assert!(err.path.starts_with("$.swamp_overlay.villages[0]"));
        assert!(!format!("{err:?}").contains("private-secret"));
    }
    for (field, value) in [
        ("terrain", json!([])),
        ("map_resources", json!({})),
        ("tree_stumps", json!([{"x":-1,"y":0}])),
    ] {
        let mut before = case.before.clone();
        before[field] = value;
        assert!(materialize(case, &before, &case.villages).is_err());
    }
    assert!(SwampOverlay::materialize(b"{}", b"[]", ChunkKey { x: u32::MAX, y: 0 }, 480).is_err());
    assert!(SwampOverlay::materialize(b"{}", b"[]", key(&case.key), 120).is_err());
}

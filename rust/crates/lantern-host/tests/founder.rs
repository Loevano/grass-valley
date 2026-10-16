use lantern_core::units::Position;
use lantern_host::legacy::PrivateFounderCopy;
use serde_json::{Value, json};
fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/founder/python.json")).unwrap()
}
#[test]
fn actual_python_founder_traces_and_queries_preserve_private_source() {
    for case in vectors()["cases"].as_array().unwrap() {
        let source = serde_json::to_vec(&case["source"]).unwrap();
        let original = source.clone();
        let copy = PrivateFounderCopy::decode(&source).unwrap();
        let mut state = copy.state();
        let inputs: Vec<_> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32))
            .collect();
        for (i, &input) in inputs.iter().enumerate() {
            let result = copy.walk_batch(state, &[input]).unwrap();
            let expected = &case["expected"][i];
            assert!(
                (result.state.position.x.get() - expected["x"].as_f64().unwrap()).abs() < 1e-12,
                "{} step {i} x",
                case["name"]
            );
            assert!(
                (result.state.position.y.get() - expected["y"].as_f64().unwrap()).abs() < 1e-12,
                "{} step {i} y",
                case["name"]
            );
            assert_eq!(
                i64::from(result.state.facing),
                expected["facing"].as_i64().unwrap()
            );
            assert_eq!(result.clear_activity, input != (0, 0));
            state = result.state;
        }
        assert_eq!(copy.walk_batch(copy.state(), &inputs).unwrap().state, state);
        for (probe, expected) in case["probes"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["open"].as_array().unwrap())
        {
            assert_eq!(
                copy.walkable(
                    Position::new(probe[0].as_f64().unwrap(), probe[1].as_f64().unwrap()).unwrap()
                )
                .unwrap(),
                expected.as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(source, original);
        assert_eq!(copy.private_bytes(), original);
        assert_eq!(
            copy.state().position.x.get(),
            case["source"]["player"]["x"].as_f64().unwrap()
        );
    }
}
#[test]
fn invalid_input_rejects_atomically_and_errors_do_not_leak_private_values() {
    let mut value = vectors()["cases"][0]["source"].clone();
    let source = serde_json::to_vec(&value).unwrap();
    let copy = PrivateFounderCopy::decode(&source).unwrap();
    let state = copy.state();
    for batch in [vec![], vec![(1, 0); 6], vec![(1, 0), (2, 0)]] {
        assert!(copy.walk_batch(state, &batch).is_err());
    }
    assert_eq!(copy.state(), state);
    assert_eq!(copy.private_bytes(), source);
    for key in ["x", "boat", "boat_level", "equipment", "mounted", "facing"] {
        let mut malformed = value.clone();
        malformed["player"][key] = if key == "equipment" {
            json!({"feet": {"private-secret":"never print me"}})
        } else {
            json!({"private-secret":"never print me"})
        };
        let bytes = serde_json::to_vec(&malformed).unwrap();
        let error = PrivateFounderCopy::decode(&bytes).err().unwrap();
        assert!(!format!("{error:?}").contains("private-secret"));
        assert!(!format!("{error:?}").contains("never print me"));
        assert_eq!(bytes, serde_json::to_vec(&malformed).unwrap());
    }
    value["player"]["x"] = json!(50);
    let copy = PrivateFounderCopy::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        copy.walk_batch(copy.state(), &[(0, 0)]).is_err(),
        "blocked start requires host recovery"
    );
    value["chunked"] = json!(true);
    assert!(PrivateFounderCopy::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}

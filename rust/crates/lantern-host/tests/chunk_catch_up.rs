use lantern_host::legacy::{CaughtUpChunk, ChunkKey};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

#[derive(Deserialize)]
struct Case {
    label: String,
    key: String,
    size: u32,
    version: u32,
    clock: Box<RawValue>,
    occupied: Vec<(i32, i32)>,
    source: Box<RawValue>,
    expected: Box<RawValue>,
    repeat: Box<RawValue>,
}
fn cases() -> Vec<Case> {
    serde_json::from_slice(include_bytes!("fixtures/terrain/catch_up.json")).unwrap()
}
fn key(text: &str) -> ChunkKey {
    let (x, y) = text.split_once(',').unwrap();
    ChunkKey {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
    }
}
fn equal(actual: &Value, expected: &Value, label: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            assert_eq!(a.as_f64(), b.as_f64(), "{label}")
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{label}");
            for (k, v) in b {
                equal(
                    a.get(k).unwrap_or_else(|| panic!("{label}: missing {k}")),
                    v,
                    &format!("{label}.{k}"),
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equal(a, b, &format!("{label}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{label}"),
    }
}
fn assert_record(actual: &[u8], expected: &[u8], label: &str) {
    equal(
        &serde_json::from_slice(actual).unwrap(),
        &serde_json::from_slice(expected).unwrap(),
        label,
    );
    // Compare object field order independently, including preserved extensions.
    fn order(bytes: &[u8]) -> Vec<String> {
        // RawValue avoids coercing private numeric lexemes through Value.
        #[derive(Deserialize)]
        struct Ordered(#[serde(deserialize_with = "ordered")] Vec<String>);
        fn ordered<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Vec<String>;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut m: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut keys = Vec::new();
                    while let Some((key, _)) = m.next_entry::<String, Box<RawValue>>()? {
                        keys.push(key);
                    }
                    Ok(keys)
                }
            }
            d.deserialize_map(Visitor)
        }
        serde_json::from_slice::<Ordered>(bytes).unwrap().0
    }
    assert_eq!(order(actual), order(expected), "{label}: root order");
}
#[test]
fn actual_python_catch_up_records_and_repeat() {
    let cases = cases();
    assert_eq!(cases.len(), 65);
    for case in cases {
        let caught = CaughtUpChunk::prepare(
            case.source.get().as_bytes(),
            key(&case.key),
            case.size,
            case.version,
            case.clock.get().as_bytes(),
            &case.occupied,
        )
        .unwrap_or_else(|e| panic!("{}: {e:?}", case.label));
        assert_eq!(caught.private_source_bytes(), case.source.get().as_bytes());
        assert_record(
            caught.private_record_bytes(),
            case.expected.get().as_bytes(),
            &case.label,
        );
        let repeat = CaughtUpChunk::prepare(
            caught.private_record_bytes(),
            key(&case.key),
            case.size,
            case.version,
            case.clock.get().as_bytes(),
            &case.occupied,
        )
        .unwrap();
        assert_record(
            repeat.private_record_bytes(),
            case.repeat.get().as_bytes(),
            &format!("{} repeat", case.label),
        );
    }
}
#[test]
fn precise_private_extensions_survive_without_numeric_coercion() {
    let case = cases().remove(0);
    let source = case.source.get().replace(
        "123456789012345678901234567890",
        "123456789012345678901234567890123456789012345678901234567890",
    );
    let caught = CaughtUpChunk::prepare(
        source.as_bytes(),
        key(&case.key),
        case.size,
        case.version,
        b"1.2500e2",
        &[],
    )
    .unwrap();
    let text = std::str::from_utf8(caught.private_record_bytes()).unwrap();
    assert!(text.contains("123456789012345678901234567890123456789012345678901234567890"));
    assert!(text.contains("\"clock\":1.2500e2"));
    assert!(text.contains("SYNTHETIC_SECRET"));
    assert_eq!(caught.private_source_bytes(), source.as_bytes());
}
#[test]
fn malformed_and_unsupported_inputs_reject_without_private_error_values() {
    let case = cases().remove(0);
    for (field, value) in [
        ("industry_version", json!(3)),
        ("mineral_version", json!(2)),
        ("terrain_version", json!(8)),
        ("clock", json!(-1)),
        ("clock", json!(9007199254740993_u64)),
    ] {
        let mut source: Value = serde_json::from_str(case.source.get()).unwrap();
        source[field] = value;
        let error = CaughtUpChunk::prepare(
            &serde_json::to_vec(&source).unwrap(),
            key(&case.key),
            case.size,
            case.version,
            b"0",
            &[],
        )
        .err()
        .unwrap();
        let message = format!("{error:?}");
        assert!(message.contains("$.chunk_catch_up"));
        assert!(!message.contains("SYNTHETIC_SECRET"));
    }
    let mut source: Value = serde_json::from_str(case.source.get()).unwrap();
    source["map_resources"][0]["kind"] = json!("SYNTHETIC_SECRET");
    assert!(
        CaughtUpChunk::prepare(
            &serde_json::to_vec(&source).unwrap(),
            key(&case.key),
            case.size,
            case.version,
            b"0",
            &[]
        )
        .is_err()
    );
    for clock in [
        b"9007199254740993".as_slice(),
        b"900719925474099300000000000000000",
        b"-1",
        b"null",
        b"true",
        b"1e999",
    ] {
        assert!(
            CaughtUpChunk::prepare(
                case.source.get().as_bytes(),
                key(&case.key),
                case.size,
                case.version,
                clock,
                &[]
            )
            .is_err()
        );
    }
    for clock in [b"9007199254740993.0".as_slice(), b"9007199254740993e0"] {
        // These are Python float literals: both already round to 2^53.
        assert!(
            CaughtUpChunk::prepare(
                case.source.get().as_bytes(),
                key(&case.key),
                case.size,
                case.version,
                clock,
                &[]
            )
            .is_ok()
        );
    }
    assert!(
        CaughtUpChunk::prepare(
            case.source.get().as_bytes(),
            ChunkKey { x: 48, y: 0 },
            1920,
            case.version,
            b"0",
            &[]
        )
        .is_err()
    );
}

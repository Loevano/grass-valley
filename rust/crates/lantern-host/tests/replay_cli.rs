use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn replay(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
        .arg("replay")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdio_fixture_is_repeatable_and_emits_only_one_response() {
    let input = include_str!("../../../../tests/replay/rust/transport_smoke_v1.json");
    let first = replay(input);
    let second = replay(input);
    assert!(first.status.success());
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let response: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        response["initial_state"],
        json!({"recipe": "transport_smoke_v1", "gameplay_ported": false})
    );
    assert_eq!(response["observations"].as_array().unwrap().len(), 2);
}

#[test]
fn unsupported_gameplay_and_invalid_input_have_distinct_nonzero_exits() {
    let input = include_str!("../../../../tests/replay/rust/transport_smoke_v1.json");
    let unsupported = replay(&input.replace("transport_smoke_v1", "meals_v1"));
    assert_eq!(unsupported.status.code(), Some(2));
    assert!(unsupported.stdout.is_empty());
    assert!(
        String::from_utf8(unsupported.stderr)
            .unwrap()
            .contains("gameplay is not ported")
    );
    let invalid = replay(r#"{"pin_hash":"never-log-this"}"#);
    assert_eq!(invalid.status.code(), Some(1));
    assert!(invalid.stdout.is_empty());
    assert!(
        !String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("never-log-this")
    );
}

#[test]
fn host_requires_explicit_replay_subcommand() {
    for args in [vec![], vec!["serve"], vec!["replay", "savegame.json"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_lantern-host"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
}

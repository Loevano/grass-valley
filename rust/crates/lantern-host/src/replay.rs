use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol_version: u64,
    trace_id: String,
    initial: Initial,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Initial {
    recipe: String,
    terrain: String,
    parameters: Parameters,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    seed: u64,
    setup_draws: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    operation: Operation,
    random_draws: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Advance {
        clock: String,
        seconds: f64,
    },
    Command {
        town: String,
        data: serde_json::Map<String, Value>,
    },
    Reload {},
}

impl Operation {
    fn valid(&self) -> bool {
        match self {
            Self::Advance { clock, seconds } => {
                ["movement", "economy", "adventure"].contains(&clock.as_str())
                    && seconds.is_finite()
                    && *seconds >= 0.0
            }
            Self::Command { town, data } => {
                !town.is_empty() && data.get("action").is_some_and(Value::is_string)
            }
            Self::Reload {} => true,
        }
    }

    fn supported(&self) -> bool {
        matches!(self, Self::Advance { clock, seconds } if clock == "movement" && *seconds == 0.0)
    }
}

fn public_json(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().all(|(key, value)| {
            ![
                "token",
                "tokens",
                "pin",
                "pin_hash",
                "password",
                "credentials",
                "player_accounts",
                "accounts",
                "authorization",
            ]
            .contains(&key.to_lowercase().as_str())
                && public_json(value)
        }),
        Value::Array(values) => values.iter().all(public_json),
        _ => true,
    }
}

fn valid_draw(value: &Value) -> bool {
    value.as_object().is_some_and(|draw| {
        draw.len() == 4
            && ["site", "method"].iter().all(|key| {
                draw.get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
            })
            && draw.get("args").is_some_and(Value::is_array)
            && draw.contains_key("value")
    })
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Invalid,
    Unsupported,
}

pub fn run(input: &str) -> Result<Value, Error> {
    let value: Value = serde_json::from_str(input).map_err(|_| Error::Invalid)?;
    if !public_json(&value) {
        return Err(Error::Invalid);
    }
    let request: Request = serde_json::from_value(value).map_err(|_| Error::Invalid)?;
    if request.protocol_version != 1
        || request.trace_id.is_empty()
        || request.initial.recipe.is_empty()
        || !["synthetic_flat", "real_generator"].contains(&request.initial.terrain.as_str())
        || request.steps.is_empty()
        || !request
            .initial
            .parameters
            .setup_draws
            .iter()
            .all(valid_draw)
        || !request
            .steps
            .iter()
            .all(|step| step.operation.valid() && step.random_draws.iter().all(valid_draw))
    {
        return Err(Error::Invalid);
    }
    if request.initial.recipe != "transport_smoke_v1"
        || request.initial.terrain != "synthetic_flat"
        || request.initial.parameters.seed != 0
        || !request.initial.parameters.setup_draws.is_empty()
        || !request
            .steps
            .iter()
            .all(|step| step.operation.supported() && step.random_draws.is_empty())
    {
        return Err(Error::Unsupported);
    }
    // Construct actual transport-only output; never receive/copy baseline expectations.
    let state = json!({"recipe": "transport_smoke_v1", "gameplay_ported": false});
    let observations: Vec<_> = request
        .steps
        .iter()
        .map(|_| json!({"public": {"result": null}, "state": state}))
        .collect();
    Ok(json!({"protocol_version": 1, "trace_id": request.trace_id,
              "initial_state": state, "observations": observations}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Value {
        serde_json::from_str(include_str!(
            "../../../../tests/replay/rust/transport_smoke_v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn fixture_runs_without_a_world_and_preserves_ordered_observation_count() {
        let request = request();
        let response = run(&request.to_string()).unwrap();
        assert_eq!(response["trace_id"], request["trace_id"]);
        assert_eq!(response["initial_state"]["gameplay_ported"], false);
        assert_eq!(response["observations"].as_array().unwrap().len(), 2);
        for observation in response["observations"].as_array().unwrap() {
            assert_eq!(observation["state"], response["initial_state"]);
            assert_eq!(observation["public"], json!({"result": null}));
        }
    }

    #[test]
    fn gameplay_time_commands_reload_and_draws_remain_unsupported() {
        let changes = [
            ("/initial/recipe", json!("meals_v1")),
            ("/initial/terrain", json!("real_generator")),
            ("/initial/parameters/seed", json!(1)),
            (
                "/steps/0/operation",
                json!({"kind": "advance", "clock": "movement", "seconds": 1}),
            ),
            ("/steps/0/operation", json!({"kind": "reload"})),
            (
                "/steps/0/operation",
                json!({"kind": "command", "town": "a", "data": {"action": "move"}}),
            ),
            (
                "/steps/0/random_draws",
                json!([{"site": "smoke", "method": "random", "args": [], "value": 0.5}]),
            ),
        ];
        for (path, value) in changes {
            let mut request = request();
            *request.pointer_mut(path).unwrap() = value;
            assert_eq!(run(&request.to_string()), Err(Error::Unsupported), "{path}");
        }
    }

    #[test]
    fn invalid_protocol_oracle_private_fields_and_malformed_operations_are_rejected() {
        for (path, value) in [
            ("/protocol_version", json!(true)),
            ("/protocol_version", json!(2)),
            ("/trace_id", json!("")),
            ("/steps", json!([])),
            ("/initial/parameters/seed", json!(-1)),
            (
                "/steps/0/operation",
                json!({"kind": "advance", "clock": "movement", "seconds": -1}),
            ),
            ("/steps/0/random_draws", json!([{"value": 1}])),
        ] {
            let mut request = request();
            *request.pointer_mut(path).unwrap() = value;
            assert_eq!(run(&request.to_string()), Err(Error::Invalid), "{path}");
        }
        for field in ["expected", "pin_hash"] {
            let mut request = request();
            request[field] = json!("never-copy-me");
            assert_eq!(run(&request.to_string()), Err(Error::Invalid));
        }
        for path in ["/initial/parameters", "/steps/0", "/steps/0/operation"] {
            let mut request = request();
            request.pointer_mut(path).unwrap()["expected"] = json!({});
            assert_eq!(run(&request.to_string()), Err(Error::Invalid), "{path}");
        }
        let mut request = request();
        request["steps"][0]["operation"] = json!({"kind": "reload", "unexpected": true});
        assert_eq!(run(&request.to_string()), Err(Error::Invalid));
        assert_eq!(run("not json"), Err(Error::Invalid));
        assert_eq!(run("{} {}"), Err(Error::Invalid));
    }
}

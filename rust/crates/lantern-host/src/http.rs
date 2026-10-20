//! Experimental R20 read-only HTTP boundary with explicit native coverage gates.
//!
//! Admitted native copies have checked read-only landscape/town presentations. In particular,
//! an offline model observation or preserved legacy archive is NOT a browser DTO.
//! Serializers own detached values; neither serialization nor network backpressure
//! holds an authoritative owner lock. Active-world adapters remain coverage gated.
use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};
mod read;
mod static_assets;
use crate::legacy::http_snapshot::PublicFrame;
use read::ReadError;
pub use read::ReadRegistry;

#[derive(Serialize)]
#[serde(untagged)]
enum Reply {
    Json(Value),
    Public(PublicFrame),
}
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::sync::Semaphore;
use tower_http::services::ServeFile;

pub const UNSUPPORTED: &str =
    "Experimental Rust HTTP lacks coverage for this state or operation. Use the Python host.";

#[derive(Clone)]
struct Boundary {
    web: PathBuf,
    favicon: PathBuf,
    started: Instant,
    serialization: Arc<Semaphore>,
    registry: Arc<ReadRegistry>,
}

/// No API accepts arbitrary saved JSON, credentials or client-provided town IDs.
/// Concurrency is bounded before spawn_blocking; there is no unbounded job queue.
pub fn bootstrap_router(web: PathBuf, favicon: PathBuf) -> Router {
    read_router(web, favicon, ReadRegistry::default())
}

/// Serves only state that passed native read-only coverage admission.
pub fn read_router(web: PathBuf, favicon: PathBuf, registry: ReadRegistry) -> Router {
    Router::new().fallback(dispatch).with_state(Boundary {
        web,
        favicon,
        started: Instant::now(),
        serialization: Arc::new(Semaphore::new(4)),
        registry: Arc::new(registry),
    })
}

fn json_body(status: StatusCode, body: Vec<u8>) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::CONTENT_LENGTH, body.len().to_string()),
        ],
        body,
    )
        .into_response()
}
fn error(status: StatusCode, message: &str) -> Response {
    json_body(
        status,
        serde_json::to_vec(&json!({"error": message})).expect("error JSON"),
    )
}
pub(crate) fn catalogs() -> &'static Value {
    static CATALOGS: OnceLock<Value> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        serde_json::from_str(include_str!("http_catalogs.json")).expect("reviewed HTTP catalogs")
    })
}

async fn dispatch(
    State(state): State<Boundary>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request,
) -> Response {
    // BaseHTTPRequestHandler collapses an initial // before route dispatch.
    if let Some(raw) = request
        .uri()
        .path_and_query()
        .filter(|p| p.as_str().starts_with("//"))
        && let Ok(uri) = format!("/{}", raw.as_str().trim_start_matches('/')).parse()
    {
        *request.uri_mut() = uri;
    }
    let path = request.uri().path();
    let raw = request
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or(path);
    if request.method() == Method::GET {
        // Python uses parsed paths for world/lanterns but exact raw paths for
        // players/worlds. Empty query separators are significant for the latter.
        let value: Box<dyn FnOnce() -> Result<Reply, ReadError> + Send> = match path {
            "/api/world" => {
                let query = match SnapshotQuery::parse(request.uri().query().unwrap_or("")) {
                    Ok(query) => query,
                    Err(message) => return error(StatusCode::BAD_REQUEST, message),
                };
                let sampled_at = state.started.elapsed().as_secs_f64();
                let registry = state.registry.clone();
                Box::new(move || {
                    if registry.active().is_some() {
                        return registry.frame(&query, sampled_at).map(Reply::Public);
                    }
                    Ok(Reply::Json(
                        json!({"towns": {}, "world_id": null, "name": null,
                    "players_sampled_at": sampled_at,
                    "simulation_clock": {"tick_seconds": 3, "next_tick_in": 3},
                    "hall_upgrades": catalogs()["hall_upgrades"],
                    "progression": crate::resources::public_catalog(),
                    "local_stories": catalogs()["local_stories"],
                    "quarry_catalog": catalogs()["quarry_catalog"], "hall_ready": {}}),
                    ))
                })
            }
            "/api/lanterns" => {
                let registry = state.registry.clone();
                Box::new(move || {
                    Ok(Reply::Json(
                        json!({"world_id": registry.active(), "quest": registry.lanterns()}),
                    ))
                })
            }
            "/api/players" if raw == path => {
                let sampled_at = state.started.elapsed().as_secs_f64();
                let registry = state.registry.clone();
                Box::new(move || {
                    Ok(Reply::Json(
                        json!({"world_id": registry.active(), "sampled_at": sampled_at, "players": registry.players()}),
                    ))
                })
            }
            "/api/worlds" if raw == path => {
                let can_manage = is_host(peer, request.headers());
                let registry = state.registry.clone();
                Box::new(move || Ok(Reply::Json(registry.worlds(can_manage))))
            }
            _ => return static_response(state, request).await,
        };
        return serialize(state.serialization, value).await;
    }
    if request.method() == Method::HEAD {
        // SimpleHTTPRequestHandler's HEAD never calls do_GET: API HEAD requests
        // use the static fallback, rather than exposing GET's dynamic payload.
        return static_response(state, request).await;
    }
    if request.method() == Method::POST {
        // No mutation, login, save import or activation is admitted in this slice.
        return error(StatusCode::NOT_IMPLEMENTED, UNSUPPORTED);
    }
    static_error(
        StatusCode::NOT_IMPLEMENTED,
        &format!("Unsupported method ('{}')", request.method()),
        "Server does not support this operation",
        false,
    )
}

async fn serialize(
    slots: Arc<Semaphore>,
    value: impl FnOnce() -> Result<Reply, ReadError> + Send + 'static,
) -> Response {
    let Ok(permit) = slots.try_acquire_owned() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Snapshot queue is full. Try again.",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        value().map(|value| serde_json::to_vec(&value))
    })
    .await
    {
        Ok(Ok(Ok(body))) => json_body(StatusCode::OK, body),
        Ok(Err(ReadError::Query(message))) => error(StatusCode::BAD_REQUEST, message),
        Ok(Err(ReadError::Coverage)) => error(StatusCode::NOT_IMPLEMENTED, UNSUPPORTED),
        _ => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Snapshot serialization failed.",
        ),
    }
}

fn static_error(status: StatusCode, message: &str, explain: &str, head: bool) -> Response {
    let message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    // The audited Python 3.9 handler formats HTTPStatus through %s in the
    // explanation (its integer formatting is used only for the first code).
    let explanation_code = match status {
        StatusCode::NOT_FOUND => "HTTPStatus.NOT_FOUND",
        StatusCode::NOT_IMPLEMENTED => "HTTPStatus.NOT_IMPLEMENTED",
        _ => unreachable!("only audited static error statuses"),
    };
    let body = format!(
        "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"\n        \"http://www.w3.org/TR/html4/strict.dtd\">\n<html>\n    <head>\n        <meta http-equiv=\"Content-Type\" content=\"text/html;charset=utf-8\">\n        <title>Error response</title>\n    </head>\n    <body>\n        <h1>Error response</h1>\n        <p>Error code: {}</p>\n        <p>Message: {message}.</p>\n        <p>Error code explanation: {} - {explain}.</p>\n    </body>\n</html>\n",
        status.as_u16(),
        explanation_code
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html;charset=utf-8".to_owned()),
            (header::CONTENT_LENGTH, body.len().to_string()),
        ],
        if head { String::new() } else { body },
    )
        .into_response()
}

async fn static_response(state: Boundary, request: Request) -> Response {
    static_assets::respond(state, request).await
}

/// Peer, Host and Origin are all required to agree. Forwarded headers never
/// participate. This is read-only can_manage metadata; mutations remain refused.
fn is_host(peer: SocketAddr, headers: &axum::http::HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(authority) = host.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    let hostname = authority
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let local = hostname.eq_ignore_ascii_case("localhost")
        || hostname
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    peer.ip().is_loopback()
        && local
        && headers.get(header::ORIGIN).is_none_or(|origin| {
            origin
                .to_str()
                .is_ok_and(|origin| origin == format!("http://{host}"))
        })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SnapshotQuery {
    pub world_id: Option<String>,
    pub chunks: Vec<String>,
    pub known: Vec<String>,
    pub resources: BTreeMap<String, String>,
}
impl SnapshotQuery {
    pub fn parse(query: &str) -> Result<Self, &'static str> {
        // parse_qs keeps the first nonempty duplicate value and ignores blanks.
        let mut fields = BTreeMap::new();
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            if !value.is_empty() {
                fields.entry(key.into_owned()).or_insert(value.into_owned());
            }
        }
        let get = |key: &str| fields.get(key).map(String::as_str).unwrap_or("");
        let chunks = parse_keys(get("chunks"), 49, "Too many map chunks.")?;
        let known = parse_keys(get("known"), 128, "Too many cached map chunks.")?;
        let items: Vec<_> = get("resources")
            .split(';')
            .filter(|_| !get("resources").is_empty())
            .collect();
        if items.len() > 128 {
            return Err("Too many cached resource chunks.");
        }
        let mut resources = BTreeMap::new();
        for item in items {
            let (key, version) = item
                .split_once(':')
                .ok_or("Invalid resource chunk version.")?;
            if version.len() != 16
                || !version
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("Invalid resource chunk version.");
            }
            let keys = parse_keys(key, 49, "Too many map chunks.")?;
            if keys.len() != 1 {
                return Err("Invalid resource chunk key.");
            }
            resources.insert(keys[0].clone(), version.into());
        }
        Ok(Self {
            world_id: fields.get("world_id").cloned(),
            chunks,
            known,
            resources,
        })
    }
}
fn parse_keys(
    value: &str,
    limit: usize,
    overflow: &'static str,
) -> Result<Vec<String>, &'static str> {
    let parts: Vec<_> = if value.is_empty() {
        vec![]
    } else {
        value.split(';').collect()
    };
    if parts.len() > limit {
        return Err(overflow);
    }
    let mut result = std::collections::BTreeSet::new();
    for key in parts {
        let coords: Vec<_> = key.split(',').collect();
        if coords.len() != 2 {
            return Err("Invalid map chunk.");
        }
        let mut pair = Vec::new();
        for coordinate in coords {
            // Python str.isdecimal accepts Unicode decimal digits, including
            // mixed scripts. Match only Nd digits, never numeric superscripts.
            let digits: Option<Vec<_>> = coordinate.chars().map(decimal_digit).collect();
            let Some(digits) = digits.filter(|d| !d.is_empty() && d.len() <= 2) else {
                return Err("Invalid map chunk.");
            };
            let number = digits.into_iter().fold(0, |n, d| n * 10 + d);
            pair.push(number);
        }
        if pair.iter().any(|number| *number >= 48) {
            return Err("Map chunk outside the world.");
        }
        result.insert(format!("{},{}", pair[0], pair[1]));
    }
    Ok(result.into_iter().collect())
}
fn decimal_digit(c: char) -> Option<u32> {
    // Python 3.9's Unicode 13 Nd zero points. Shared protocol tests detect drift
    // if the supported Python runtime upgrades its Unicode database.
    const ZEROS: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x16a60, 0x16b50,
        0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e950, 0x1fbf0,
    ];
    ZEROS
        .iter()
        .find_map(|zero| (c as u32).checked_sub(*zero).filter(|n| *n < 10))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_matches_python_cache_limits_and_canonicalization() {
        assert_eq!(
            SnapshotQuery::parse("chunks=０１,٢;1,2").unwrap().chunks,
            ["1,2"]
        );
        assert_eq!(
            SnapshotQuery::parse("chunks=&chunks=2,3&chunks=bad")
                .unwrap()
                .chunks,
            ["2,3"]
        );
        assert_eq!(
            SnapshotQuery::parse("chunks=48,0"),
            Err("Map chunk outside the world.")
        );
        assert_eq!(
            SnapshotQuery::parse("resources=:0123456789abcdef"),
            Err("Invalid resource chunk key.")
        );
        assert_eq!(
            SnapshotQuery::parse("chunks=²,0"),
            Err("Invalid map chunk.")
        );
        assert!(SnapshotQuery::parse(&format!("known={}", vec!["1,2"; 128].join(";"))).is_ok());
        assert_eq!(
            SnapshotQuery::parse(&format!("chunks={}", vec!["1,2"; 50].join(";"))),
            Err("Too many map chunks.")
        );
    }
    #[test]
    fn host_requires_peer_host_and_origin_and_ignores_forwarding() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::HOST, "localhost:8000".parse().unwrap());
        let local = "127.0.0.1:1234".parse().unwrap();
        assert!(is_host(local, &headers));
        assert!(!is_host("192.0.2.1:1234".parse().unwrap(), &headers));
        headers.insert("x-forwarded-host", "localhost".parse().unwrap());
        headers.insert(header::HOST, "attacker.example".parse().unwrap());
        assert!(!is_host(local, &headers));
        headers.insert(header::HOST, "[::1]:8000".parse().unwrap());
        assert!(is_host(local, &headers));
        headers.insert(header::ORIGIN, "http://evil.example".parse().unwrap());
        assert!(!is_host(local, &headers));
    }
    #[test]
    fn saturation_is_truthful_and_recovers() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let slots = Arc::new(Semaphore::new(1));
            let permit = slots.clone().try_acquire_owned().unwrap();
            assert_eq!(
                serialize(slots.clone(), || Ok(Reply::Json(json!({}))))
                    .await
                    .status(),
                StatusCode::SERVICE_UNAVAILABLE
            );
            drop(permit);
            assert_eq!(
                serialize(slots, || Ok(Reply::Json(json!({}))))
                    .await
                    .status(),
                StatusCode::OK
            );
        });
    }

    #[test]
    fn slow_and_failed_serialization_release_capacity_without_blocking_runtime() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let slots = Arc::new(Semaphore::new(2));
            let (entered_tx, entered_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let slow = tokio::spawn(serialize(slots.clone(), move || {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
                Ok(Reply::Json(json!({})))
            }));
            entered_rx
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap();
            assert_eq!(
                serialize(slots.clone(), || Ok(Reply::Json(json!({}))))
                    .await
                    .status(),
                StatusCode::OK
            );
            let failed = serialize(slots.clone(), || panic!("synthetic serializer failure")).await;
            assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(slots.available_permits(), 1);
            release_tx.send(()).unwrap();
            assert_eq!(slow.await.unwrap().status(), StatusCode::OK);
            assert_eq!(slots.available_permits(), 2);
        });
    }
}

//! Python SimpleHTTPRequestHandler path, index and listing contracts. Files stream
//! through ServeFile; directory work runs away from Tokio's executor threads.
use super::*;
use std::path::{Path, PathBuf};

fn unquote(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut input = value.as_bytes().iter().copied().peekable();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let mut look = input.clone();
            if let (Some(a), Some(b)) = (look.next(), look.next())
                && let (Some(a), Some(b)) = ((a as char).to_digit(16), (b as char).to_digit(16))
            {
                bytes.push((a * 16 + b) as u8);
                input = look;
                continue;
            }
        }
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn quote(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"_.-~/".contains(&byte) {
            result.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(result, "%{byte:02X}").unwrap();
        }
    }
    result
}
fn translated(root: &Path, uri: &str) -> PathBuf {
    let decoded = unquote(uri.split(['?', '#']).next().unwrap_or(""));
    let mut components = Vec::new();
    for word in decoded.split('/') {
        match word {
            "" | "." => (),
            ".." => {
                components.pop();
            }
            _ => components.push(word),
        }
    }
    let mut path = root.to_path_buf();
    path.extend(components);
    path
}
enum Asset {
    File(PathBuf),
    Redirect(String),
    Listing(Vec<u8>),
    Missing(&'static str),
}
fn locate(root: &Path, uri: &str) -> Asset {
    let path = translated(root, uri);
    if path.is_dir() {
        let (url_path, query) = uri.split_once('?').unwrap_or((uri, ""));
        if !url_path.ends_with('/') {
            return Asset::Redirect(format!(
                "{url_path}/{}",
                if uri.contains('?') {
                    format!("?{query}")
                } else {
                    String::new()
                }
            ));
        }
        for index in ["index.html", "index.htm"] {
            let index = path.join(index);
            if index.exists() {
                return Asset::File(index);
            }
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            return Asset::Missing("No permission to list directory");
        };
        let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(_) => return Asset::Missing("No permission to list directory"),
        };
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
        let title = format!("Directory listing for {}", escape(&unquote(uri)));
        let mut lines = vec![
            "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\" \"http://www.w3.org/TR/html4/strict.dtd\">".into(),
            "<html>\n<head>".into(),
            "<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">".into(),
            format!("<title>{title}</title>\n</head>"),
            format!("<body>\n<h1>{title}</h1>"), "<hr>\n<ul>".into(),
        ];
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let directory = entry.path().is_dir();
            let link = format!("{name}{}", if directory { "/" } else { "" });
            let display = format!(
                "{name}{}",
                if entry.path().is_symlink() {
                    "@"
                } else if directory {
                    "/"
                } else {
                    ""
                }
            );
            lines.push(format!(
                "<li><a href=\"{}\">{}</a></li>",
                quote(&link),
                escape(&display)
            ));
        }
        lines.push("</ul>\n<hr>\n</body>\n</html>\n".into());
        return Asset::Listing(lines.join("\n").into_bytes());
    }
    if uri
        .split('?')
        .next()
        .unwrap_or("")
        .trim_end()
        .ends_with('/')
    {
        return Asset::Missing("File not found");
    }
    // ServeFile doesn't interpret the URI; this path has already been normalized.
    if !path.is_file() {
        return Asset::Missing("File not found");
    }
    Asset::File(path)
}
pub(super) async fn respond(state: Boundary, mut request: Request) -> Response {
    let head = request.method() == Method::HEAD;
    let uri = request
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/")
        .to_owned();
    let asset = if request.uri().path() == "/favicon.ico" {
        Asset::File(state.favicon)
    } else {
        let Ok(permit) = state.serialization.clone().try_acquire_owned() else {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Snapshot queue is full. Try again.",
            );
        };
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            locate(&state.web, &uri)
        })
        .await
        {
            Ok(asset) => asset,
            Err(_) => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to read static asset.",
                );
            }
        }
    };
    match asset {
        Asset::Missing(message) => static_error(
            StatusCode::NOT_FOUND,
            message,
            "Nothing matches the given URI",
            head,
        ),
        Asset::Redirect(location) => (
            StatusCode::MOVED_PERMANENTLY,
            [
                (header::LOCATION, location),
                (header::CONTENT_LENGTH, "0".into()),
            ],
            Body::empty(),
        )
            .into_response(),
        Asset::Listing(body) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8".into()),
                (header::CONTENT_LENGTH, body.len().to_string()),
            ],
            if head { vec![] } else { body },
        )
            .into_response(),
        Asset::File(path) => {
            request.headers_mut().remove(header::RANGE);
            request.headers_mut().remove(header::IF_RANGE);
            request.headers_mut().remove(header::IF_MATCH);
            request.headers_mut().remove(header::IF_UNMODIFIED_SINCE);
            // Python ignores If-Modified-Since when any If-None-Match is present.
            if request.headers().contains_key(header::IF_NONE_MATCH) {
                request.headers_mut().remove(header::IF_MODIFIED_SINCE);
                request.headers_mut().remove(header::IF_NONE_MATCH);
            }
            // mime_guess differs from the audited Python MIME database here.
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let mime = match extension.as_str() {
                "mjs" => Some("text/javascript"),
                "woff" => Some("font/woff"),
                _ => None,
            };
            match ServeFile::new(path).try_call(request).await {
                Ok(response) if response.status() == StatusCode::NOT_FOUND => static_error(
                    StatusCode::NOT_FOUND,
                    "File not found",
                    "Nothing matches the given URI",
                    head,
                ),
                Ok(mut response) => {
                    if response.status() == StatusCode::OK
                        && let Some(mime) = mime
                    {
                        response
                            .headers_mut()
                            .insert(header::CONTENT_TYPE, mime.parse().unwrap());
                    }
                    if response.status() == StatusCode::NOT_MODIFIED {
                        response.headers_mut().remove(header::CONTENT_TYPE);
                        response.headers_mut().remove(header::CONTENT_LENGTH);
                        response.headers_mut().remove(header::LAST_MODIFIED);
                    }
                    response.headers_mut().remove(header::ACCEPT_RANGES);
                    response.map(Body::new)
                }
                Err(_) => error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to read static asset.",
                ),
            }
        }
    }
}

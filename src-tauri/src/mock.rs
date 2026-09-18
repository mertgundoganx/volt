//! A mock server, on this machine.
//!
//! It serves the responses already kept under `.examples/`: point a client at
//! `http://127.0.0.1:<port>` and it answers with what the real API answered
//! when the example was taken. That makes a saved example worth keeping twice
//! over — it documents the API and it stands in for it.
//!
//! There is no hosted URL and no account, because there is no volt service to
//! host one. The mock runs while volt runs, on the loopback address only.
//!
//! The HTTP is hand-rolled on tokio rather than pulling in a server framework:
//! what a mock needs is one request line, some headers, and a response, and
//! that is a page of code against a dependency measured in crates.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use crate::collection::{self, Node};
use crate::error::{Error, Result};
use crate::examples::{self, Example};
use crate::model::Request;

/// One thing the mock can answer: a method, the path it matches, and the
/// example to send back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// The request this came from, for the UI to point at.
    pub id: String,
    pub name: String,
    pub method: String,
    /// The path, with `{{variables}}` left as `*` segments.
    pub path: String,
    pub example: String,
    pub status: u16,
    /// What a call has to carry as well as the path — `status=active`,
    /// `plan=pro` — so the dialog can say why two routes are not the same.
    pub conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub port: u16,
    pub routes: Vec<Route>,
}

struct Route_ {
    method: String,
    /// Split on `/`; `None` is a segment that came from a variable and so
    /// matches anything.
    segments: Vec<Option<String>>,
    /// Query parameters the request declares with a literal value. A request
    /// that asks for `?status=paid` only answers a call that asks the same.
    query: Vec<(String, String)>,
    /// Top-level fields of a JSON body with a literal value, for telling two
    /// calls to the same path apart.
    body: Vec<(String, String)>,
    example: Example,
    route: Route,
}

#[derive(Default)]
pub struct Server {
    inner: Mutex<Option<Handle>>,
}

struct Handle {
    port: u16,
    routes: Vec<Route>,
    stop: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    pub fn running(&self) -> Option<Running> {
        let inner = self.inner.lock().expect("mock server");
        inner.as_ref().map(|h| Running { port: h.port, routes: h.routes.clone() })
    }

    pub fn stop(&self) {
        let mut inner = self.inner.lock().expect("mock server");
        if let Some(handle) = inner.take() {
            // The receiver going away is just as good as a message arriving.
            let _ = handle.stop.send(());
        }
    }
}

/// Every request in the collection that has at least one saved example.
///
/// The first example wins when a request has several; the others are still
/// reachable by name, which is how you ask for the 404 on purpose.
fn routes(root: &Path) -> Result<Vec<Route_>> {
    let collection = collection::load(root)?;
    let mut out = Vec::new();
    walk(root, &collection.tree, &mut out)?;
    // Longer paths first, so `/users/42` is not swallowed by `/users/*`, and
    // within one shape the route that asks for more wins.
    out.sort_by_key(|route| {
        std::cmp::Reverse((route.segments.len(), route.query.len() + route.body.len()))
    });
    Ok(out)
}

fn walk(root: &Path, nodes: &[Node], out: &mut Vec<Route_>) -> Result<()> {
    for node in nodes {
        match node {
            Node::Folder { children, .. } => walk(root, children, out)?,
            Node::Request { id, name, .. } => {
                let examples = examples::list(root, id)?;
                if examples.is_empty() {
                    continue;
                }
                let request = collection::read_request(root, id)?;
                for example in examples {
                    out.push(route_of(id, name, &request, example));
                }
            }
        }
    }
    Ok(())
}

fn route_of(id: &str, name: &str, request: &Request, example: Example) -> Route_ {
    let method = request.method.trim().to_uppercase();
    let segments = path_segments(&request.url);
    let shown = segments
        .iter()
        .map(|segment| segment.clone().unwrap_or_else(|| "*".into()))
        .collect::<Vec<_>>()
        .join("/");

    let query = literal_query(request);
    let body = literal_body(request);

    Route_ {
        method: method.clone(),
        segments,
        route: Route {
            id: id.to_string(),
            name: name.to_string(),
            method,
            path: format!("/{shown}"),
            example: example.name.clone(),
            status: example.status,
            conditions: query
                .iter()
                .chain(&body)
                .map(|(name, value)| format!("{name}={value}"))
                .collect(),
        },
        query,
        body,
        example,
    }
}

/// The query parameters a request declares with a value of its own, from the
/// URL and from the params list alike. A `{{variable}}` matches anything, so it
/// is not a condition.
fn literal_query(request: &Request) -> Vec<(String, String)> {
    let inline = request.url.split_once('?').map(|(_, query)| query).unwrap_or("");
    let from_url = inline.split('&').filter_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        Some((name.to_string(), value.to_string()))
    });

    from_url
        .chain(
            request
                .params
                .iter()
                .filter(|param| param.enabled)
                .map(|param| (param.name.clone(), param.value.clone())),
        )
        .filter(|(name, value)| !name.is_empty() && !name.contains("{{") && !value.contains("{{"))
        .collect()
}

/// Top-level fields of a JSON body, for telling two calls to the same path
/// apart. Nested fields are deliberately not read: a matcher that walks a whole
/// document is one nobody can predict.
fn literal_body(request: &Request) -> Vec<(String, String)> {
    let crate::model::Body::Json { content } = &request.body else { return Vec::new() };
    let Ok(serde_json::Value::Object(fields)) = serde_json::from_str::<serde_json::Value>(content) else {
        return Vec::new();
    };

    fields
        .into_iter()
        .filter_map(|(name, value)| {
            let text = match value {
                serde_json::Value::String(text) => text,
                serde_json::Value::Null | serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                    return None
                }
                other => other.to_string(),
            };
            (!text.contains("{{")).then_some((name, text))
        })
        .collect()
}

/// The path part of a URL, with any segment holding a `{{variable}}` turned
/// into a wildcard. `{{baseUrl}}/users/{{id}}` → `users`, `*`.
fn path_segments(url: &str) -> Vec<Option<String>> {
    let without_query = url.split(['?', '#']).next().unwrap_or("").trim();
    // Drop scheme and host, which for `{{baseUrl}}/x` is the variable itself.
    let path = match without_query.find("://") {
        Some(at) => without_query[at + 3..].split_once('/').map(|(_, rest)| rest).unwrap_or(""),
        None => without_query
            .strip_prefix("{{")
            .and_then(|rest| rest.split_once("}}"))
            .map(|(_, rest)| rest)
            .unwrap_or(without_query),
    };

    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| if segment.contains("{{") { None } else { Some(segment.to_string()) })
        .collect()
}

/// A route answers a call when the method and the shape of the path agree, and
/// when every literal the request declared is also in the call — query first,
/// then top-level JSON body fields. That is as far as the matching goes: every
/// rule beyond it is one somebody has to reason about when a mock answers the
/// wrong thing, and a matching language is a product of its own.
fn matches(route: &Route_, method: &str, path: &str, query: &[(String, String)], body: Option<&serde_json::Value>) -> bool {
    if !route.method.eq_ignore_ascii_case(method) {
        return false;
    }
    let asked: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if asked.len() != route.segments.len() {
        return false;
    }
    let path_fits = route
        .segments
        .iter()
        .zip(&asked)
        .all(|(segment, given)| segment.as_deref().is_none_or(|want| want == *given));
    if !path_fits {
        return false;
    }

    let query_fits = route
        .query
        .iter()
        .all(|(name, want)| query.iter().any(|(given, value)| given == name && value == want));
    if !query_fits {
        return false;
    }

    route.body.iter().all(|(name, want)| {
        body.and_then(|body| body.get(name)).is_some_and(|given| match given {
            serde_json::Value::String(text) => text == want,
            serde_json::Value::Bool(yes) => want == if *yes { "true" } else { "false" },
            serde_json::Value::Number(number) => number.to_string() == want.as_str(),
            _ => false,
        })
    })
}

/// Start serving on `port`, or on a port the OS picks when it is 0.
pub async fn start(server: &Server, root: PathBuf, port: u16) -> Result<Running> {
    server.stop();

    let built = routes(&root)?;
    if built.is_empty() {
        return Err(Error::Invalid(
            "nothing to serve yet: keep a response as an example first, and the mock will answer with it".into(),
        ));
    }

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(address).await.map_err(|e| {
        Error::Invalid(format!("could not listen on port {port}: {e}"))
    })?;
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);

    let shown: Vec<Route> = built.iter().map(|route| route.route.clone()).collect();
    let table = Arc::new(built);
    let (stop, mut stopped) = tokio::sync::oneshot::channel();

    // A client that connects and never speaks would otherwise pin a task and a
    // socket for as long as the app runs, and nothing stops it opening more.
    const READ_TIMEOUT: Duration = Duration::from_secs(15);
    const MAX_CONNECTIONS: usize = 64;
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let table = table.clone();
                    let Ok(slot) = slots.clone().try_acquire_owned() else {
                        // Refusing is better than queueing: the caller finds out
                        // now rather than hanging on a socket that will not be
                        // read for minutes.
                        drop(stream);
                        continue;
                    };
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(READ_TIMEOUT, serve(stream, &table)).await;
                        drop(slot);
                    });
                }
            }
        }
    });

    *server.inner.lock().expect("mock server") =
        Some(Handle { port, routes: shown.clone(), stop });
    Ok(Running { port, routes: shown })
}

async fn serve(mut stream: TcpStream, table: &[Route_]) {
    let Some((method, path, headers, body_text)) = read_request(&mut stream).await else {
        let _ = respond(&mut stream, 400, "Bad Request", &[], b"could not read the request").await;
        return;
    };

    // A page served from this machine may ask; anything else is answered
    // without the CORS header, so the browser will not hand it over.
    if method.eq_ignore_ascii_case("OPTIONS") {
        let _ = respond(&mut stream, 204, "No Content", &cors(headers.get("origin")), b"").await;
        return;
    }

    // `?example=Name` asks for one by name, which is how the 404 is reached.
    let (path, raw_query) = match path.split_once('?') {
        Some((path, query)) => (path, query),
        None => (path.as_str(), ""),
    };
    let wanted = example_from(raw_query);
    let query: Vec<(String, String)> = url::form_urlencoded::parse(raw_query.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    let json: Option<serde_json::Value> = serde_json::from_str(&body_text).ok();

    let hit = table
        .iter()
        .filter(|route| matches(route, &method, path, &query, json.as_ref()))
        .find(|route| wanted.as_deref().is_none_or(|name| route.example.name == name));

    match hit {
        Some(route) => {
            let mut out = cors(headers.get("origin"));
            for header in &route.example.headers {
                // The mock decides its own framing; copying these would lie.
                let name = header.name.to_ascii_lowercase();
                if name == "content-length" || name == "transfer-encoding" || name == "connection" {
                    continue;
                }
                out.push((header.name.clone(), header.value.clone()));
            }
            let body = if route.example.body_is_base64 {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD.decode(&route.example.body).unwrap_or_default()
            } else {
                route.example.body.clone().into_bytes()
            };
            let status_text = route.example.status_text.clone();
            let _ = respond(&mut stream, route.example.status, &status_text, &out, &body).await;
        }
        None => {
            let known: Vec<String> =
                table.iter().map(|r| format!("{} {}", r.route.method, r.route.path)).collect();
            let body = format!(
                "volt mock: nothing kept for {method} {path}.\n\nWhat is served:\n{}\n",
                known.join("\n")
            );
            let _ = respond(&mut stream, 404, "Not Found", &cors(headers.get("origin")), body.as_bytes()).await;
        }
    }
}

/// The headers every answer carries.
///
/// `Access-Control-Allow-Origin` is echoed back only to a loopback origin. A
/// wildcard would let any page open in the user's browser read the responses
/// they kept as examples — which are their API's, on a predictable port — and
/// a mock exists to be poked at from this machine, not from the web.
fn cors(origin: Option<&String>) -> Vec<(String, String)> {
    let mut out = vec![
        ("Access-Control-Allow-Headers".into(), "*".into()),
        ("Access-Control-Allow-Methods".into(), "*".into()),
        ("X-Volt-Mock".into(), "1".into()),
    ];
    if let Some(origin) = origin.filter(|origin| is_local_origin(origin)) {
        out.push(("Access-Control-Allow-Origin".into(), origin.clone()));
        // The answer differs by origin, so a cache must not reuse it.
        out.push(("Vary".into(), "Origin".into()));
    }
    out
}

/// `http://localhost:5173` yes, `https://evil.example` no.
fn is_local_origin(origin: &str) -> bool {
    let host = origin.split_once("://").map(|(_, rest)| rest).unwrap_or(origin);
    let host = match host.strip_prefix('[') {
        // An IPv6 literal keeps its brackets; the port is after them.
        Some(rest) => rest.split_once(']').map(|(inside, _)| inside).unwrap_or(rest),
        None => host.split(':').next().unwrap_or(host),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// A header name as RFC 7230 allows it, and a value with nothing in it that
/// could end the line.
fn header_is_safe(name: &str, value: &str) -> bool {
    let token = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
    !name.is_empty()
        && name.chars().all(token)
        && !value.bytes().any(|b| b < 0x20 || b == 0x7f)
}
fn example_from(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "example").then(|| value.replace('+', " "))
    })
}

/// Read the request line, the headers, and as much of the body as arrived with
/// them. The body is only read to tell two calls to the same path apart.
async fn read_request(stream: &mut TcpStream) -> Option<(String, String, HashMap<String, String>, String)> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    let head = loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = find_blank_line(&buffer) {
            break String::from_utf8_lossy(&buffer[..at]).into_owned();
        }
        // A request head this long is not one a mock should try to serve.
        if buffer.len() > 64 * 1024 {
            return None;
        }
    };

    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();

    let headers: HashMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();

    // Whatever arrived after the blank line is the start of the body. Read on
    // only as far as Content-Length promises: a mock must never block waiting
    // for a body nobody said was coming.
    let at = find_blank_line(&buffer)?;
    let start = at + if buffer[at..].starts_with(b"\r\n\r\n") { 4 } else { 2 };
    let mut body = buffer[start.min(buffer.len())..].to_vec();
    let want: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .map_or(0, |length: usize| length.min(1024 * 1024));
    while body.len() < want {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    if want > 0 {
        body.truncate(want);
    }

    Some((method, path, headers, String::from_utf8_lossy(&body).into_owned()))
}

fn find_blank_line(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n").or_else(|| buffer.windows(2).position(|w| w == b"\n\n"))
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    status_text: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> std::io::Result<()> {
    // Everything here comes from a file in the collection, and a `.examples/`
    // file is whatever some server sent. A CR or LF in a header value would
    // end the head early and let the rest of it be read as a second response.
    let status_text: String = status_text
        .chars()
        .map(|c| if (c as u32) < 0x20 || c == '\u{7f}' { ' ' } else { c })
        .collect();

    let mut head = format!("HTTP/1.1 {status} {status_text}\r\n");
    for (name, value) in headers {
        if !header_is_safe(name, value) {
            continue;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()));

    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    let _ = stream.shutdown().await;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_becomes_a_path_with_variables_as_wildcards() {
        let shown = |url: &str| {
            path_segments(url)
                .iter()
                .map(|s| s.clone().unwrap_or_else(|| "*".into()))
                .collect::<Vec<_>>()
                .join("/")
        };

        assert_eq!(shown("{{baseUrl}}/users"), "users");
        assert_eq!(shown("{{baseUrl}}/users/{{id}}/orders"), "users/*/orders");
        assert_eq!(shown("https://api.test/v2/users?page=1"), "v2/users");
        assert_eq!(shown("{{baseUrl}}/"), "");
    }

    fn route_with(method: &str, url: &str, params: &[(&str, &str)], json: &str) -> Route_ {
        let request = Request {
            method: method.into(),
            url: url.into(),
            params: params
                .iter()
                .map(|(name, value)| crate::model::KeyValue {
                    name: (*name).into(),
                    value: (*value).into(),
                    enabled: true,
                    description: None,
                })
                .collect(),
            body: if json.is_empty() {
                crate::model::Body::default()
            } else {
                crate::model::Body::Json { content: json.into() }
            },
            ..Default::default()
        };
        let example = Example {
            name: "ok".into(),
            at: 0,
            status: 200,
            status_text: "OK".into(),
            headers: Vec::new(),
            body: String::new(),
            body_is_base64: false,
        };
        route_of("x.yaml", "X", &request, example)
    }

    fn route(method: &str, url: &str) -> Route_ {
        route_with(method, url, &[], "")
    }

    fn hits(route: &Route_, method: &str, path: &str) -> bool {
        matches(route, method, path, &[], None)
    }

    #[test]
    fn a_path_matches_on_shape_not_on_the_value_in_a_variable() {
        let users = route("GET", "{{baseUrl}}/users/{{id}}");

        assert!(hits(&users, "GET", "/users/42"));
        assert!(hits(&users, "get", "/users/anything"), "the method is not case sensitive");
        assert!(!hits(&users, "POST", "/users/42"), "another method is another route");
        assert!(!hits(&users, "GET", "/users"), "and a shorter path is a different one");
        assert!(!hits(&users, "GET", "/users/42/orders"));
    }

    #[test]
    fn a_query_the_request_declares_has_to_be_in_the_call() {
        let active = route_with("GET", "{{baseUrl}}/users", &[("status", "active"), ("page", "{{page}}")], "");
        assert_eq!(active.query, vec![("status".to_string(), "active".to_string())], "a variable is not a condition");

        let pair = |name: &str, value: &str| (name.to_string(), value.to_string());
        assert!(matches(&active, "GET", "/users", &[pair("status", "active")], None));
        assert!(
            matches(&active, "GET", "/users", &[pair("status", "active"), pair("page", "2")], None),
            "extra parameters are the caller's business"
        );
        assert!(!matches(&active, "GET", "/users", &[pair("status", "banned")], None));
        assert!(!matches(&active, "GET", "/users", &[], None), "and leaving it out is not a match");

        // A request that asks for nothing still answers anything.
        let any = route("GET", "{{baseUrl}}/users");
        assert!(matches(&any, "GET", "/users", &[pair("status", "banned")], None));
    }

    #[test]
    fn a_json_field_the_request_declares_has_to_be_in_the_body() {
        let signup =
            route_with("POST", "{{baseUrl}}/users", &[], "{\"plan\":\"pro\",\"name\":\"{{name}}\",\"seats\":3}");
        assert_eq!(signup.body.len(), 2, "a variable is not a condition, a number is");

        let body = |text: &str| serde_json::from_str::<serde_json::Value>(text).unwrap();
        assert!(matches(&signup, "POST", "/users", &[], Some(&body("{\"plan\":\"pro\",\"seats\":3}"))));
        assert!(!matches(&signup, "POST", "/users", &[], Some(&body("{\"plan\":\"free\",\"seats\":3}"))));
        assert!(!matches(&signup, "POST", "/users", &[], None), "no body cannot satisfy a field");
    }

    #[test]
    fn the_route_that_asks_for_more_is_tried_first() {
        let mut table = [
            route("GET", "{{baseUrl}}/users"),
            route_with("GET", "{{baseUrl}}/users", &[("status", "active")], ""),
        ];
        table.sort_by_key(|route| {
            std::cmp::Reverse((route.segments.len(), route.query.len() + route.body.len()))
        });
        assert_eq!(table[0].query.len(), 1);
    }


    #[test]
    fn an_example_can_be_asked_for_by_name() {
        assert_eq!(example_from("example=Not+found").as_deref(), Some("Not found"));
        assert_eq!(example_from("page=2").as_deref(), None);
    }

    /// The whole thing, over a real socket, with a real client.
    #[tokio::test]
    async fn the_mock_answers_with_what_was_kept() {
        let root = std::env::temp_dir().join(format!("volt-mock-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("users")).unwrap();
        std::fs::write(root.join("collection.yaml"), "name: Mock\nversion: 1\n").unwrap();
        std::fs::write(
            root.join("users/get.yaml"),
            "name: Get user\nseq: 1\nmethod: GET\nurl: '{{baseUrl}}/users/{{id}}'\n",
        )
        .unwrap();

        let kept = crate::http::HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![crate::model::KeyValue {
                name: "content-type".into(),
                value: "application/json".into(),
                enabled: true,
                description: None,
            }],
            body: "{\"id\":42}".into(),
            body_is_base64: false,
            size_bytes: 9,
            duration_ms: 1,
            time_to_first_byte_ms: 1,
            final_url: String::new(),
            sent_url: String::new(),
            sent_method: "GET".into(),
            sent_headers: Vec::new(),
            sent_body_bytes: 0,
            redirects: Vec::new(),
            version: "HTTP/1.1".into(),
            missing_vars: Vec::new(),
        };
        examples::save(&root, "users/get.yaml", "Happy path", &kept, &[]).unwrap();

        let server = Server::default();
        let running = start(&server, root.clone(), 0).await.unwrap();
        assert_eq!(running.routes.len(), 1);
        assert_eq!(running.routes[0].path, "/users/*");

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://127.0.0.1:{}/users/42", running.port))
            .send()
            .await
            .expect("the mock should answer");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "application/json");
        assert_eq!(response.text().await.unwrap(), "{\"id\":42}");

        // A path with nothing kept for it says what is served instead.
        let missing = client
            .get(format!("http://127.0.0.1:{}/nope", running.port))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);
        assert!(missing.text().await.unwrap().contains("GET /users/*"));

        server.stop();
        std::fs::remove_dir_all(&root).ok();
    }

    /// Two requests on one path, told apart by what the call carries.
    #[tokio::test]
    async fn two_calls_to_one_path_are_told_apart() {
        let root = std::env::temp_dir().join(format!("volt-mock-match-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("collection.yaml"), "name: Mock\nversion: 1\n").unwrap();
        std::fs::write(
            root.join("pro.yaml"),
            "name: Pro signup\nseq: 1\nmethod: POST\nurl: '{{baseUrl}}/signup'\nbody:\n  type: json\n  content: '{\"plan\": \"pro\"}'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("free.yaml"),
            "name: Free signup\nseq: 2\nmethod: POST\nurl: '{{baseUrl}}/signup'\nbody:\n  type: json\n  content: '{\"plan\": \"free\"}'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("search.yaml"),
            "name: Active users\nseq: 3\nmethod: GET\nurl: '{{baseUrl}}/users'\nparams:\n  - name: status\n    value: active\n    enabled: true\n",
        )
        .unwrap();

        let kept = |body: &str| crate::http::HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: Vec::new(),
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: 1,
            time_to_first_byte_ms: 1,
            final_url: String::new(),
            sent_url: String::new(),
            sent_method: "POST".into(),
            sent_headers: Vec::new(),
            sent_body_bytes: 0,
            redirects: Vec::new(),
            version: "HTTP/1.1".into(),
            missing_vars: Vec::new(),
        };
        examples::save(&root, "pro.yaml", "Kept", &kept("paid"), &[]).unwrap();
        examples::save(&root, "free.yaml", "Kept", &kept("gratis"), &[]).unwrap();
        examples::save(&root, "search.yaml", "Kept", &kept("actives"), &[]).unwrap();

        let server = Server::default();
        let running = start(&server, root.clone(), 0).await.unwrap();
        let base = format!("http://127.0.0.1:{}", running.port);
        let client = reqwest::Client::new();

        let answer = |body: &'static str| {
            let client = client.clone();
            let url = format!("{base}/signup");
            async move {
                client
                    .post(url)
                    .header("content-type", "application/json")
                    .body(body)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            }
        };

        assert_eq!(answer("{\"plan\":\"pro\",\"email\":\"a@b.c\"}").await, "paid");
        assert_eq!(answer("{\"plan\":\"free\"}").await, "gratis");
        assert!(
            answer("{\"plan\":\"enterprise\"}").await.contains("nothing kept"),
            "a body nothing declared is a 404, not a coin toss"
        );

        let query = |suffix: &'static str| {
            let client = client.clone();
            let url = format!("{base}/users{suffix}");
            async move { client.get(url).send().await.unwrap().text().await.unwrap() }
        };
        assert_eq!(query("?status=active").await, "actives");
        assert!(query("?status=banned").await.contains("nothing kept"));

        server.stop();
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn only_a_page_from_this_machine_is_allowed_to_read_the_answers() {
        let allowed = |origin: &str| {
            cors(Some(&origin.to_string()))
                .iter()
                .any(|(name, value)| name == "Access-Control-Allow-Origin" && value == origin)
        };

        assert!(allowed("http://localhost:5173"));
        assert!(allowed("http://127.0.0.1:3000"));
        assert!(allowed("http://[::1]:8080"));

        // The mock serves the user's own API responses on a predictable port,
        // so a wildcard would hand them to any tab they happen to have open.
        assert!(!allowed("https://evil.example"));
        assert!(!allowed("https://localhost.evil.example"));
        assert!(!allowed("null"));
        assert!(
            !cors(None).iter().any(|(name, _)| name == "Access-Control-Allow-Origin"),
            "no Origin means no header at all, not a wildcard"
        );
    }

    #[test]
    fn a_header_that_would_end_the_head_early_is_dropped() {
        // `.examples/` holds whatever some server sent, so this is reachable
        // from a real response that was kept.
        assert!(!header_is_safe("X-Evil", "ok\r\nContent-Length: 0\r\n\r\nHTTP/1.1 200 OK"));
        assert!(!header_is_safe("X-Evil", "ok\nSet-Cookie: a=b"));
        assert!(!header_is_safe("bad name", "ok"));
        assert!(!header_is_safe("", "ok"));
        assert!(header_is_safe("Content-Type", "application/json; charset=utf-8"));
        assert!(header_is_safe("X-Rate-Limit", "100"));
    }
}

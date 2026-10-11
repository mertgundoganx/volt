//! HAR import: what a browser recorded, as requests worth keeping.
//!
//! A HAR file is a recording, not a collection, and most of a recording is
//! not what anybody wants: the stylesheet, forty images, the same poll every
//! five seconds. So this is a translation with opinions, like the OpenAPI one:
//!
//! - Static assets are left out — by what the browser said the resource was,
//!   by what the server said it sent, or by the extension — and counted.
//! - The same method, address and body twice is one request.
//! - What is left is grouped into a folder per host and named for what it
//!   does: `GET /users/42`.
//! - Headers a client sets for itself (`Host`, `Content-Length`, `Connection`,
//!   `Accept-Encoding`) and HTTP/2's pseudo-headers are dropped; the ones only
//!   a browser sends are kept switched off, as a pasted curl command's are.
//! - Cookies are left out and the report says so. A recorded `Cookie` header
//!   is somebody's live session, and volt keeps cookies in a jar of its own.
//!
//! Whatever credentials remain — an `Authorization` header, an `api_key` in
//! the query — go through the same move into secret variables every other
//! import does, in `import::finish`.

use std::collections::HashSet;

use serde_json::Value;

use crate::import::{split_query, text_body, ImportNode, Imported, Warnings};
use crate::model::{Auth, Body, FormField, KeyValue, Kind, Request};

pub fn looks_like(value: &Value) -> bool {
    value.pointer("/log/entries").is_some_and(Value::is_array)
}

fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or("")
}

pub fn parse(value: &Value) -> Imported {
    let mut warnings = Warnings::default();
    let entries = value.pointer("/log/entries").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);

    // In the order hosts were first seen, which is the order of the recording.
    let mut hosts: Vec<(String, Vec<ImportNode>)> = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut assets = 0usize;
    let mut repeats = 0usize;

    for entry in entries {
        let recorded = &entry["request"];
        let method = match text(recorded, "/method").trim() {
            "" => "GET".to_string(),
            method => method.to_uppercase(),
        };
        let address = text(recorded, "/url").trim();
        if address.is_empty() {
            continue;
        }
        let (base, params) = split_query(address);
        let (host, path) = host_and_path(&base);
        let name = name_for(&method, &path);

        if is_socket(entry, address) {
            warnings.add("WebSocket connections in the recording were not imported", &name);
            continue;
        }
        if is_asset(entry, &method, &path) {
            assets += 1;
            continue;
        }

        let mut headers = Vec::new();
        let mut had_cookies = !arr(recorded, "cookies").is_empty();
        for header in arr(recorded, "headers") {
            let header_name = text(header, "/name").trim();
            let lower = header_name.to_ascii_lowercase();
            if header_name.is_empty() || lower.starts_with(':') || SET_BY_THE_CLIENT.contains(&lower.as_str()) {
                continue;
            }
            if lower == "cookie" {
                had_cookies = true;
                continue;
            }
            let browser_only = lower.starts_with("sec-fetch-") || lower.starts_with("sec-ch-") || lower == "priority";
            headers.push(KeyValue {
                name: header_name.to_string(),
                value: text(header, "/value").to_string(),
                enabled: !browser_only,
                description: None,
            });
        }
        if had_cookies {
            warnings.add("Cookies were left out: a recorded session is a credential, and volt keeps cookies in its own jar", &name);
        }

        let body = body_of(&recorded["postData"], &mut headers, &name, &mut warnings);

        // The same thing asked twice is one request; a poll recorded forty
        // times is not forty things to keep.
        let fingerprint = serde_json::to_string(&body).unwrap_or_default();
        if !seen.insert((method.clone(), address.to_string(), fingerprint)) {
            repeats += 1;
            continue;
        }

        let request = Request {
            name,
            kind: Kind::Http,
            seq: 1,
            method,
            url: base,
            headers,
            params,
            body,
            // A recording has no notion of auth beyond the headers it saw.
            auth: Auth::None,
            ..Default::default()
        };
        match hosts.iter_mut().find(|(known, _)| *known == host) {
            Some((_, requests)) => requests.push(ImportNode::Request(Box::new(request))),
            None => hosts.push((host, vec![ImportNode::Request(Box::new(request))])),
        }
    }

    if assets > 0 {
        warnings.add(
            format!("{assets} static asset{} (images, fonts, stylesheets, scripts) {} skipped", plural(assets), was(assets)),
            "",
        );
    }
    if repeats > 0 {
        warnings.add(
            format!("{repeats} repeat{} of a request already imported {} left out", plural(repeats), was(repeats)),
            "",
        );
    }

    let name = match text(value, "/log/pages/0/title").trim() {
        "" => "Imported HAR".to_string(),
        // A page title is often the address itself, and a folder cannot be
        // called `https://…`; the collection's directory name is made safe
        // elsewhere, but the display name should read as a name too.
        title => title.chars().take(80).collect(),
    };
    let nodes = hosts
        .into_iter()
        .map(|(host, children)| ImportNode::Folder { name: host, auth: Auth::Inherit, headers: Vec::new(), vars: Vec::new(), children })
        .collect();

    Imported {
        format: "HAR",
        name,
        headers: Vec::new(),
        auth: Auth::None,
        vars: Vec::new(),
        nodes,
        environments: Vec::new(),
        warnings,
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn was(count: usize) -> &'static str {
    if count == 1 { "was" } else { "were" }
}

fn arr<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// Headers the sending side works out for itself. Carrying a recorded
/// `Content-Length` onto an edited body is how a request comes to hang.
const SET_BY_THE_CLIENT: [&str; 4] = ["host", "content-length", "connection", "accept-encoding"];

/// `https://api.test:8443/users/42` → (`api.test:8443`, `/users/42`).
fn host_and_path(base: &str) -> (String, String) {
    let rest = base.split_once("://").map_or(base, |(_, rest)| rest);
    let (host, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    // Credentials in the address are not part of what the host is called.
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    (if host.is_empty() { "unknown host".to_string() } else { host.to_string() }, path.to_string())
}

/// `GET /users/42`, cut short when the path runs on.
fn name_for(method: &str, path: &str) -> String {
    let path = match path.trim_end_matches('/') {
        "" => "/",
        trimmed => trimmed,
    };
    let name = format!("{method} {path}");
    match name.chars().count() > 70 {
        true => format!("{}…", name.chars().take(69).collect::<String>()),
        false => name,
    }
}

fn is_socket(entry: &Value, address: &str) -> bool {
    text(entry, "/_resourceType").eq_ignore_ascii_case("websocket")
        || address.starts_with("ws://")
        || address.starts_with("wss://")
        || entry.get("_webSocketMessages").is_some()
}

/// A page's furniture rather than its API: decided by what the browser called
/// the resource, what the server said it was, or failing both the extension.
/// Only for a GET — nobody POSTs to a stylesheet, and a POST that answers
/// with an image is an API that makes images.
fn is_asset(entry: &Value, method: &str, path: &str) -> bool {
    if !matches!(method, "GET" | "HEAD") {
        return false;
    }
    let kind = text(entry, "/_resourceType").to_ascii_lowercase();
    if matches!(kind.as_str(), "stylesheet" | "script" | "image" | "font" | "media" | "manifest") {
        return true;
    }
    // What the browser called a call to an API is one, whatever came back.
    if matches!(kind.as_str(), "xhr" | "fetch") {
        return false;
    }
    let mime = text(entry, "/response/content/mimeType").to_ascii_lowercase();
    let mime = mime.split(';').next().unwrap_or_default().trim();
    if mime.starts_with("image/")
        || mime.starts_with("font/")
        || mime.starts_with("audio/")
        || mime.starts_with("video/")
        || mime == "text/css"
        || mime.ends_with("javascript")
        || mime.ends_with("ecmascript")
        || mime.contains("font-")
    {
        return true;
    }
    const EXTENSIONS: [&str; 24] = [
        "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "avif", "bmp", "css", "js", "mjs", "map", "woff", "woff2",
        "ttf", "otf", "eot", "mp3", "mp4", "webm", "ogg", "wasm", "webmanifest",
    ];
    let file = path.rsplit('/').next().unwrap_or_default();
    file.rsplit_once('.').is_some_and(|(_, extension)| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// The recorded body, as the kind of body volt would have sent it as.
fn body_of(data: &Value, headers: &mut Vec<KeyValue>, subject: &str, warnings: &mut Warnings) -> Body {
    if !data.is_object() {
        return Body::None;
    }
    let mime = text(data, "/mimeType").to_string();
    let lower = mime.to_ascii_lowercase();
    let content = text(data, "/text");

    if text(data, "/encoding").eq_ignore_ascii_case("base64") {
        warnings.add("A binary body was recorded as base64 and was not imported", subject);
        return Body::None;
    }

    if lower.contains("x-www-form-urlencoded") {
        // The text is the truth. `params` repeats it, and browsers disagree on
        // whether the values in it are still percent-encoded.
        let fields: Vec<KeyValue> = match content.is_empty() {
            false => url::form_urlencoded::parse(content.as_bytes())
                .map(|(name, value)| pair(&name, &value))
                .collect(),
            true => arr(data, "params").iter().map(|p| pair(text(p, "/name"), text(p, "/value"))).collect(),
        };
        return Body::UrlEncoded { fields };
    }

    if lower.contains("multipart/form-data") {
        // The recorded Content-Type names the recorded boundary. volt writes
        // its own, so the header has to go or the two will disagree.
        headers.retain(|header| !header.name.eq_ignore_ascii_case("content-type"));

        let listed = arr(data, "params");
        let (fields, files) = match listed.is_empty() {
            false => {
                let mut fields = Vec::new();
                let mut files = 0;
                for param in listed {
                    match param.get("fileName").is_some_and(|name| !name.is_null()) {
                        true => files += 1,
                        false => fields.push(form_field(text(param, "/name"), text(param, "/value"))),
                    }
                }
                (fields, files)
            }
            // Chrome records the raw body and no list, so it is read here.
            true => multipart_fields(&mime, content),
        };
        if files > 0 {
            warnings.add("File fields in a multipart body were left out: the recording does not keep the file", subject);
        }
        return Body::Form { fields };
    }

    if content.is_empty() {
        return Body::None;
    }
    text_body(content.to_string(), &lower, headers)
}

fn pair(name: &str, value: &str) -> KeyValue {
    KeyValue { name: name.to_string(), value: value.to_string(), enabled: true, description: None }
}

fn form_field(name: &str, value: &str) -> FormField {
    FormField { name: name.to_string(), value: value.to_string(), enabled: true, description: None, file: false }
}

/// The most parts of one multipart body that are read. A form has a handful;
/// a body with thousands is not a form anybody will edit by hand.
const MAX_PARTS: usize = 500;

/// The text fields of a raw multipart body, and how many file parts there
/// were beside them. Only what a form needs: the boundary from the
/// Content-Type, each part's `name`, and the value for parts that are not
/// files.
fn multipart_fields(mime: &str, content: &str) -> (Vec<FormField>, usize) {
    let boundary = mime
        .split(';')
        .filter_map(|piece| piece.trim().split_once('='))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case("boundary"))
        .map(|(_, value)| value.trim().trim_matches('"').to_string());
    let Some(boundary) = boundary.filter(|boundary| !boundary.is_empty()) else {
        return (Vec::new(), 0);
    };

    let mut fields = Vec::new();
    let mut files = 0;
    for part in content.split(&format!("--{boundary}")).skip(1).take(MAX_PARTS) {
        // `--` straight after a boundary is the end of the body.
        if part.starts_with("--") {
            break;
        }
        let part = part.strip_prefix("\r\n").or_else(|| part.strip_prefix('\n')).unwrap_or(part);
        let Some((head, value)) = part.split_once("\r\n\r\n").or_else(|| part.split_once("\n\n")) else {
            continue;
        };
        let disposition = head
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("content-disposition:"))
            .unwrap_or_default();
        let attribute = |wanted: &str| {
            disposition.split(';').skip(1).find_map(|piece| {
                let (key, value) = piece.trim().split_once('=')?;
                key.trim().eq_ignore_ascii_case(wanted).then(|| value.trim().trim_matches('"').to_string())
            })
        };
        let Some(name) = attribute("name") else { continue };
        if attribute("filename").is_some() {
            files += 1;
            continue;
        }
        let value = value.strip_suffix("\r\n").or_else(|| value.strip_suffix('\n')).unwrap_or(value);
        fields.push(form_field(&name, value));
    }
    (fields, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import;

    const RECORDING: &str = r##"{
      "log": {
        "version": "1.2",
        "creator": { "name": "WebInspector", "version": "537.36" },
        "pages": [{ "id": "page_1", "title": "Shop — orders" }],
        "entries": [
          { "_resourceType": "document",
            "request": { "method": "GET", "url": "https://shop.test/orders", "headers": [
                { "name": ":authority", "value": "shop.test" }, { "name": ":method", "value": "GET" },
                { "name": "accept", "value": "text/html" }, { "name": "cookie", "value": "session=abc123" },
                { "name": "sec-fetch-mode", "value": "navigate" }, { "name": "accept-encoding", "value": "gzip, br, zstd" }
              ], "queryString": [], "cookies": [{ "name": "session", "value": "abc123" }] },
            "response": { "status": 200, "content": { "mimeType": "text/html" } } },
          { "_resourceType": "stylesheet",
            "request": { "method": "GET", "url": "https://cdn.shop.test/app.css", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "text/css" } } },
          { "request": { "method": "GET", "url": "https://cdn.shop.test/logo", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "image/png; charset=binary" } } },
          { "request": { "method": "GET", "url": "https://cdn.shop.test/bundle.min.js?v=3", "headers": [] },
            "response": { "status": 304, "content": {} } },
          { "_resourceType": "font",
            "request": { "method": "GET", "url": "https://cdn.shop.test/plex.woff2", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "font/woff2" } } },
          { "_resourceType": "xhr",
            "request": { "method": "GET", "url": "https://api.shop.test/v1/orders?page=2&api_key=live_k3y_123", "headers": [
                { "name": "Host", "value": "api.shop.test" }, { "name": "Connection", "value": "keep-alive" },
                { "name": "Authorization", "value": "Bearer eyJ.recorded.token" },
                { "name": "Accept", "value": "application/json" }
              ], "queryString": [{ "name": "page", "value": "2" }, { "name": "api_key", "value": "live_k3y_123" }] },
            "response": { "status": 200, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "xhr",
            "request": { "method": "GET", "url": "https://api.shop.test/v1/orders?page=2&api_key=live_k3y_123", "headers": [] },
            "response": { "status": 200, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "fetch",
            "request": { "method": "POST", "url": "https://api.shop.test/v1/orders", "headers": [
                { "name": "content-type", "value": "application/json" }, { "name": "content-length", "value": "19" }
              ], "postData": { "mimeType": "application/json", "text": "{\"sku\": \"A-1\"}" } },
            "response": { "status": 201, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "fetch",
            "request": { "method": "POST", "url": "https://api.shop.test/v1/orders", "headers": [],
                         "postData": { "mimeType": "application/json", "text": "{\"sku\": \"B-2\"}" } },
            "response": { "status": 201, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "xhr",
            "request": { "method": "POST", "url": "https://api.shop.test/login", "headers": [
                { "name": "Content-Type", "value": "application/x-www-form-urlencoded" }
              ], "postData": { "mimeType": "application/x-www-form-urlencoded; charset=UTF-8",
                               "text": "user=ada%40example.test&password=hunter2+secret",
                               "params": [{ "name": "user", "value": "ada%40example.test" }, { "name": "password", "value": "hunter2+secret" }] } },
            "response": { "status": 200, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "xhr",
            "request": { "method": "POST", "url": "https://api.shop.test/v1/avatar", "headers": [
                { "name": "Content-Type", "value": "multipart/form-data; boundary=----X" }
              ], "postData": { "mimeType": "multipart/form-data; boundary=----X",
                "text": "------X\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\nme, on a boat\r\n------X\r\nContent-Disposition: form-data; name=\"file\"; filename=\"me.png\"\r\nContent-Type: image/png\r\n\r\n\r\n------X--\r\n" } },
            "response": { "status": 200, "content": { "mimeType": "application/json" } } },
          { "_resourceType": "xhr",
            "request": { "method": "PUT", "url": "https://api.shop.test/v1/notes/7", "headers": [],
                         "postData": { "mimeType": "text/plain", "text": "remember the milk" } },
            "response": { "status": 204, "content": {} } },
          { "_resourceType": "websocket",
            "request": { "method": "GET", "url": "wss://api.shop.test/live", "headers": [] },
            "response": { "status": 101, "content": {} } }
        ]
      }
    }"##;

    fn requests<'a>(imported: &'a Imported, host: &str) -> Vec<&'a Request> {
        imported
            .nodes
            .iter()
            .find_map(|node| match node {
                ImportNode::Folder { name, children, .. } if name == host => Some(children),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no folder for {host}"))
            .iter()
            .map(|node| match node {
                ImportNode::Request(request) => &**request,
                ImportNode::Folder { .. } => panic!("hosts hold requests, not folders"),
            })
            .collect()
    }

    fn header<'a>(request: &'a Request, name: &str) -> Option<&'a KeyValue> {
        request.headers.iter().find(|header| header.name.eq_ignore_ascii_case(name))
    }

    #[test]
    fn a_recording_becomes_a_folder_per_host_with_requests_named_for_what_they_do() {
        let imported = import::parse(RECORDING).expect("a HAR file is recognised");
        assert_eq!(imported.format, "HAR");
        assert_eq!(imported.name, "Shop — orders");

        let hosts: Vec<&str> = imported
            .nodes
            .iter()
            .map(|node| match node {
                ImportNode::Folder { name, .. } => name.as_str(),
                ImportNode::Request(_) => panic!("every request sits under its host"),
            })
            .collect();
        assert_eq!(hosts, ["shop.test", "api.shop.test"], "in the order they were first seen; the CDN left nothing");

        let api: Vec<&str> = requests(&imported, "api.shop.test").iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            api,
            ["GET /v1/orders", "POST /v1/orders", "POST /v1/orders", "POST /login", "POST /v1/avatar", "PUT /v1/notes/7"]
        );
    }

    #[test]
    fn static_assets_and_repeats_are_left_out_and_counted() {
        let imported = import::parse(RECORDING).unwrap();
        let said = imported.warnings.render();
        assert!(said.iter().any(|line| line == "4 static assets (images, fonts, stylesheets, scripts) were skipped"), "{said:?}");
        assert!(said.iter().any(|line| line == "1 repeat of a request already imported was left out"), "{said:?}");
        assert!(said.iter().any(|line| line.contains("WebSocket") && line.contains("GET /live")), "{said:?}");

        // Two POSTs to one address with different bodies are two requests.
        let api = requests(&imported, "api.shop.test");
        assert_eq!(api.iter().filter(|r| r.name == "POST /v1/orders").count(), 2);
    }

    #[test]
    fn headers_keep_what_was_meant_and_drop_what_a_client_sets_itself() {
        let imported = import::parse(RECORDING).unwrap();

        let page = requests(&imported, "shop.test")[0];
        assert!(page.headers.iter().all(|h| !h.name.starts_with(':')), "pseudo-headers are HTTP/2's, not ours");
        assert!(header(page, "cookie").is_none() && header(page, "accept-encoding").is_none());
        assert!(header(page, "accept").is_some_and(|h| h.enabled));
        assert!(header(page, "sec-fetch-mode").is_some_and(|h| !h.enabled), "browser-only headers are kept, switched off");
        assert!(
            imported.warnings.render().iter().any(|line| line.starts_with("Cookies were left out") && line.ends_with("GET /orders")),
            "{:?}",
            imported.warnings.render()
        );

        let list = requests(&imported, "api.shop.test")[0];
        assert!(header(list, "host").is_none() && header(list, "connection").is_none());
        let create = requests(&imported, "api.shop.test")[1];
        assert!(header(create, "content-length").is_none(), "a recorded length would be wrong after the first edit");
    }

    #[test]
    fn the_query_and_each_kind_of_body_arrive_as_volt_would_write_them() {
        let imported = import::parse(RECORDING).unwrap();
        let api = requests(&imported, "api.shop.test");

        assert_eq!(api[0].url, "https://api.shop.test/v1/orders", "the query moves into params");
        assert_eq!(api[0].params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["page", "api_key"]);

        assert!(matches!(&api[1].body, Body::Json { content } if content == "{\"sku\": \"A-1\"}"), "{:?}", api[1].body);

        let Body::UrlEncoded { fields } = &api[3].body else { panic!("{:?}", api[3].body) };
        assert_eq!(fields[0].value, "ada@example.test", "decoded from the text, whatever `params` said");

        let Body::Form { fields } = &api[4].body else { panic!("{:?}", api[4].body) };
        assert_eq!(fields.len(), 1, "the text field is read out of the raw body; the file is not: {fields:?}");
        assert_eq!((fields[0].name.as_str(), fields[0].value.as_str()), ("caption", "me, on a boat"));
        assert!(header(api[4], "content-type").is_none(), "the recorded boundary would not match the one volt writes");
        assert!(imported.warnings.render().iter().any(|line| line.contains("File fields in a multipart body")));

        assert!(matches!(&api[5].body, Body::Text { content } if content == "remember the milk"));
    }

    #[test]
    fn recorded_credentials_go_to_secret_variables_like_any_other_import() {
        let imported = import::parse(RECORDING).unwrap();
        let api = requests(&imported, "api.shop.test");
        let env = &imported.environments[0];
        let secret = |name: &str| {
            env.vars.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no {name} in {:?}", env.vars))
        };

        assert_eq!(header(api[0], "authorization").unwrap().value, "Bearer {{token}}");
        assert_eq!(secret("token").value, "eyJ.recorded.token");
        assert_eq!(api[0].params[1].value, "{{apiKey}}");
        assert!(secret("apiKey").secret);

        let Body::UrlEncoded { fields } = &api[3].body else { panic!() };
        assert_eq!(fields[1].value, "{{password}}");
        assert_eq!(secret("password").value, "hunter2 secret");

        // Nothing that will be written as YAML still holds one of them.
        for request in api {
            let yaml = serde_yaml_ng::to_string(request).unwrap();
            for value in ["eyJ.recorded.token", "live_k3y_123", "hunter2", "abc123"] {
                assert!(!yaml.contains(value), "`{value}` reached the YAML of {}", request.name);
            }
        }
    }

    #[test]
    fn an_asset_is_told_by_what_it_is_and_an_api_call_is_not_mistaken_for_one() {
        let entry = |kind: &str, mime: &str| {
            serde_json::json!({ "_resourceType": kind, "response": { "content": { "mimeType": mime } } })
        };
        assert!(is_asset(&entry("image", ""), "GET", "/avatar/7"));
        assert!(is_asset(&entry("", "application/javascript; charset=utf-8"), "GET", "/app"));
        assert!(is_asset(&entry("", ""), "GET", "/static/Logo.PNG"), "by extension, whatever its case");
        assert!(!is_asset(&entry("xhr", "image/png"), "GET", "/v1/chart.png"), "the browser said it was a call");
        assert!(!is_asset(&entry("", "image/png"), "POST", "/v1/render"), "a POST that answers with an image is an API");
        assert!(!is_asset(&entry("", "application/json"), "GET", "/v1/users.json"));
        assert!(!is_asset(&entry("", ""), "GET", "/v1/users"));
    }

    #[test]
    fn a_recording_with_nothing_in_it_and_a_body_with_no_boundary_do_not_break_anything() {
        let empty = import::parse(r#"{"log": {"entries": []}}"#).unwrap();
        assert_eq!(empty.name, "Imported HAR");
        assert!(empty.nodes.is_empty());

        assert_eq!(multipart_fields("multipart/form-data", "whatever").0.len(), 0);
        let (fields, files) = multipart_fields("multipart/form-data; boundary=\"b\"", "--b\nContent-Disposition: form-data; name=a\n\n1\n--b--");
        assert_eq!((fields.len(), files), (1, 0));
        assert_eq!(fields[0].value, "1", "bare newlines and an unquoted name are read too");

        let odd = import::parse(r#"{"log": {"entries": [{"request": {"url": "notaurl"}}, {"request": {}}, 7]}}"#).unwrap();
        let ImportNode::Folder { name, children, .. } = &odd.nodes[0] else { panic!() };
        assert_eq!((name.as_str(), children.len()), ("notaurl", 1), "odd entries import as best they can or not at all");
    }
}

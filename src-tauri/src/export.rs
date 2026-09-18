//! Writing a collection out as Postman v2.1.
//!
//! The mirror of `import.rs`, and it exists for one reason: a collection you
//! cannot hand to a colleague who uses Postman is a collection you cannot
//! share. Everything else about volt says the files are yours; this is the
//! part that makes that true across tools.
//!
//! Secrets never travel. Requests keep their `{{name}}` placeholders, exactly
//! as they are on disk, and environments are not exported at all — the
//! receiving side fills its own in.

use serde_json::{json, Map, Value};

use crate::collection::{self, Collection, Node};
use crate::error::Result;
use crate::model::{ApiKeyLocation, Auth, Body, CollectionMeta, FolderMeta, KeyValue, Request};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub json: String,
    /// What did not survive the trip, said plainly rather than dropped.
    pub notes: Vec<String>,
}

pub fn postman(root: &std::path::Path, collection: &Collection) -> Result<Exported> {
    let mut notes = Vec::new();
    let meta: CollectionMeta = collection.meta.clone();

    let items: Vec<Value> =
        collection.tree.iter().map(|node| item(root, node, &mut notes)).collect::<Result<_>>()?;

    let mut out = Map::new();
    out.insert(
        "info".into(),
        json!({
            "name": meta.name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
            "description": "Exported from volt",
        }),
    );
    out.insert("item".into(), Value::Array(items));
    if !meta.headers.is_empty() {
        // Postman has no collection-wide headers, only folder and request ones.
        notes.push(format!(
            "The collection's {} shared header(s) were copied onto every request, since Postman has no collection-level headers.",
            meta.headers.len()
        ));
    }
    if let Some(auth) = auth(&meta.auth) {
        out.insert("auth".into(), auth);
    }
    if !meta.vars.is_empty() {
        out.insert("variable".into(), Value::Array(meta.vars.iter().map(variable).collect()));
    }
    if !collection.environments.is_empty() {
        notes.push(
            "Environments were not exported: their secret values live outside the collection and stay on this machine."
                .into(),
        );
    }

    let json = serde_json::to_string_pretty(&Value::Object(out))
        .map_err(|e| crate::error::Error::Invalid(format!("could not write the export: {e}")))?;
    Ok(Exported { json, notes })
}

fn item(root: &std::path::Path, node: &Node, notes: &mut Vec<String>) -> Result<Value> {
    match node {
        Node::Folder { id, name, .. } => {
            let meta: FolderMeta = collection::folder_meta(root, id)?;
            let children: Vec<Value> = match node {
                Node::Folder { children, .. } => {
                    children.iter().map(|child| item(root, child, notes)).collect::<Result<_>>()?
                }
                _ => unreachable!(),
            };
            let mut folder = Map::new();
            folder.insert("name".into(), json!(name));
            folder.insert("item".into(), Value::Array(children));
            if let Some(auth) = auth(&meta.auth) {
                folder.insert("auth".into(), auth);
            }
            if !meta.vars.is_empty() {
                folder.insert("variable".into(), Value::Array(meta.vars.iter().map(variable).collect()));
            }
            if !meta.headers.is_empty() {
                notes.push(format!(
                    "`{name}` has folder headers; Postman applies headers per request, so they were copied down."
                ));
            }
            Ok(Value::Object(folder))
        }
        Node::Request { id, .. } => {
            let request = collection::read_request(root, id)?;
            Ok(json!({ "name": request.name, "request": postman_request(root, id, &request, notes)? }))
        }
    }
}

fn variable(pair: &KeyValue) -> Value {
    json!({ "key": pair.name, "value": pair.value, "disabled": !pair.enabled })
}

fn postman_request(
    root: &std::path::Path,
    id: &str,
    request: &Request,
    notes: &mut Vec<String>,
) -> Result<Value> {
    // Headers and auth are flattened the way Postman expects them: it has no
    // collection-level headers, and inheritance only reaches one folder deep.
    let scopes = collection::scopes(root, Some(id))?;
    let mut headers: Vec<&KeyValue> = scopes.headers().collect();
    headers.extend(request.headers.iter());

    let mut seen: Vec<String> = Vec::new();
    let mut header_json: Vec<Value> = Vec::new();
    // Later wins, as when sending, so walk backwards and keep the first seen.
    for header in headers.iter().rev() {
        let lower = header.name.to_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        seen.push(lower);
        header_json.push(json!({
            "key": header.name,
            "value": header.value,
            "disabled": !header.enabled,
            "description": header.description.clone().unwrap_or_default(),
        }));
    }
    header_json.reverse();

    let mut out = Map::new();
    out.insert("method".into(), json!(request.method.to_uppercase()));
    out.insert("header".into(), Value::Array(header_json));
    out.insert("url".into(), url(request));
    if let Some(docs) = request.docs.as_deref().filter(|d| !d.trim().is_empty()) {
        out.insert("description".into(), json!(docs));
    }

    let effective = match &request.auth {
        Auth::Inherit => scopes.auth().clone(),
        other => other.clone(),
    };
    if let Some(auth) = auth(&effective) {
        out.insert("auth".into(), auth);
    }
    if let Some(body) = body(&request.body, notes, &request.name) {
        out.insert("body".into(), body);
    }
    if !request.captures.is_empty() {
        notes.push(format!(
            "`{}` captures values from its response; Postman would need a test script for that, which was not written.",
            request.name
        ));
    }
    Ok(Value::Object(out))
}

/// Postman wants the query split out as well as the raw string.
fn url(request: &Request) -> Value {
    let raw = request.url.trim();
    let query: Vec<Value> = request
        .params
        .iter()
        .map(|p| json!({ "key": p.name, "value": p.value, "disabled": !p.enabled }))
        .collect();

    let (base, inline) = raw.split_once('?').unwrap_or((raw, ""));
    let mut all = query;
    for pair in inline.split('&').filter(|s| !s.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        all.push(json!({ "key": key, "value": value }));
    }

    let mut url = Map::new();
    url.insert("raw".into(), json!(raw));
    if !all.is_empty() {
        url.insert("query".into(), Value::Array(all));
    }
    // `host` as a single piece: splitting `{{baseUrl}}/users` into host and
    // path is guesswork, and Postman reads `raw` first anyway.
    url.insert("host".into(), json!([base]));
    Value::Object(url)
}

fn auth(auth: &Auth) -> Option<Value> {
    match auth {
        Auth::Inherit => None,
        Auth::None => Some(json!({ "type": "noauth" })),
        Auth::Bearer { token } => Some(json!({
            "type": "bearer",
            "bearer": [{ "key": "token", "value": token, "type": "string" }],
        })),
        Auth::Basic { username, password } => Some(json!({
            "type": "basic",
            "basic": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" },
            ],
        })),
        // Postman has no place for these two, so they are said in the report
        // rather than turned into something that would not work.
        Auth::Digest { username, password } => Some(json!({
            "type": "digest",
            "digest": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" },
            ],
        })),
        Auth::Ntlm { username, password, domain } => Some(json!({
            "type": "ntlm",
            "ntlm": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" },
                { "key": "domain", "value": domain, "type": "string" },
            ],
        })),
        Auth::AwsSigV4 { key_id, secret, region, service, session_token } => Some(json!({
            "type": "awsv4",
            "awsv4": [
                { "key": "accessKey", "value": key_id, "type": "string" },
                { "key": "secretKey", "value": secret, "type": "string" },
                { "key": "region", "value": region, "type": "string" },
                { "key": "service", "value": service, "type": "string" },
                { "key": "sessionToken", "value": session_token, "type": "string" },
            ],
        })),
        Auth::ApiKey { key, value, location } => Some(json!({
            "type": "apikey",
            "apikey": [
                { "key": "key", "value": key, "type": "string" },
                { "key": "value", "value": value, "type": "string" },
                { "key": "in", "value": match location { ApiKeyLocation::Query => "query", ApiKeyLocation::Header => "header" }, "type": "string" },
            ],
        })),
    }
}

fn body(body: &Body, notes: &mut Vec<String>, name: &str) -> Option<Value> {
    let raw = |content: &String, language: &str| {
        json!({ "mode": "raw", "raw": content, "options": { "raw": { "language": language } } })
    };
    match body {
        Body::None => None,
        Body::Text { content } => Some(raw(content, "text")),
        Body::Json { content } => Some(raw(content, "json")),
        Body::Xml { content } => Some(raw(content, "xml")),
        Body::UrlEncoded { fields } => Some(json!({
            "mode": "urlencoded",
            "urlencoded": fields.iter().map(|f| json!({ "key": f.name, "value": f.value, "disabled": !f.enabled })).collect::<Vec<_>>(),
        })),
        Body::Form { fields } => Some(json!({
            "mode": "formdata",
            "formdata": fields.iter().map(|f| {
                if f.file {
                    json!({ "key": f.name, "src": f.value, "type": "file", "disabled": !f.enabled })
                } else {
                    json!({ "key": f.name, "value": f.value, "type": "text", "disabled": !f.enabled })
                }
            }).collect::<Vec<_>>(),
        })),
        // Postman has its own gRPC requests, in a shape this export cannot
        // produce; the report says so rather than writing something broken.
        Body::Grpc { method, message, .. } => Some(json!({
            "mode": "raw",
            "raw": message,
            "options": { "raw": { "language": "json" } },
            "description": format!("gRPC call to {method}"),
        })),
        Body::GraphQl { query, variables } => Some(json!({
            "mode": "graphql",
            "graphql": { "query": query, "variables": variables },
        })),
        Body::Binary { path } => {
            notes.push(format!("`{name}` sends a file; its path only makes sense on this machine."));
            Some(json!({ "mode": "file", "file": { "src": path } }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sample-collection")
    }

    fn exported() -> Value {
        let root = sample();
        let out = postman(&root, &collection::load(&root).unwrap()).unwrap();
        serde_json::from_str(&out.json).expect("the export is JSON")
    }

    fn find<'a>(items: &'a Value, name: &str) -> Option<&'a Value> {
        items.as_array()?.iter().find(|item| item["name"] == name)
    }

    #[test]
    fn the_example_collection_exports_as_a_v21_collection() {
        let out = exported();
        assert_eq!(out["info"]["name"], "Sample API");
        assert!(out["info"]["schema"].as_str().unwrap().contains("v2.1.0"));

        let users = find(&out["item"], "Users").expect("the folder is there");
        let request = find(&users["item"], "List users").expect("with its requests inside");
        assert_eq!(request["request"]["method"], "GET");
        assert!(request["request"]["url"]["raw"].as_str().unwrap().contains("{{baseUrl}}"), "variables stay as they are");
    }

    #[test]
    fn what_a_request_inherits_is_written_onto_it() {
        // Postman has no collection-level headers, so they are flattened down
        // rather than quietly lost.
        let out = exported();
        let users = find(&out["item"], "Users").unwrap();
        let request = find(&users["item"], "List users").unwrap();
        let headers = request["request"]["header"].as_array().unwrap();

        let collection_header = collection::load(&sample()).unwrap().meta.headers;
        for expected in collection_header.iter().filter(|h| h.enabled) {
            assert!(
                headers.iter().any(|h| h["key"] == expected.name.as_str()),
                "`{}` should have been copied down: {headers:?}",
                expected.name
            );
        }
    }

    #[test]
    fn secrets_and_environments_stay_behind() {
        let root = sample();
        let out = postman(&root, &collection::load(&root).unwrap()).unwrap();
        let loaded = collection::load(&root).unwrap();

        for env in &loaded.environments {
            for var in env.vars.iter().filter(|v| v.secret && v.value.len() > 3) {
                assert!(!out.json.contains(&var.value), "a secret value reached the export");
            }
        }
        assert!(out.notes.iter().any(|n| n.contains("Environments were not exported")), "{:?}", out.notes);
    }
}

//! Writing a collection out as an OpenAPI 3.0.3 document.
//!
//! The mirror of `openapi.rs`, and lossy in the other direction: a collection
//! is a list of calls somebody made, and a specification is a description of
//! everything that could be called. So what comes out is the part of a
//! specification a collection can honestly vouch for — which paths exist,
//! which methods they take, what was sent and what came back — with examples
//! where a schema would be, because an example is what volt has and a schema
//! inferred from one example is a guess written down as a fact.
//!
//! How a collection maps onto it:
//!
//! - The address before the path is a server. A leading `{{baseUrl}}` becomes
//!   a server with a variable of that name, so the document stays as portable
//!   as the collection was.
//! - `{{id}}` and `:id` in a path are path parameters, `{id}`.
//! - A request is an operation: its name the summary, its top folder the tag,
//!   its docs the description, its saved examples the responses.
//! - Bearer, basic, digest and API-key auth become security schemes. The ones
//!   OpenAPI has no words for are named in the notes.
//!
//! Secrets never travel, for the same reason they do not in the Postman
//! export: everything is taken as it is written on disk, where a secret is a
//! `{{name}}`, and a server variable's default is only ever a value that was
//! not marked secret.

use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

use crate::collection::{self, Collection, Node};
use crate::error::{Error, Result};
use crate::examples::{self, Example};
use crate::export::Exported;
use crate::import::split_query;
use crate::model::{ApiKeyLocation, Auth, Body, KeyValue, Kind, Request};

/// A mapping that keeps the order things were put in, which is the order a
/// person expects to read a specification in.
#[derive(Default)]
struct Object(Mapping);

impl Object {
    fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(Value::String(key.to_string()), value.into());
        self
    }

    fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.0.insert(Value::String(key.to_string()), value.into());
    }

    fn done(self) -> Value {
        Value::Mapping(self.0)
    }
}

fn text(value: &str) -> Value {
    Value::String(value.to_string())
}

/// The methods a path item has a field for.
const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

#[derive(Default)]
struct Document {
    /// In the order first seen.
    servers: Vec<Server>,
    paths: Vec<PathItem>,
    tags: Vec<String>,
    schemes: Vec<(String, Value, Auth)>,
    notes: Vec<String>,
    skipped: usize,
    used_ids: Vec<String>,
}

/// One path and the operations on it, both in the order first seen.
struct PathItem {
    path: String,
    operations: Vec<Operation>,
}

struct Operation {
    method: String,
    body: Mapping,
    /// Which of the document's servers it is called on.
    server: usize,
}

/// What came back with one status: each content type, and under it the saved
/// examples by name.
struct Answer {
    status: u16,
    description: String,
    content: Vec<(String, Vec<(String, Value)>)>,
}

#[derive(PartialEq)]
struct Server {
    url: String,
    /// The variable in it, when the address began with `{{name}}`.
    variable: Option<String>,
}

pub fn openapi(root: &Path, collection: &Collection) -> Result<Exported> {
    let mut doc = Document::default();
    for node in &collection.tree {
        walk(root, node, None, &mut doc)?;
    }

    let mut out = Object::default().with("openapi", text("3.0.3")).with(
        "info",
        Object::default()
            .with("title", text(&collection.meta.name))
            .with("version", text("1.0.0"))
            .with("description", text("Exported from volt"))
            .done(),
    );

    if !doc.servers.is_empty() {
        let servers: Vec<Value> = doc.servers.iter().map(|server| server_object(server, collection, &mut doc.notes)).collect();
        out.set("servers", Value::Sequence(servers));
    }
    if !doc.tags.is_empty() {
        let tags = doc.tags.iter().map(|tag| Object::default().with("name", text(tag)).done()).collect();
        out.set("tags", Value::Sequence(tags));
    }

    // What the collection itself asks for applies to every operation that
    // does not say otherwise, which is exactly what top-level `security` is.
    let collection_auth = collection.meta.auth.clone();
    if let Some(requirement) = security(&collection_auth, "the collection", &mut doc) {
        if !matches!(collection_auth, Auth::None | Auth::Inherit) {
            out.set("security", requirement);
        }
    }

    let several_servers = doc.servers.len() > 1;
    let mut paths = Mapping::new();
    for PathItem { path, operations } in std::mem::take(&mut doc.paths) {
        let mut item = Mapping::new();
        for Operation { method, body: mut operation, server } in operations {
            // With one server it is said once, at the top. With several, each
            // operation says which one is its own.
            if several_servers {
                if let Some(server) = doc.servers.get(server) {
                    let object = server_object(server, collection, &mut Vec::new());
                    operation.insert(text("servers"), Value::Sequence(vec![object]));
                }
            }
            item.insert(text(&method), Value::Mapping(operation));
        }
        paths.insert(text(&path), Value::Mapping(item));
    }
    out.set("paths", Value::Mapping(paths));

    if !doc.schemes.is_empty() {
        let mut schemes = Mapping::new();
        for (name, scheme, _) in &doc.schemes {
            schemes.insert(text(name), scheme.clone());
        }
        out.set("components", Object::default().with("securitySchemes", Value::Mapping(schemes)).done());
    }

    if doc.skipped > 0 {
        doc.notes.push(format!(
            "{} request{} that {} not plain HTTP (WebSocket, server-sent events, gRPC) {} left out: OpenAPI describes requests and responses, not connections.",
            doc.skipped,
            if doc.skipped == 1 { "" } else { "s" },
            if doc.skipped == 1 { "is" } else { "are" },
            if doc.skipped == 1 { "was" } else { "were" },
        ));
    }
    doc.notes.push(
        "Bodies and responses are written as examples, not schemas: an example is what the collection knows.".into(),
    );

    let document = out.done();
    let yaml = serde_yaml_ng::to_string(&document)
        .map_err(|e| Error::Invalid(format!("could not write the export: {e}")))?;
    let json = serde_json::to_string_pretty(&document)
        .map_err(|e| Error::Invalid(format!("could not write the export: {e}")))?;
    doc.notes.dedup();
    Ok(Exported { json, yaml: Some(yaml), notes: doc.notes })
}

fn walk(root: &Path, node: &Node, tag: Option<&str>, doc: &mut Document) -> Result<()> {
    match node {
        Node::Folder { name, children, .. } => {
            // The outermost folder is the tag; folders inside it are how the
            // files are kept, not a second way of grouping operations.
            let tag = tag.unwrap_or(name);
            for child in children {
                walk(root, child, Some(tag), doc)?;
            }
            Ok(())
        }
        Node::Request { id, .. } => {
            let request = collection::read_request(root, id)?;
            if request.kind != Kind::Http || matches!(request.body, Body::Grpc { .. }) {
                doc.skipped += 1;
                return Ok(());
            }
            operation(root, id, &request, tag, doc)
        }
    }
}

fn operation(root: &Path, id: &str, request: &Request, tag: Option<&str>, doc: &mut Document) -> Result<()> {
    let method = request.method.trim().to_ascii_lowercase();
    if !METHODS.contains(&method.as_str()) {
        doc.notes.push(format!(
            "`{}` uses the method {}, which an OpenAPI path has no place for, so it was left out.",
            request.name,
            request.method.trim().to_uppercase()
        ));
        return Ok(());
    }

    let (address, inline_query) = split_query(request.url.trim());
    let (server, path, path_parameters) = split_address(&address);
    let server_index = match doc.servers.iter().position(|known| *known == server) {
        Some(index) => index,
        None => {
            doc.servers.push(server);
            doc.servers.len() - 1
        }
    };

    let position = match doc.paths.iter().position(|known| known.path == path) {
        Some(position) => position,
        None => {
            doc.paths.push(PathItem { path: path.clone(), operations: Vec::new() });
            doc.paths.len() - 1
        }
    };
    if doc.paths[position].operations.iter().any(|known| known.method == method) {
        doc.notes.push(format!(
            "`{}` is a second {} {path}; OpenAPI has one operation per method and path, so only the first was written.",
            request.name,
            method.to_uppercase()
        ));
        return Ok(());
    }

    let mut out = Object::default().with("summary", text(&request.name)).with("operationId", text(&operation_id(id, doc)));
    if let Some(tag) = tag {
        if !doc.tags.iter().any(|known| known == tag) {
            doc.tags.push(tag.to_string());
        }
        out.set("tags", Value::Sequence(vec![text(tag)]));
    }
    if let Some(docs) = request.docs.as_deref().filter(|docs| !docs.trim().is_empty()) {
        out.set("description", text(docs));
    }

    // --- Parameters --------------------------------------------------------
    let scopes = collection::scopes(root, Some(id))?;
    let mut parameters: Vec<Value> = path_parameters.iter().map(|name| parameter(name, "path", true, "")).collect();
    let mut seen: Vec<String> = Vec::new();
    for param in inline_query.iter().chain(request.params.iter()) {
        if param.name.trim().is_empty() || seen.contains(&param.name) {
            continue;
        }
        seen.push(param.name.clone());
        parameters.push(parameter(&param.name, "query", false, &param.value));
    }
    // What the request inherits is part of what it sends, so it is part of
    // what the operation takes. A nearer scope replaces the same name.
    let mut headers: Vec<&KeyValue> = Vec::new();
    for header in scopes.headers().chain(request.headers.iter()).filter(|h| h.enabled && !h.name.trim().is_empty()) {
        headers.retain(|known| !known.name.eq_ignore_ascii_case(&header.name));
        headers.push(header);
    }
    for header in &headers {
        // "If `in` is `header` and the name is Accept, Content-Type or
        // Authorization, the parameter definition SHALL be ignored" — those
        // three are said by the content and by `security` instead.
        let lower = header.name.to_ascii_lowercase();
        if matches!(lower.as_str(), "accept" | "content-type" | "authorization") {
            continue;
        }
        parameters.push(parameter(&header.name, "header", false, &header.value));
    }
    if !parameters.is_empty() {
        out.set("parameters", Value::Sequence(parameters));
    }

    // --- Body --------------------------------------------------------------
    let declared_type = headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| mime_of(&h.value))
        .filter(|mime| !mime.is_empty() && !mime.contains("{{"));
    if let Some(body) = request_body(&request.body, declared_type.as_deref()) {
        out.set("requestBody", body);
    }

    // --- Responses ---------------------------------------------------------
    let kept = examples::list(root, id).unwrap_or_default();
    out.set("responses", responses(&kept, &request.name, &mut doc.notes));

    // --- Security ----------------------------------------------------------
    let effective = match &request.auth {
        Auth::Inherit => scopes.auth().clone(),
        other => other.clone(),
    };
    if !same_auth(&effective, &scopes.collection.auth) {
        if let Some(requirement) = security(&effective, &format!("`{}`", request.name), doc) {
            out.set("security", requirement);
        }
    }

    doc.paths[position].operations.push(Operation { method, body: out.0, server: server_index });
    Ok(())
}

/// `users/list-users.yaml` → `users_list_users`, made unique if it has to be.
fn operation_id(id: &str, doc: &mut Document) -> String {
    let stem = id.strip_suffix(".yaml").unwrap_or(id);
    let mut base: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if base.is_empty() {
        base.push_str("operation");
    }
    let mut candidate = base.clone();
    let mut n = 2;
    while doc.used_ids.contains(&candidate) {
        candidate = format!("{base}_{n}");
        n += 1;
    }
    doc.used_ids.push(candidate.clone());
    candidate
}

/// An address as a server, a path, and the parameters in that path.
///
/// `{{baseUrl}}/users/{{id}}` → the server `{baseUrl}`, the path
/// `/users/{id}`. `https://api.test/v1/users/:id` → the server
/// `https://api.test`, the path `/v1/users/{id}`. Where the server ends and
/// the path begins is the first `/` after the host, which for a variable is
/// the first `/` after it: a `{{baseUrl}}` that itself ends in `/v1` keeps
/// that in the server, where it belongs.
fn split_address(address: &str) -> (Server, String, Vec<String>) {
    let (base, rest, variable) = if let Some(after) = address.strip_prefix("{{") {
        match after.split_once("}}") {
            Some((name, rest)) if rest.is_empty() || rest.starts_with('/') => {
                let name = parameter_name(name);
                (format!("{{{name}}}"), rest, Some(name))
            }
            _ => (String::new(), address, None),
        }
    } else if let Some((scheme, rest)) = address.split_once("://") {
        let slash = rest.find('/').unwrap_or(rest.len());
        (format!("{scheme}://{}", &rest[..slash]), &rest[slash..], None)
    } else {
        (String::new(), address, None)
    };

    let mut parameters: Vec<String> = Vec::new();
    let mut path = String::new();
    for (index, segment) in rest.split('/').enumerate() {
        if index > 0 {
            path.push('/');
        }
        let mut name_it = |name: &str| {
            let name = parameter_name(name);
            if !parameters.contains(&name) {
                parameters.push(name.clone());
            }
            format!("{{{name}}}")
        };
        match segment.strip_prefix(':') {
            // `:id`, the way Postman and most routers write one.
            Some(name) if !name.is_empty() && !name.contains("{{") => path.push_str(&name_it(name)),
            _ => {
                // `{{id}}` anywhere in a segment: `report-{{id}}.pdf` too.
                let mut left = segment;
                while let Some(start) = left.find("{{") {
                    path.push_str(&left[..start]);
                    match left[start + 2..].split_once("}}") {
                        Some((name, after)) => {
                            path.push_str(&name_it(name));
                            left = after;
                        }
                        None => {
                            path.push_str(&left[start..]);
                            left = "";
                        }
                    }
                }
                path.push_str(left);
            }
        }
    }
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    (Server { url: base, variable }, path, parameters)
}

/// A variable's name as a parameter's: `$guid` and `user.id` are names volt
/// allows and a path template does not.
fn parameter_name(name: &str) -> String {
    let cleaned: String =
        name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let cleaned = cleaned.trim_matches('_').to_string();
    if cleaned.is_empty() { "param".to_string() } else { cleaned }
}

fn server_object(server: &Server, collection: &Collection, notes: &mut Vec<String>) -> Value {
    let url = if server.url.is_empty() { "/" } else { server.url.as_str() };
    let mut out = Object::default().with("url", text(url));
    if let Some(name) = &server.variable {
        // The collection's own value first, then the first environment that
        // has one — but never a secret: this document is meant to be shared.
        let value = collection
            .meta
            .vars
            .iter()
            .filter(|var| var.enabled && &var.name == name)
            .map(|var| var.value.clone())
            .chain(
                collection
                    .environments
                    .iter()
                    .flat_map(|env| env.vars.iter())
                    .filter(|var| !var.secret && &var.name == name)
                    .map(|var| var.value.clone()),
            )
            .find(|value| !value.trim().is_empty() && !value.contains("{{"));
        let default = match value {
            Some(value) => value,
            None => {
                notes.push(format!(
                    "No value for `{{{{{name}}}}}` was found to use as the server's default, so it is a placeholder; set it in the document."
                ));
                "http://localhost".to_string()
            }
        };
        let variable = Object::default().with("default", text(&default)).done();
        out.set("variables", Object::default().with(name, variable).done());
    }
    out.done()
}

fn parameter(name: &str, location: &str, required: bool, value: &str) -> Value {
    let mut out = Object::default()
        .with("name", text(name))
        .with("in", text(location))
        .with("required", Value::Bool(required))
        .with("schema", Object::default().with("type", text("string")).done());
    // A `{{variable}}` is not an example of anything; a literal is.
    if !value.trim().is_empty() && !value.contains("{{") {
        out.set("example", text(value));
    }
    out.done()
}

/// `application/json; charset=utf-8` → `application/json`.
fn mime_of(content_type: &str) -> String {
    content_type.split(';').next().unwrap_or_default().trim().to_ascii_lowercase()
}

/// Text as the value it is, when it is JSON, and as text when it is not —
/// so a JSON example reads as a structure in the document rather than as one
/// long quoted string.
fn example_value(content: &str, json: bool) -> Value {
    if json {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
            if let Ok(value) = serde_yaml_ng::to_value(&parsed) {
                return value;
            }
        }
    }
    text(content)
}

fn media(content_type: &str, schema: Option<Value>, example: Option<Value>) -> Value {
    let mut media = Object::default();
    if let Some(schema) = schema {
        media.set("schema", schema);
    }
    if let Some(example) = example {
        media.set("example", example);
    }
    Object::default().with("content", Object::default().with(content_type, media.done()).done()).done()
}

fn request_body(body: &Body, declared: Option<&str>) -> Option<Value> {
    let string = || Object::default().with("type", text("string")).done();
    let raw = |fallback: &str, content: &str| {
        let content_type = declared.unwrap_or(fallback);
        let example = example_value(content, content_type.contains("json"));
        // The one thing an example does say for certain is what kind of thing
        // it is at the top.
        let kind = match &example {
            Value::Mapping(_) => "object",
            Value::Sequence(_) => "array",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            _ => "string",
        };
        let schema = Object::default().with("type", text(kind)).done();
        Some(media(content_type, Some(schema), Some(example)))
    };

    match body {
        Body::None | Body::Grpc { .. } => None,
        Body::Json { content } => raw("application/json", content),
        Body::Text { content } => raw("text/plain", content),
        Body::Xml { content } => raw("application/xml", content),
        Body::UrlEncoded { fields } => {
            let mut properties = Mapping::new();
            let mut example = Mapping::new();
            for field in fields.iter().filter(|field| !field.name.trim().is_empty()) {
                properties.insert(text(&field.name), string());
                example.insert(text(&field.name), text(&field.value));
            }
            let schema = Object::default().with("type", text("object")).with("properties", Value::Mapping(properties));
            Some(media("application/x-www-form-urlencoded", Some(schema.done()), Some(Value::Mapping(example))))
        }
        Body::Form { fields } => {
            let mut properties = Mapping::new();
            let mut example = Mapping::new();
            for field in fields.iter().filter(|field| !field.name.trim().is_empty()) {
                match field.file {
                    // A file is a type, not an example: its path is this
                    // machine's business.
                    true => {
                        properties.insert(text(&field.name), Object::default().with("type", text("string")).with("format", text("binary")).done());
                    }
                    false => {
                        properties.insert(text(&field.name), string());
                        example.insert(text(&field.name), text(&field.value));
                    }
                }
            }
            let schema = Object::default().with("type", text("object")).with("properties", Value::Mapping(properties));
            let example = (!example.is_empty()).then_some(Value::Mapping(example));
            Some(media("multipart/form-data", Some(schema.done()), example))
        }
        Body::GraphQl { query, variables } => {
            let mut example = Object::default().with("query", text(query));
            if !variables.trim().is_empty() {
                example.set("variables", example_value(variables, true));
            }
            let schema = Object::default().with("type", text("object")).done();
            Some(media("application/json", Some(schema), Some(example.done())))
        }
        Body::Binary { .. } => {
            let schema = Object::default().with("type", text("string")).with("format", text("binary")).done();
            Some(media(declared.unwrap_or("application/octet-stream"), Some(schema), None))
        }
    }
}

/// The saved examples, as the responses they are examples of.
///
/// One entry per status. Several examples of the same status and type sit
/// side by side under `examples`, by name; one alone is the `example`.
fn responses(kept: &[Example], subject: &str, notes: &mut Vec<String>) -> Value {
    if kept.is_empty() {
        // A document needs at least one response per operation, and nothing
        // is known here except that the request exists.
        return Object::default().with("200", Object::default().with("description", text("Successful response")).done()).done();
    }

    let mut by_status: Vec<Answer> = Vec::new();
    for example in kept {
        let position = match by_status.iter().position(|answer| answer.status == example.status) {
            Some(position) => position,
            None => {
                let description = match example.status_text.trim() {
                    "" => example.name.clone(),
                    status_text => status_text.to_string(),
                };
                by_status.push(Answer { status: example.status, description, content: Vec::new() });
                by_status.len() - 1
            }
        };
        if example.body.is_empty() {
            continue;
        }
        if example.body_is_base64 {
            notes.push(format!("An example of {subject} has a binary body, which was left out of the document."));
            continue;
        }
        let content_type = example
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-type"))
            .map(|header| mime_of(&header.value))
            .filter(|mime| !mime.is_empty())
            .unwrap_or_else(|| "text/plain".to_string());
        let value = example_value(&example.body, content_type.contains("json"));
        let content = &mut by_status[position].content;
        match content.iter_mut().find(|(known, _)| *known == content_type) {
            Some((_, values)) => values.push((example.name.clone(), value)),
            None => content.push((content_type, vec![(example.name.clone(), value)])),
        }
    }
    by_status.sort_by_key(|answer| answer.status);

    let mut out = Mapping::new();
    for Answer { status, description, content } in by_status {
        let mut response = Object::default().with("description", text(&description));
        if !content.is_empty() {
            let mut types = Mapping::new();
            for (content_type, mut values) in content {
                let media = match values.len() {
                    1 => Object::default().with("example", values.remove(0).1),
                    _ => {
                        let mut named = Mapping::new();
                        for (name, value) in values {
                            named.insert(text(&name), Object::default().with("value", value).done());
                        }
                        Object::default().with("examples", Value::Mapping(named))
                    }
                };
                types.insert(text(&content_type), media.done());
            }
            response.set("content", Value::Mapping(types));
        }
        // A status is a string key: `200` unquoted would be a number, which a
        // strict reader refuses.
        out.insert(text(&status.to_string()), response.done());
    }
    Value::Mapping(out)
}

fn same_auth(a: &Auth, b: &Auth) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// The security requirement for an auth, registering its scheme on the way.
/// `None` when OpenAPI has no scheme for it — which is said in the notes —
/// or when nothing at all was asked for at the collection.
fn security(auth: &Auth, subject: &str, doc: &mut Document) -> Option<Value> {
    let http = |scheme: &str| Object::default().with("type", text("http")).with("scheme", text(scheme)).done();
    let (base, scheme) = match auth {
        // "No auth here" is an empty list, which is how an operation opts out
        // of what the document asks for everywhere else.
        Auth::None | Auth::Inherit => return Some(Value::Sequence(Vec::new())),
        Auth::Bearer { .. } => ("bearerAuth", http("bearer")),
        Auth::Basic { .. } => ("basicAuth", http("basic")),
        Auth::Digest { .. } => ("digestAuth", http("digest")),
        Auth::ApiKey { key, location, .. } => (
            "apiKeyAuth",
            Object::default()
                .with("type", text("apiKey"))
                .with("in", text(match location { ApiKeyLocation::Header => "header", ApiKeyLocation::Query => "query" }))
                .with("name", text(key))
                .done(),
        ),
        Auth::Ntlm { .. } | Auth::AwsSigV4 { .. } | Auth::OAuth1 { .. } => {
            let kind = match auth {
                Auth::Ntlm { .. } => "NTLM",
                Auth::AwsSigV4 { .. } => "AWS Signature V4",
                _ => "OAuth 1.0a",
            };
            doc.notes.push(format!("{subject} uses {kind}, which OpenAPI has no security scheme for, so its auth was left out."));
            return None;
        }
    };

    // Two API keys under different names are two schemes; the same one used
    // twice is one.
    let name = match doc.schemes.iter().find(|(_, known, _)| *known == scheme) {
        Some((name, _, _)) => name.clone(),
        None => {
            let mut name = base.to_string();
            let mut n = 2;
            while doc.schemes.iter().any(|(known, _, _)| *known == name) {
                name = format!("{base}{n}");
                n += 1;
            }
            doc.schemes.push((name.clone(), scheme, auth.clone()));
            name
        }
    };
    Some(Value::Sequence(vec![Object::default().with(&name, Value::Sequence(Vec::new())).done()]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use crate::model::{CollectionMeta, FolderMeta, FormField};

    fn sample() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sample-collection")
    }

    #[test]
    fn the_example_collection_exports_as_an_openapi_document() {
        let root = sample();
        let out = openapi(&root, &collection::load(&root).unwrap()).unwrap();

        // Both spellings of the same document, and both parse.
        let yaml: serde_json::Value = serde_yaml_ng::from_str(out.yaml.as_deref().expect("YAML is offered")).expect("valid YAML");
        let json: serde_json::Value = serde_json::from_str(&out.json).expect("`json` is JSON");
        assert_eq!(yaml, json);
        assert!(out.yaml.as_deref().unwrap().starts_with("openapi: 3.0.3\ninfo:\n  title: Sample API\n"), "{}", out.yaml.unwrap());

        assert_eq!(json["openapi"], "3.0.3");
        assert_eq!(json["info"]["title"], "Sample API");
        assert_eq!(json["servers"][0]["url"], "{baseUrl}");
        assert_eq!(
            json["servers"][0]["variables"]["baseUrl"]["default"], "https://jsonplaceholder.typicode.com",
            "the variable keeps the document portable, and its default makes it usable as it is"
        );
        assert_eq!(json["tags"], serde_json::json!([{ "name": "Auth" }, { "name": "Users" }]));

        let paths = json["paths"].as_object().unwrap();
        assert_eq!(paths.keys().collect::<Vec<_>>(), ["/posts", "/users"], "two requests on /users are one path");

        let list = &json["paths"]["/users"]["get"];
        assert_eq!(list["summary"], "List users");
        assert_eq!(list["tags"], serde_json::json!(["Users"]));
        assert_eq!(list["operationId"], "users_list_users");
        let limit = list["parameters"].as_array().unwrap().iter().find(|p| p["name"] == "_limit").expect("the query parameter");
        assert_eq!((&limit["in"], &limit["example"], &limit["required"]), (&serde_json::json!("query"), &serde_json::json!("3"), &serde_json::json!(false)));
        assert!(
            list["parameters"].as_array().unwrap().iter().all(|p| p["name"] != "Accept"),
            "Accept is the collection's header, and OpenAPI says it through content types, not a parameter"
        );
        assert_eq!(list["responses"]["200"]["description"], "Successful response", "no saved example: the default");
        assert!(list.get("requestBody").is_none());

        let create = &json["paths"]["/users"]["post"];
        assert_eq!(create["summary"], "Create user");
        assert!(create["description"].as_str().unwrap().contains("auth: inherit"), "docs are the description");
        let body = &create["requestBody"]["content"]["application/json"];
        assert_eq!(body["example"]["name"], "Ada Lovelace", "a JSON body is a structure in the document, not a string");
        assert_eq!(body["schema"]["type"], "object");

        let login = &json["paths"]["/posts"]["post"];
        assert_eq!(login["requestBody"]["content"]["application/json"]["example"]["password"], "{{token}}", "a secret stays a placeholder");
        assert!(json.get("security").is_none() && json.get("components").is_none(), "the sample asks for no auth");

        assert!(out.notes.iter().any(|note| note.contains("examples, not schemas")), "{:?}", out.notes);
    }

    #[test]
    fn an_address_divides_into_a_server_a_path_and_its_parameters() {
        let split = |address: &str| {
            let (server, path, parameters) = split_address(address);
            (server.url, server.variable, path, parameters)
        };
        assert_eq!(
            split("{{baseUrl}}/users/{{userId}}/posts/:postId"),
            ("{baseUrl}".into(), Some("baseUrl".into()), "/users/{userId}/posts/{postId}".into(), vec!["userId".to_string(), "postId".to_string()])
        );
        assert_eq!(split("https://api.test:8443/v1/users/:id"), ("https://api.test:8443".into(), None, "/v1/users/{id}".into(), vec!["id".to_string()]));
        assert_eq!(split("https://api.test"), ("https://api.test".into(), None, "/".into(), vec![]));
        assert_eq!(split("{{baseUrl}}"), ("{baseUrl}".into(), Some("baseUrl".into()), "/".into(), vec![]));
        // A variable that is not the whole of the host is not a server.
        assert_eq!(split("{{scheme}}://api.test/x").0, "");
        assert_eq!(split("/reports/report-{{id}}.pdf").2, "/reports/report-{id}.pdf");
        assert_eq!(split("{{baseUrl}}/a/{{$guid}}/{{user.id}}").3, ["guid", "user_id"], "names a template can hold");
        assert_eq!(split("{{baseUrl}}/a/{{id}}/b/{{id}}").3, ["id"], "one parameter however often it is used");
        assert_eq!(split("{{baseUrl}}/users/").2, "/users/", "a trailing slash is part of the path");
    }

    /// A collection made for the purpose: scopes, every kind of body, saved
    /// examples, several servers and the auth schemes on both sides of what
    /// OpenAPI can say.
    #[test]
    fn scopes_bodies_examples_and_auth_all_reach_the_document() {
        let root = std::env::temp_dir().join(format!("volt-openapi-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        collection::init(&root, "Shop").unwrap();
        collection::write_meta(
            &root,
            &CollectionMeta {
                name: "Shop".into(),
                version: 1,
                headers: vec![kv("X-Client", "volt")],
                auth: Auth::Bearer { token: "{{token}}".into() },
                vars: vec![kv("baseUrl", "https://api.shop.test/v2")],
            },
        )
        .unwrap();

        let orders = collection::create_folder(&root, None, "Orders").unwrap();
        collection::write_folder_meta(&root, &orders, &FolderMeta { name: Some("Orders".into()), headers: vec![kv("X-Area", "orders")], ..Default::default() }).unwrap();
        let save = |id: &str, request: Request| collection::write_request(&root, id, &request).unwrap();
        let base = |name: &str, method: &str, url: &str| Request { name: name.into(), method: method.into(), url: url.into(), ..Default::default() };

        save(&format!("{orders}/get.yaml"), Request {
            headers: vec![kv("X-Trace", "{{$guid}}"), KeyValue { enabled: false, ..kv("X-Off", "1") }],
            params: vec![kv("expand", "items"), KeyValue { enabled: false, ..kv("debug", "1") }],
            ..base("Get order", "GET", "{{baseUrl}}/orders/{{orderId}}?fields=id")
        });
        save(&format!("{orders}/get-again.yaml"), base("Get order again", "get", "{{baseUrl}}/orders/:orderId"));
        save(&format!("{orders}/pay.yaml"), Request {
            body: Body::UrlEncoded { fields: vec![kv("amount", "12.50"), kv("currency", "EUR")] },
            auth: Auth::ApiKey { key: "X-Pay-Key".into(), value: "{{payKey}}".into(), location: ApiKeyLocation::Header },
            ..base("Pay", "POST", "{{baseUrl}}/orders/{{orderId}}/pay")
        });
        save(&format!("{orders}/receipt.yaml"), Request {
            body: Body::Form { fields: vec![
                FormField { name: "note".into(), value: "thanks".into(), enabled: true, description: None, file: false },
                FormField { name: "scan".into(), value: "scans/r.png".into(), enabled: true, description: None, file: true },
            ] },
            auth: Auth::None,
            ..base("Upload receipt", "PUT", "{{baseUrl}}/orders/{{orderId}}/receipt")
        });
        save("status.yaml", Request { auth: Auth::Basic { username: "u".into(), password: "{{pw}}".into() }, ..base("Status", "GET", "https://status.shop.test/health") });
        save("search.yaml", Request {
            body: Body::GraphQl { query: "{ orders { id } }".into(), variables: "{\"first\": 3}".into() },
            auth: Auth::AwsSigV4 { key_id: "AK".into(), secret: "{{s}}".into(), region: "eu-west-1".into(), service: "execute-api".into(), session_token: String::new() },
            ..base("Search", "POST", "{{baseUrl}}/graphql")
        });
        save("purge.yaml", base("Purge", "PURGE", "{{baseUrl}}/cache"));
        save("live.yaml", Request { kind: Kind::Websocket, ..base("Live", "GET", "wss://api.shop.test/live") });
        save("events.yaml", Request { kind: Kind::Sse, ..base("Events", "GET", "{{baseUrl}}/events") });

        let response = |status: u16, status_text: &str, content_type: &str, body: &str| HttpResponse {
            status,
            status_text: status_text.into(),
            headers: vec![kv("Content-Type", content_type)],
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: 1,
            time_to_first_byte_ms: 1,
            final_url: String::new(),
            sent_url: String::new(),
            sent_method: String::new(),
            sent_headers: Vec::new(),
            sent_body_bytes: 0,
            redirects: Vec::new(),
            version: String::new(),
            missing_vars: Vec::new(),
        };
        let get = format!("{orders}/get.yaml");
        examples::save(&root, &get, "Found", &response(200, "OK", "application/json; charset=utf-8", r#"{"id": 7, "items": []}"#), &[]).unwrap();
        examples::save(&root, &get, "Found, with items", &response(200, "OK", "application/json", r#"{"id": 8, "items": [1]}"#), &[]).unwrap();
        examples::save(&root, &get, "Missing", &response(404, "Not Found", "text/plain", "no such order"), &[]).unwrap();

        let out = openapi(&root, &collection::load(&root).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        let json: serde_json::Value = serde_json::from_str(&out.json).unwrap();
        let notes = out.notes.join("\n");

        // Servers: one with a variable, one literal, and each operation says
        // which is its own because there is more than one.
        assert_eq!(json["servers"][0]["variables"]["baseUrl"]["default"], "https://api.shop.test/v2");
        assert_eq!(json["servers"][1]["url"], "https://status.shop.test");
        assert_eq!(json["paths"]["/health"]["get"]["servers"][0]["url"], "https://status.shop.test");
        assert_eq!(json["paths"]["/graphql"]["post"]["servers"][0]["url"], "{baseUrl}");

        // Path, query and inherited header parameters.
        let get = &json["paths"]["/orders/{orderId}"]["get"];
        let parameter = |name: &str| get["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).cloned().unwrap_or_else(|| panic!("no {name}: {get}"));
        assert_eq!((&parameter("orderId")["in"], &parameter("orderId")["required"]), (&serde_json::json!("path"), &serde_json::json!(true)));
        assert_eq!(parameter("fields")["example"], "id", "a query written into the address is a parameter too");
        assert_eq!(parameter("expand")["in"], "query");
        assert_eq!(parameter("debug")["in"], "query", "a switched-off parameter is still one the endpoint takes");
        assert_eq!(parameter("X-Client")["in"], "header", "the collection's header");
        assert_eq!(parameter("X-Area")["example"], "orders", "and the folder's");
        assert!(parameter("X-Trace").get("example").is_none(), "a variable is not an example");
        assert!(get["parameters"].as_array().unwrap().iter().all(|p| p["name"] != "X-Off"));
        assert_eq!(get["tags"], serde_json::json!(["Orders"]));

        // Responses from the saved examples.
        let ok = &get["responses"]["200"];
        assert_eq!(ok["description"], "OK");
        assert_eq!(ok["content"]["application/json"]["examples"]["Found"]["value"]["id"], 7, "two of one status sit side by side");
        assert_eq!(ok["content"]["application/json"]["examples"]["Found, with items"]["value"]["items"][0], 1);
        assert_eq!(get["responses"]["404"]["content"]["text/plain"]["example"], "no such order");

        assert!(notes.contains("`Get order again` is a second GET /orders/{orderId}"), "{notes}");

        // Bodies.
        let pay = &json["paths"]["/orders/{orderId}/pay"]["post"];
        let form = &pay["requestBody"]["content"]["application/x-www-form-urlencoded"];
        assert_eq!(form["example"], serde_json::json!({ "amount": "12.50", "currency": "EUR" }));
        assert_eq!(form["schema"]["properties"]["amount"]["type"], "string");
        let receipt = &json["paths"]["/orders/{orderId}/receipt"]["put"];
        let multipart = &receipt["requestBody"]["content"]["multipart/form-data"];
        assert_eq!(multipart["schema"]["properties"]["scan"]["format"], "binary");
        assert_eq!(multipart["example"], serde_json::json!({ "note": "thanks" }), "a file's path is not an example");
        let search = &json["paths"]["/graphql"]["post"]["requestBody"]["content"]["application/json"]["example"];
        assert_eq!((&search["query"], &search["variables"]["first"]), (&serde_json::json!("{ orders { id } }"), &serde_json::json!(3)));

        // Security: the collection's at the top, and only the operations that
        // differ say anything of their own.
        let schemes = &json["components"]["securitySchemes"];
        assert_eq!(schemes["bearerAuth"], serde_json::json!({ "type": "http", "scheme": "bearer" }));
        assert_eq!(schemes["basicAuth"]["scheme"], "basic");
        assert_eq!(schemes["apiKeyAuth"], serde_json::json!({ "type": "apiKey", "in": "header", "name": "X-Pay-Key" }));
        assert_eq!(json["security"], serde_json::json!([{ "bearerAuth": [] }]));
        assert!(get.get("security").is_none(), "inherits, so the document's own applies");
        assert_eq!(pay["security"], serde_json::json!([{ "apiKeyAuth": [] }]));
        assert_eq!(receipt["security"], serde_json::json!([]), "no auth is an empty list, which opts out");
        assert_eq!(json["paths"]["/health"]["get"]["security"], serde_json::json!([{ "basicAuth": [] }]));
        assert!(json["paths"]["/graphql"]["post"].get("security").is_none());
        assert!(notes.contains("`Search` uses AWS Signature V4, which OpenAPI has no security scheme for"), "{notes}");

        // What cannot be an operation is counted or named, not dropped.
        assert!(notes.contains("2 requests that are not plain HTTP"), "{notes}");
        assert!(notes.contains("`Purge` uses the method PURGE"), "{notes}");
        assert!(json["paths"].get("/cache").is_none() && json["paths"].get("/live").is_none());

        // No value anywhere in it but placeholders.
        assert!(!out.json.contains("payKey") && !out.json.contains("{{pw}}") && !out.json.contains("{{token}}"));
    }

    fn kv(name: &str, value: &str) -> KeyValue {
        KeyValue { name: name.into(), value: value.into(), enabled: true, description: None }
    }
}

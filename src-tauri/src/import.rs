//! Import from Postman (Collection v2.0 / v2.1) and Insomnia (v4 JSON, v5 YAML).
//!
//! Two stages. `parse` turns an export into an in-memory collection without
//! touching the disk, which is what the tests exercise. `write` lays that out
//! as a new collection directory using the same writers as the rest of the app.
//!
//! Exports are read as loose `serde_json::Value`s rather than strict structs:
//! real files omit fields, use numbers where the schema says strings, and
//! differ between versions, and one odd request should not fail the import.
//!
//! Folder-level auth and headers have no equivalent in the file format, so they
//! are pushed down into the requests that would have inherited them.
//!
//! Exports routinely carry live credentials as plain text. Committing them is
//! exactly what this project exists to prevent, so literal tokens, passwords
//! and API keys are moved into secret environment variables (and so into the
//! gitignored `.env.<environment>` files), leaving `{{name}}` in the YAML.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::collection::{self, Collection, COLLECTION_FILE, ENV_DIR, FOLDER_FILE};
use crate::error::{Error, Result};
use crate::scripts;
use crate::openapi;
use crate::model::{
    ApiKeyLocation, Auth, Body, CollectionMeta, EnvVar, Environment, FolderMeta, FormField, KeyValue, Kind, Request,
};

// ---------------------------------------------------------------------------
// In-memory result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ImportNode {
    /// A folder carries what it passes down, now that `folder.yaml` can hold
    /// headers and auth. Nothing is flattened onto the requests inside.
    Folder { name: String, auth: Auth, headers: Vec<KeyValue>, children: Vec<ImportNode> },
    /// Boxed: a `Request` dwarfs a folder, and an import tree is mostly folders.
    Request(Box<Request>),
}

#[derive(Debug)]
pub struct Imported {
    pub format: &'static str,
    pub name: String,
    pub headers: Vec<KeyValue>,
    pub auth: Auth,
    pub nodes: Vec<ImportNode>,
    pub environments: Vec<Environment>,
    pub warnings: Warnings,
}

/// Warnings grouped by message, so fifty requests with scripts read as one line.
#[derive(Debug, Default)]
pub struct Warnings(BTreeMap<String, Vec<String>>);

impl Warnings {
    pub(crate) fn add(&mut self, message: impl Into<String>, subject: &str) {
        let subjects = self.0.entry(message.into()).or_default();
        if !subject.is_empty() && !subjects.iter().any(|s| s == subject) {
            subjects.push(subject.to_string());
        }
    }

    pub fn render(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|(message, subjects)| match subjects.len() {
                0 => message.clone(),
                1..=3 => format!("{message}: {}", subjects.join(", ")),
                n => format!("{message}: {}, and {} more", subjects[..3].join(", "), n - 3),
            })
            .collect()
    }

    #[cfg(test)]
    fn mentions(&self, needle: &str) -> bool {
        self.render().iter().any(|w| w.contains(needle))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub format: String,
    pub path: String,
    pub requests: usize,
    pub folders: usize,
    pub environments: Vec<String>,
    /// Every variable whose value went to a `.env.<environment>` file rather than the YAML.
    pub secrets: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub collection: Collection,
    pub report: Report,
}

/// Read an export and create a collection for it inside `into`.
pub fn import_file(source: &Path, into: &Path) -> Result<Outcome> {
    let text = fs::read_to_string(source)
        .map_err(|e| Error::io(source.to_string_lossy(), e))?;
    let imported = parse(&text)?;
    let root = write(into, &imported)?;
    let collection = collection::load(&root)?;

    let (requests, folders) = count(&imported.nodes);
    let secrets: BTreeSet<String> = imported
        .environments
        .iter()
        .flat_map(|e| e.vars.iter().filter(|v| v.secret).map(|v| v.name.clone()))
        .collect();

    Ok(Outcome {
        report: Report {
            format: imported.format.to_string(),
            path: root.to_string_lossy().into_owned(),
            requests,
            folders,
            environments: imported.environments.iter().map(|e| e.name.clone()).collect(),
            secrets: secrets.into_iter().collect(),
            warnings: imported.warnings.render(),
        },
        collection,
    })
}

fn count(nodes: &[ImportNode]) -> (usize, usize) {
    nodes.iter().fold((0, 0), |(r, f), node| match node {
        ImportNode::Request(_) => (r + 1, f),
        ImportNode::Folder { children, .. } => {
            let (cr, cf) = count(children);
            (r + cr, f + cf + 1)
        }
    })
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

pub fn parse(text: &str) -> Result<Imported> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let value: Value = if text.starts_with('{') {
        serde_json::from_str(text).map_err(|e| Error::Invalid(format!("the file is not valid JSON: {e}")))?
    } else {
        collection::yaml_value(text)
            .map_err(|e| Error::Invalid(format!("the file is not valid JSON or YAML: {e}")))?
    };

    let mut imported = if openapi::looks_like(&value) {
        openapi::parse(&value)
    } else if value.get("info").is_some() && value.get("item").is_some() {
        postman(&value)
    } else if value.get("requests").is_some() && value.get("order").is_some() {
        return Err(Error::Invalid(
            "this is a Postman v1 export; export the collection again from Postman as \"Collection v2.1\"".into(),
        ));
    } else if value.get("_type").and_then(Value::as_str) == Some("export") {
        insomnia_v4(&value)?
    } else if let Some(kind) = value.get("type").and_then(Value::as_str) {
        if kind.starts_with("collection.insomnia.rest/") {
            insomnia_v5(&value)
        } else {
            return Err(Error::Invalid(format!(
                "`{kind}` is not a request collection; export the collection itself from Insomnia"
            )));
        }
    } else {
        return Err(Error::Invalid(
            "this is not a Postman (v2.0/v2.1), Insomnia (v4/v5) or OpenAPI/Swagger file".into(),
        ));
    };

    finish(&mut imported);
    Ok(imported)
}

// ---------------------------------------------------------------------------
// Small readers over loose JSON
// ---------------------------------------------------------------------------

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// Exports put numbers and booleans where strings are expected.
fn text_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() { fallback.to_string() } else { trimmed.to_string() }
}

/// Postman descriptions are a string or `{ content, type }`.
fn description(v: Option<&Value>) -> Option<String> {
    let text = match v? {
        Value::String(s) => s.clone(),
        other => s(other, "content").to_string(),
    };
    (!text.trim().is_empty()).then_some(text)
}

fn sort_key(v: &Value, pointer: &str) -> f64 {
    v.pointer(pointer).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Auth a node declares, versus "whatever is above me".
#[derive(Debug, Clone)]
enum AuthSpec {
    Inherit,
    Set(Auth),
}

impl AuthSpec {
    fn or(self, inherited: &AuthSpec) -> AuthSpec {
        match self {
            AuthSpec::Inherit => inherited.clone(),
            set => set,
        }
    }

    fn into_auth(self) -> Auth {
        match self {
            AuthSpec::Set(auth) => auth,
            AuthSpec::Inherit => Auth::Inherit,
        }
    }
}

/// Pick a body type from an explicit hint, the Content-Type, or the content.
fn text_body(content: String, hint: &str, headers: &[KeyValue]) -> Body {
    let content_type = headers
        .iter()
        .find(|h| h.enabled && h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default();
    let hint = hint.to_ascii_lowercase();
    let start = content.trim_start();

    if hint.contains("json") || content_type.contains("json") {
        Body::Json { content }
    } else if hint.contains("xml") || content_type.contains("xml") {
        Body::Xml { content }
    } else if hint.is_empty() && content_type.is_empty() && (start.starts_with('{') || start.starts_with('[')) {
        Body::Json { content }
    } else {
        Body::Text { content }
    }
}

// ---------------------------------------------------------------------------
// Postman
// ---------------------------------------------------------------------------

fn postman(v: &Value) -> Imported {
    let mut w = Warnings::default();
    let info = &v["info"];
    let format = if s(info, "schema").contains("v2.0") { "Postman v2.0" } else { "Postman v2.1" };
    let name = non_empty(s(info, "name"), "Imported collection");

    postman_scripts(v, &name, &mut w);
    let auth = match postman_auth(&v["auth"], &name, &mut w) {
        AuthSpec::Set(auth) => auth,
        AuthSpec::Inherit => Auth::None,
    };
    let nodes = postman_items(arr(v, "item"), &mut w);

    let vars: Vec<EnvVar> = arr(v, "variable")
        .iter()
        .filter(|var| !flag(var, "disabled"))
        .filter_map(|var| {
            let key = text_of(var.get("key").or_else(|| var.get("id")));
            (!key.is_empty()).then(|| EnvVar {
                name: key,
                value: text_of(var.get("value")),
                secret: s(var, "type") == "secret",
            })
        })
        .collect();
    let environments = if vars.is_empty() {
        Vec::new()
    } else {
        vec![Environment { name: "local".into(), vars }]
    };

    Imported { format, name, headers: Vec::new(), auth, nodes, environments, warnings: w }
}

fn postman_items(items: &[Value], w: &mut Warnings) -> Vec<ImportNode> {
    let mut out = Vec::new();
    for item in items {
        let name = non_empty(s(item, "name"), "Untitled");

        if let Some(children) = item.get("item").and_then(Value::as_array) {
            postman_scripts(item, &name, w);
            let auth = postman_auth(&item["auth"], &name, w).into_auth();
            let children = postman_items(children, w);
            out.push(ImportNode::Folder { name, auth, headers: Vec::new(), children });
        } else if let Some(request) = item.get("request") {
            out.push(ImportNode::Request(Box::new(postman_request(item, request, name, w))));
        }
    }
    out
}

/// A collection's or a folder's own scripts. volt has no folder-level checks or
/// captures — a check belongs to the request it is about — so these are
/// reported rather than pushed down onto every request inside.
fn postman_scripts(item: &Value, subject: &str, w: &mut Warnings) {
    if !postman_code(item, "prerequest").is_empty() || !postman_code(item, "test").is_empty() {
        w.add("Scripts on a collection or a folder are not imported", subject);
    }
}

/// The code of one of an item's events, as one string.
fn postman_code(item: &Value, listen: &str) -> String {
    arr(item, "event")
        .iter()
        .filter(|event| s(event, "listen") == listen)
        .map(|event| match event.pointer("/script/exec") {
            Some(Value::Array(lines)) => {
                lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")
            }
            Some(Value::String(code)) => code.clone(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read a request's after-response script, and say in the report exactly what
/// was understood and what was left — the whole point of translating only the
/// shapes volt has is that the rest is visible rather than quietly gone.
fn read_script(code: &str, pre: &str, subject: &str, w: &mut Warnings) -> scripts::Translated {
    if !pre.trim().is_empty() {
        w.add("Pre-request scripts are not imported: nothing in volt runs before a send", subject);
    }
    let out = scripts::translate(code);
    if out.found_something() {
        w.add("Test scripts were read as checks and captures", subject);
    }
    for line in &out.left {
        let shown: String = match line.chars().count() > 60 {
            true => line.chars().take(57).collect::<String>() + "…",
            false => line.clone(),
        };
        w.add(format!("A line of a test script was not imported — `{shown}`"), subject);
    }
    out
}

fn postman_auth(v: &Value, subject: &str, w: &mut Warnings) -> AuthSpec {
    let Some(kind) = v.get("type").and_then(Value::as_str) else {
        return AuthSpec::Inherit;
    };
    // v2.1 stores the parameters as [{key, value}], v2.0 as a plain object.
    let field = |key: &str| -> String {
        match v.get(kind) {
            Some(Value::Array(items)) => items
                .iter()
                .find(|item| s(item, "key") == key)
                .map(|item| text_of(item.get("value")))
                .unwrap_or_default(),
            Some(object @ Value::Object(_)) => text_of(object.get(key)),
            _ => String::new(),
        }
    };

    match kind {
        "noauth" => AuthSpec::Set(Auth::None),
        "inherit" => AuthSpec::Inherit,
        "bearer" => AuthSpec::Set(Auth::Bearer { token: field("token") }),
        "basic" => AuthSpec::Set(Auth::Basic { username: field("username"), password: field("password") }),
        "apikey" => AuthSpec::Set(Auth::ApiKey {
            key: field("key"),
            value: field("value"),
            location: if field("in") == "query" { ApiKeyLocation::Query } else { ApiKeyLocation::Header },
        }),
        "digest" => AuthSpec::Set(Auth::Digest { username: field("username"), password: field("password") }),
        "ntlm" => AuthSpec::Set(Auth::Ntlm {
            username: field("username"),
            password: field("password"),
            domain: field("domain"),
        }),
        "awsv4" => AuthSpec::Set(Auth::AwsSigV4 {
            key_id: field("accessKey"),
            secret: field("secretKey"),
            region: non_empty(&field("region"), "us-east-1"),
            service: field("service"),
            session_token: field("sessionToken"),
        }),
        other => {
            w.add(format!("`{other}` auth is not supported yet, so these use the auth above them instead"), subject);
            AuthSpec::Inherit
        }
    }
}

fn postman_request(item: &Value, req: &Value, name: String, w: &mut Warnings) -> Request {
    // v2 allows `"request": "https://…"` as shorthand for a GET.
    if let Some(raw) = req.as_str() {
        let (url, params) = split_query(raw);
        return Request { name, seq: 1, method: "GET".into(), url, params, ..Default::default() };
    }

    let method = non_empty(s(req, "method"), "GET").to_uppercase();
    let (url, params) = postman_url(&req["url"]);
    let headers = key_values(arr(req, "header"), "key", |s| s.to_string());
    let body = postman_body(&req["body"], &headers, &name, w);
    // Its own, or `inherit`: the folder above it holds what it would inherit.
    let auth = postman_auth(&req["auth"], &name, w).into_auth();
    let docs = description(req.get("description")).or_else(|| description(item.get("description")));
    // `pm.environment.set` is a capture and `pm.test` is a check; the rest of
    // the script is listed in the report rather than guessed at.
    let script = read_script(&postman_code(item, "test"), &postman_code(item, "prerequest"), &name, w);

    let request = Request {
        name,
        kind: Kind::Http,
        seq: 1,
        method,
        url,
        headers,
        params,
        body,
        auth,
        captures: script.captures,
        checks: script.checks,
        options: None,
        docs,
    };
    if serde_json::to_string(&request).is_ok_and(|json| json.contains("{{$")) {
        w.add("Postman dynamic variables such as {{$guid}} are not supported and will show as undefined", &request.name);
    }
    request
}

fn key_values(items: &[Value], key: &str, convert: impl Fn(&str) -> String) -> Vec<KeyValue> {
    items
        .iter()
        .filter_map(|item| {
            let name = convert(&text_of(item.get(key)));
            let value = convert(&text_of(item.get("value")));
            if name.is_empty() && value.is_empty() {
                return None;
            }
            Some(KeyValue {
                name,
                value,
                enabled: !flag(item, "disabled"),
                description: description(item.get("description")),
            })
        })
        .collect()
}

fn postman_url(v: &Value) -> (String, Vec<KeyValue>) {
    match v {
        Value::String(raw) => split_query(raw),
        Value::Object(_) => {
            let raw = v.get("raw").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| build_url(v));
            let (mut base, from_raw) = split_query(&raw);
            // The raw URL repeats the query; the `query` list is the real one
            // and also carries disabled parameters.
            let params = match v.get("query").and_then(Value::as_array) {
                Some(query) => key_values(query, "key", |s| s.to_string()),
                None => from_raw,
            };

            // `:id` path variables have a value per request, so bake it in.
            for var in arr(v, "variable") {
                let key = text_of(var.get("key"));
                if key.is_empty() {
                    continue;
                }
                let value = text_of(var.get("value"));
                let replacement = if value.is_empty() { format!("{{{{{key}}}}}") } else { value };
                let segment = format!(":{key}");
                base = base
                    .split('/')
                    .map(|part| if part == segment { replacement.as_str() } else { part })
                    .collect::<Vec<_>>()
                    .join("/");
            }
            (base, params)
        }
        _ => (String::new(), Vec::new()),
    }
}

/// For URL objects exported without `raw`.
fn build_url(v: &Value) -> String {
    let join = |key: &str, separator: &str| -> String {
        match v.get(key) {
            Some(Value::Array(parts)) => parts
                .iter()
                .map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| text_of(p.get("value"))))
                .collect::<Vec<_>>()
                .join(separator),
            other => text_of(other),
        }
    };

    let mut out = String::new();
    let protocol = s(v, "protocol");
    if !protocol.is_empty() {
        out.push_str(protocol);
        out.push_str("://");
    }
    out.push_str(&join("host", "."));
    let port = text_of(v.get("port"));
    if !port.is_empty() {
        out.push(':');
        out.push_str(&port);
    }
    let path = join("path", "/");
    if !path.is_empty() {
        if !path.starts_with('/') {
            out.push('/');
        }
        out.push_str(&path);
    }
    out
}

/// Separate a query string into parameters. Values are decoded, because volt
/// encodes parameters itself when it sends them.
pub(crate) fn split_query(raw: &str) -> (String, Vec<KeyValue>) {
    let without_fragment = raw.split_once('#').map_or(raw, |(before, _)| before);
    match without_fragment.split_once('?') {
        None => (without_fragment.to_string(), Vec::new()),
        Some((base, query)) => {
            let params = url::form_urlencoded::parse(query.as_bytes())
                .map(|(name, value)| KeyValue {
                    name: name.into_owned(),
                    value: value.into_owned(),
                    enabled: true,
                    description: None,
                })
                .collect();
            (base.to_string(), params)
        }
    }
}

fn postman_body(v: &Value, headers: &[KeyValue], subject: &str, w: &mut Warnings) -> Body {
    if !v.is_object() || flag(v, "disabled") {
        return Body::None;
    }
    match s(v, "mode") {
        "raw" => {
            let content = s(v, "raw").to_string();
            if content.is_empty() {
                return Body::None;
            }
            let language = v.pointer("/options/raw/language").and_then(Value::as_str).unwrap_or("");
            text_body(content, language, headers)
        }
        "urlencoded" => Body::UrlEncoded { fields: key_values(arr(v, "urlencoded"), "key", |s| s.to_string()) },
        "formdata" => {
            // A file part imports as a path. It will not exist on this machine,
            // but the shape of the request survives and the report says so.
            let fields: Vec<FormField> = arr(v, "formdata")
                .iter()
                .filter_map(|item| {
                    let is_file = text_of(item.get("type")) == "file";
                    let name = text_of(item.get("key"));
                    let value = text_of(item.get(if is_file { "src" } else { "value" }));
                    if name.is_empty() && value.is_empty() {
                        return None;
                    }
                    Some(FormField {
                        name,
                        value,
                        enabled: !flag(item, "disabled"),
                        description: description(item.get("description")),
                        file: is_file,
                    })
                })
                .collect();
            if fields.iter().any(|f| f.file) {
                w.add("A file field keeps the path from the export; point it at a file on this machine", subject);
            }
            Body::Form { fields }
        }
        "file" => {
            w.add("File bodies were not imported, since their path belongs to another machine", subject);
            Body::None
        }
        "graphql" => {
            let graphql = &v["graphql"];
            let variables = match graphql.get("variables") {
                Some(Value::String(text)) if !text.trim().is_empty() => {
                    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
                }
                Some(object @ Value::Object(_)) => object.clone(),
                _ => Value::Object(Default::default()),
            };
            let payload = serde_json::json!({ "query": text_of(graphql.get("query")), "variables": variables });
            w.add("GraphQL bodies were converted to a JSON body", subject);
            Body::Json { content: serde_json::to_string_pretty(&payload).unwrap_or_default() }
        }
        _ => Body::None,
    }
}

// ---------------------------------------------------------------------------
// Insomnia
// ---------------------------------------------------------------------------

/// `{{ _.baseUrl }}` → `{{baseUrl}}`. Template tags (`{% … %}`) have no
/// equivalent and are left as they are.
fn insomnia_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let inner = after[..end].trim();
                let inner = inner.strip_prefix("_.").unwrap_or(inner);
                out.push_str("{{");
                out.push_str(inner);
                out.push_str("}}");
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn insomnia_v4(v: &Value) -> Result<Imported> {
    let resources = arr(v, "resources");
    if resources.is_empty() {
        return Err(Error::Invalid("the Insomnia export contains no resources".into()));
    }
    let mut w = Warnings::default();

    let name = resources
        .iter()
        .find(|r| s(r, "_type") == "workspace")
        .map(|r| non_empty(s(r, "name"), "Imported collection"))
        .unwrap_or_else(|| "Imported collection".into());

    let groups: HashSet<&str> = resources
        .iter()
        .filter(|r| matches!(s(r, "_type"), "request_group" | "folder"))
        .map(|r| s(r, "_id"))
        .collect();
    let nodes = v4_children(resources, None, &groups, &AuthSpec::Inherit, &[], &mut HashSet::new(), &mut w);

    let envs: Vec<&Value> = resources.iter().filter(|r| s(r, "_type") == "environment").collect();
    let env_ids: HashSet<&str> = envs.iter().map(|e| s(e, "_id")).collect();
    let base = envs.iter().find(|e| !env_ids.contains(s(e, "parentId")));
    let subs: Vec<(&str, &Value, bool)> = match base {
        Some(base) => envs
            .iter()
            .filter(|e| s(e, "parentId") == s(base, "_id"))
            .map(|e| (s(e, "name"), &e["data"], flag(e, "isPrivate")))
            .collect(),
        None => Vec::new(),
    };
    let environments = insomnia_environments(base.map(|b| (&b["data"], flag(b, "isPrivate"))), subs);

    Ok(Imported {
        format: "Insomnia v4",
        name,
        headers: Vec::new(),
        auth: Auth::None,
        nodes,
        environments,
        warnings: w,
    })
}

/// `path` is the chain of folder ids above this one. A document where two
/// resources share an `_id` — two exports concatenated is enough — otherwise
/// walks in a circle until the stack runs out, and a stack overflow cannot be
/// caught: it takes the whole app down with every open tab.
fn v4_children<'a>(
    resources: &'a [Value],
    parent: Option<&str>,
    groups: &HashSet<&str>,
    auth: &AuthSpec,
    headers: &[KeyValue],
    path: &mut HashSet<&'a str>,
    w: &mut Warnings,
) -> Vec<ImportNode> {
    let mut items: Vec<&Value> = resources
        .iter()
        .filter(|r| {
            let parent_id = s(r, "parentId");
            let here = match parent {
                Some(p) => parent_id == p,
                // An export of one folder has no workspace, so the roots are
                // whatever does not sit inside a group.
                None => !groups.contains(parent_id),
            };
            here && matches!(s(r, "_type"), "request" | "request_group" | "folder" | "grpc_request" | "websocket_request")
        })
        .collect();
    items.sort_by(|a, b| sort_key(a, "/metaSortKey").partial_cmp(&sort_key(b, "/metaSortKey")).unwrap_or(Ordering::Equal));

    let mut out = Vec::new();
    for item in items {
        let name = non_empty(s(item, "name"), "Untitled");
        match s(item, "_type") {
            "request_group" | "folder" => {
                // The folder keeps what it adds; what it inherited is already
                // on the folder above it.
                let (own_auth, own_headers) = insomnia_group(item, &AuthSpec::Inherit, &[], &name, w);
                let id = s(item, "_id");
                if !path.insert(id) {
                    w.add("a folder is listed inside itself and was skipped", &name);
                    continue;
                }
                let children =
                    v4_children(resources, Some(id), groups, &AuthSpec::Inherit, &[], path, w);
                path.remove(id);
                out.push(ImportNode::Folder {
                    name,
                    auth: own_auth.into_auth(),
                    headers: own_headers,
                    children,
                });
            }
            "request" => out.push(ImportNode::Request(Box::new(insomnia_request(item, name, auth, headers, w)))),
            _ => w.add("gRPC and WebSocket requests are not supported and were skipped", &name),
        }
    }
    out
}

fn insomnia_v5(v: &Value) -> Imported {
    let mut w = Warnings::default();
    let name = non_empty(s(v, "name"), "Imported collection");
    let nodes = v5_items(arr(v, "collection"), &AuthSpec::Inherit, &[], &mut w);

    let env = &v["environments"];
    let base = env.is_object().then(|| (&env["data"], false));
    let subs = arr(env, "subEnvironments")
        .iter()
        .map(|e| {
            let private = flag(e, "isPrivate") || e.pointer("/meta/isPrivate").and_then(Value::as_bool).unwrap_or(false);
            (s(e, "name"), &e["data"], private)
        })
        .collect();
    let environments = insomnia_environments(base, subs);

    Imported { format: "Insomnia v5", name, headers: Vec::new(), auth: Auth::None, nodes, environments, warnings: w }
}

fn v5_items(items: &[Value], auth: &AuthSpec, headers: &[KeyValue], w: &mut Warnings) -> Vec<ImportNode> {
    let mut sorted: Vec<&Value> = items.iter().collect();
    sorted.sort_by(|a, b| sort_key(a, "/meta/sortKey").partial_cmp(&sort_key(b, "/meta/sortKey")).unwrap_or(Ordering::Equal));

    let mut out = Vec::new();
    for item in sorted {
        let name = non_empty(s(item, "name"), "Untitled");
        if let Some(children) = item.get("children").and_then(Value::as_array) {
            let (own_auth, own_headers) = insomnia_group(item, &AuthSpec::Inherit, &[], &name, w);
            let children = v5_items(children, &AuthSpec::Inherit, &[], w);
            out.push(ImportNode::Folder {
                name,
                auth: own_auth.into_auth(),
                headers: own_headers,
                children,
            });
            continue;
        }

        let id = item.pointer("/meta/id").and_then(Value::as_str).unwrap_or("");
        let url = s(item, "url");
        let streaming = item.get("reflectionApi").is_some()
            || id.starts_with("ws-req")
            || id.starts_with("socketio-req")
            || url.starts_with("ws://")
            || url.starts_with("wss://");
        if streaming {
            w.add("gRPC and WebSocket requests are not supported and were skipped", &name);
        } else if item.get("method").is_some() || item.get("url").is_some() {
            out.push(ImportNode::Request(Box::new(insomnia_request(item, name, auth, headers, w))));
        }
    }
    out
}

/// A folder's own auth and headers, combined with what it inherits.
fn insomnia_group(
    group: &Value,
    auth: &AuthSpec,
    headers: &[KeyValue],
    subject: &str,
    w: &mut Warnings,
) -> (AuthSpec, Vec<KeyValue>) {
    let (own, extra) = insomnia_auth(&group["authentication"], subject, w);
    let mut combined = headers.to_vec();
    combined.extend(key_values(arr(group, "headers"), "name", insomnia_vars));
    combined.extend(extra);
    (own.or(auth), combined)
}

/// Returns the auth and, for a bearer token with a custom prefix (which the
/// file format cannot express), the header that does the same job.
fn insomnia_auth(v: &Value, subject: &str, w: &mut Warnings) -> (AuthSpec, Option<KeyValue>) {
    let Some(kind) = v.get("type").and_then(Value::as_str) else {
        return (AuthSpec::Inherit, None);
    };
    if flag(v, "disabled") {
        return (AuthSpec::Set(Auth::None), None);
    }
    let field = |key: &str| insomnia_vars(&text_of(v.get(key)));

    match kind {
        "none" => (AuthSpec::Set(Auth::None), None),
        "bearer" => {
            let prefix = field("prefix");
            let prefix = prefix.trim();
            if prefix.is_empty() || prefix.eq_ignore_ascii_case("bearer") {
                (AuthSpec::Set(Auth::Bearer { token: field("token") }), None)
            } else {
                let header = KeyValue {
                    name: "Authorization".into(),
                    value: format!("{prefix} {}", field("token")),
                    enabled: true,
                    description: None,
                };
                (AuthSpec::Set(Auth::None), Some(header))
            }
        }
        "basic" => (
            AuthSpec::Set(Auth::Basic { username: field("username"), password: field("password") }),
            None,
        ),
        "apikey" => {
            let location = match s(v, "addTo") {
                "queryParams" | "query" => ApiKeyLocation::Query,
                "cookie" => {
                    w.add("API keys sent as cookies are not supported, so these use the auth above them instead", subject);
                    return (AuthSpec::Inherit, None);
                }
                _ => ApiKeyLocation::Header,
            };
            (AuthSpec::Set(Auth::ApiKey { key: field("key"), value: field("value"), location }), None)
        }
        "digest" => (
            AuthSpec::Set(Auth::Digest { username: field("username"), password: field("password") }),
            None,
        ),
        "ntlm" => (
            AuthSpec::Set(Auth::Ntlm {
                username: field("username"),
                password: field("password"),
                domain: field("domain"),
            }),
            None,
        ),
        "iam" => (
            AuthSpec::Set(Auth::AwsSigV4 {
                key_id: field("accessKeyId"),
                secret: field("secretAccessKey"),
                region: non_empty(&field("region"), "us-east-1"),
                service: field("service"),
                session_token: field("sessionToken"),
            }),
            None,
        ),
        other => {
            w.add(format!("`{other}` auth is not supported yet, so these use the auth above them instead"), subject);
            (AuthSpec::Inherit, None)
        }
    }
}

fn insomnia_request(item: &Value, name: String, auth: &AuthSpec, inherited_headers: &[KeyValue], w: &mut Warnings) -> Request {
    let method = non_empty(s(item, "method"), "GET").to_uppercase();
    let url = insomnia_vars(s(item, "url"));

    let mut headers = inherited_headers.to_vec();
    headers.extend(key_values(arr(item, "headers"), "name", insomnia_vars));
    let params = key_values(arr(item, "parameters"), "name", insomnia_vars);
    let body = insomnia_body(&item["body"], &headers, &name, w);

    let (own, extra) = insomnia_auth(&item["authentication"], &name, w);
    headers.extend(extra);
    let auth = own.or(auth).into_auth();

    let docs = description(item.get("description")).or_else(|| description(item.pointer("/meta/description")));

    // v4 puts a script in a field of its own, v5 under `scripts`.
    let code = |field: &str, path: &str| {
        let own = s(item, field);
        match own.trim().is_empty() {
            false => own.to_string(),
            true => item.pointer(path).and_then(Value::as_str).unwrap_or("").to_string(),
        }
    };
    let script = read_script(
        &code("afterResponseScript", "/scripts/afterResponse"),
        &code("preRequestScript", "/scripts/preRequest"),
        &name,
        w,
    );

    let request = Request {
        name,
        kind: Kind::Http,
        seq: 1,
        method,
        url,
        headers,
        params,
        body,
        auth,
        captures: script.captures,
        checks: script.checks,
        options: None,
        docs,
    };
    if serde_json::to_string(&request).is_ok_and(|json| json.contains("{%")) {
        w.add("Insomnia template tags such as {% response %} are not supported and were left as text", &request.name);
    }
    request
}

fn insomnia_body(v: &Value, headers: &[KeyValue], subject: &str, w: &mut Warnings) -> Body {
    if !v.is_object() {
        return Body::None;
    }
    let mime = s(v, "mimeType").to_ascii_lowercase();
    let text = insomnia_vars(s(v, "text"));

    if !s(v, "fileName").is_empty() {
        w.add("File bodies were not imported, since their path belongs to another machine", subject);
        return Body::None;
    }

    if mime.contains("x-www-form-urlencoded") {
        Body::UrlEncoded { fields: key_values(arr(v, "params"), "name", insomnia_vars) }
    } else if mime.contains("multipart/form-data") {
        let (files, text_fields): (Vec<Value>, Vec<Value>) =
            arr(v, "params").iter().cloned().partition(|p| s(p, "type") == "file");
        if !files.is_empty() {
            w.add("File fields in multipart bodies are not supported and were left out", subject);
        }
        Body::Form { fields: key_values(&text_fields, "name", insomnia_vars).into_iter().map(form_field).collect() }
    } else if mime.contains("graphql") {
        // Insomnia already stores GraphQL as {"query", "variables"} JSON.
        w.add("GraphQL bodies were converted to a JSON body", subject);
        Body::Json { content: text }
    } else if text.is_empty() {
        Body::None
    } else {
        text_body(text, &mime, headers)
    }
}

/// A volt environment per Insomnia sub-environment, each with the base
/// environment's values underneath its own.
fn insomnia_environments(base: Option<(&Value, bool)>, subs: Vec<(&str, &Value, bool)>) -> Vec<Environment> {
    let mut base_vars = Vec::new();
    let base_private = base.is_some_and(|(_, private)| private);
    if let Some((data, _)) = base {
        flatten("", data, &mut base_vars);
    }

    let to_vars = |pairs: Vec<(String, String)>, private: bool| -> Vec<EnvVar> {
        pairs.into_iter().map(|(name, value)| EnvVar { name, value, secret: private }).collect()
    };

    if subs.is_empty() {
        return if base_vars.is_empty() {
            Vec::new()
        } else {
            vec![Environment { name: "local".into(), vars: to_vars(base_vars, base_private) }]
        };
    }

    subs.into_iter()
        .map(|(name, data, private)| {
            let mut merged = base_vars.clone();
            let mut own = Vec::new();
            flatten("", data, &mut own);
            for (key, value) in own {
                match merged.iter_mut().find(|(existing, _)| *existing == key) {
                    Some(slot) => slot.1 = value,
                    None => merged.push((key, value)),
                }
            }
            Environment { name: non_empty(name, "environment"), vars: to_vars(merged, private || base_private) }
        })
        .collect()
}

/// Nested environment data (`{ "aws": { "region": … } }`, used as
/// `{{ _.aws.region }}`) becomes flat dotted names.
fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let name = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                flatten(&name, child, out);
            }
        }
        Value::Null if prefix.is_empty() => {}
        Value::String(text) => out.push((prefix.to_string(), insomnia_vars(text))),
        other => out.push((prefix.to_string(), text_of(Some(other)))),
    }
}

// ---------------------------------------------------------------------------
// Shared clean-up: environment names and credentials
// ---------------------------------------------------------------------------

fn finish(imported: &mut Imported) {
    unique_environment_names(&mut imported.environments);

    for env in &mut imported.environments {
        for var in &mut env.vars {
            if looks_secret(&var.name) {
                var.secret = true;
            }
        }
    }

    move_literal_secrets(imported);

    if imported.environments.is_empty() {
        imported.environments.push(Environment { name: "local".into(), vars: Vec::new() });
    }

    warn_about_credentials_in_bodies(&imported.nodes, &mut imported.warnings);
}

/// Environment files are named by `slug`, so "Dev" and "dev" would overwrite
/// each other on disk.
fn unique_environment_names(environments: &mut [Environment]) {
    let mut seen = HashSet::new();
    for env in environments {
        let original = env.name.clone();
        let mut candidate = original.clone();
        let mut n = 2;
        while !seen.insert(collection::slug(&candidate)) {
            candidate = format!("{original} {n}");
            n += 1;
        }
        env.name = candidate;
    }
}

/// Deliberately generous: a false positive only moves a value out of the YAML.
fn looks_secret(name: &str) -> bool {
    let normalised: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    ["token", "secret", "password", "passwd", "apikey", "privatekey", "credential"]
        .iter()
        .any(|word| normalised.contains(word))
}

fn is_literal(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains("{{")
}

fn placeholder(name: &str) -> String {
    format!("{{{{{name}}}}}")
}

/// Hands out variable names for literal credentials: the same value always
/// gets the same name, and a name already holding a different value is skipped.
struct SecretNames {
    by_value: HashMap<String, String>,
    taken: HashMap<String, String>,
    /// Every name a credential was rewritten to, new or pre-existing.
    used: HashSet<String>,
    /// Names that did not exist yet and need adding to the environments.
    created: Vec<(String, String)>,
}

impl SecretNames {
    fn new(environments: &[Environment]) -> Self {
        let mut by_value = HashMap::new();
        let mut taken = HashMap::new();
        for var in environments.iter().flat_map(|e| &e.vars) {
            taken.entry(var.name.clone()).or_insert_with(|| var.value.clone());
            if is_literal(&var.value) {
                by_value.entry(var.value.clone()).or_insert_with(|| var.name.clone());
            }
        }
        SecretNames { by_value, taken, used: HashSet::new(), created: Vec::new() }
    }

    fn name_for(&mut self, base: &str, value: &str) -> String {
        if let Some(existing) = self.by_value.get(value) {
            self.used.insert(existing.clone());
            return existing.clone();
        }
        let mut name = base.to_string();
        let mut n = 2;
        while self.taken.contains_key(&name) {
            name = format!("{base}{n}");
            n += 1;
        }
        self.taken.insert(name.clone(), value.to_string());
        self.by_value.insert(value.to_string(), name.clone());
        self.used.insert(name.clone());
        self.created.push((name.clone(), value.to_string()));
        name
    }
}

fn secret_base(name: &str) -> &'static str {
    let normalised = name.to_ascii_lowercase().replace(['-', '_', '.'], "");
    if normalised.contains("apikey") {
        "apiKey"
    } else if normalised.contains("password") || normalised.contains("passwd") {
        "password"
    } else if normalised.contains("secret") {
        "clientSecret"
    } else {
        "token"
    }
}

fn move_literal_secrets(imported: &mut Imported) {
    let mut names = SecretNames::new(&imported.environments);

    scrub_auth(&mut imported.auth, &mut names);
    scrub_pairs(&mut imported.headers, &mut names);
    scrub_nodes(&mut imported.nodes, &mut names);

    // An existing variable that turned out to hold a credential becomes secret.
    for env in &mut imported.environments {
        for var in &mut env.vars {
            if names.used.contains(&var.name) {
                var.secret = true;
            }
        }
    }

    if names.created.is_empty() {
        return;
    }
    if imported.environments.is_empty() {
        imported.environments.push(Environment { name: "local".into(), vars: Vec::new() });
    }
    for env in &mut imported.environments {
        for (name, value) in &names.created {
            if !env.vars.iter().any(|v| &v.name == name) {
                env.vars.push(EnvVar { name: name.clone(), value: value.clone(), secret: true });
            }
        }
    }
}

fn scrub_nodes(nodes: &mut [ImportNode], names: &mut SecretNames) {
    for node in nodes {
        match node {
            // A folder holds credentials now, so it is scrubbed like a request.
            ImportNode::Folder { auth, headers, children, .. } => {
                scrub_auth(auth, names);
                scrub_pairs(headers, names);
                scrub_nodes(children, names);
            }
            ImportNode::Request(request) => scrub_request(request, names),
        }
    }
}

fn scrub_request(request: &mut Request, names: &mut SecretNames) {
    scrub_auth(&mut request.auth, names);
    scrub_pairs(&mut request.headers, names);
    scrub_pairs(&mut request.params, names);
    match &mut request.body {
        Body::UrlEncoded { fields } => scrub_pairs(fields, names),
        Body::Form { fields } => scrub_form(fields, names),
        _ => {}
    }
}

/// The same credential rule for one request arriving outside an import (a
/// pasted curl command): literal credentials become `{{name}}`, and the new
/// secret variables to add come back. A value an existing variable already
/// holds reuses that variable's name; names never clash with existing ones.
pub(crate) fn extract_request_secrets(request: &mut Request, existing: &[EnvVar]) -> Vec<EnvVar> {
    let environment = Environment { name: String::new(), vars: existing.to_vec() };
    let mut names = SecretNames::new(std::slice::from_ref(&environment));
    scrub_request(request, &mut names);
    names
        .created
        .into_iter()
        .map(|(name, value)| EnvVar { name, value, secret: true })
        .collect()
}

fn take(names: &mut SecretNames, base: &str, value: &str) -> String {
    placeholder(&names.name_for(base, value))
}

fn scrub_auth(auth: &mut Auth, names: &mut SecretNames) {
    match auth {
        Auth::Bearer { token } if is_literal(token) => *token = take(names, "token", token),
        Auth::Basic { password, .. } if is_literal(password) => *password = take(names, "password", password),
        Auth::ApiKey { value, .. } if is_literal(value) => *value = take(names, "apiKey", value),
        Auth::Digest { password, .. } if is_literal(password) => *password = take(names, "password", password),
        Auth::Ntlm { password, .. } if is_literal(password) => *password = take(names, "password", password),
        // The secret key is the credential; the key id is an identifier, and a
        // session token is one too short-lived to be worth a variable.
        Auth::AwsSigV4 { secret, .. } if is_literal(secret) => *secret = take(names, "awsSecret", secret),
        _ => {}
    }
}

/// A text form field. Only Postman marks a part as a file; Insomnia's text
/// fields come through here.
fn form_field(pair: KeyValue) -> FormField {
    FormField { name: pair.name, value: pair.value, enabled: pair.enabled, description: pair.description, file: false }
}

/// Form fields hide credentials as readily as headers do.
fn scrub_form(fields: &mut [FormField], names: &mut SecretNames) {
    let mut pairs: Vec<KeyValue> = fields
        .iter()
        .map(|f| KeyValue { name: f.name.clone(), value: f.value.clone(), enabled: f.enabled, description: f.description.clone() })
        .collect();
    scrub_pairs(&mut pairs, names);
    for (field, pair) in fields.iter_mut().zip(pairs) {
        // A file field holds a path, which is not a credential to hide.
        if !field.file {
            field.value = pair.value;
        }
    }
}

/// credential. `Authorization: Bearer abc` keeps its scheme readable.
fn scrub_pairs(pairs: &mut [KeyValue], names: &mut SecretNames) {
    for pair in pairs {
        let lower = pair.name.to_ascii_lowercase();
        if lower == "authorization" || lower == "proxy-authorization" {
            let (scheme, credential) = match pair.value.trim().split_once(' ') {
                Some((scheme, rest)) if !rest.trim().is_empty() => (Some(scheme.to_string()), rest.trim().to_string()),
                _ => (None, pair.value.trim().to_string()),
            };
            if !is_literal(&credential) {
                continue;
            }
            let base = if scheme.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("basic")) { "basicCredentials" } else { "token" };
            let replacement = take(names, base, &credential);
            pair.value = match scheme {
                Some(scheme) => format!("{scheme} {replacement}"),
                None => replacement,
            };
        } else if lower == "cookie" && is_literal(&pair.value) {
            // Cookies copied from a browser are usually a live session.
            pair.value = take(names, "cookie", &pair.value.clone());
        } else if looks_secret(&pair.name) && is_literal(&pair.value) {
            let base = secret_base(&pair.name);
            pair.value = take(names, base, &pair.value.clone());
        }
    }
}

/// A JSON body like `{"password": "hunter2"}` cannot be rewritten safely, so
/// point at it instead.
pub(crate) fn json_has_literal_secret(content: &str) -> bool {
    fn walk(value: &Value) -> bool {
        match value {
            Value::Object(map) => map
                .iter()
                .any(|(key, child)| (looks_secret(key) && child.as_str().is_some_and(is_literal)) || walk(child)),
            Value::Array(items) => items.iter().any(walk),
            _ => false,
        }
    }
    serde_json::from_str::<Value>(content).is_ok_and(|v| walk(&v))
}

fn warn_about_credentials_in_bodies(nodes: &[ImportNode], w: &mut Warnings) {
    for node in nodes {
        match node {
            ImportNode::Folder { children, .. } => warn_about_credentials_in_bodies(children, w),
            ImportNode::Request(request) => {
                if let Body::Json { content } = &request.body {
                    if json_has_literal_secret(content) {
                        w.add(
                            "These JSON bodies appear to contain a plain-text credential; move it into a secret variable before committing",
                            &request.name,
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Create a new collection directory inside `into` and return its path. A
/// half-written directory is removed if anything fails.
pub fn write(into: &Path, imported: &Imported) -> Result<PathBuf> {
    if !into.is_dir() {
        return Err(Error::Invalid(format!("`{}` is not a folder", into.to_string_lossy())));
    }

    let base = collection::folder_dir_name(&imported.name);
    let mut dir_name = base.clone();
    let mut n = 2;
    while into.join(&dir_name).exists() {
        dir_name = format!("{base}-{n}");
        n += 1;
    }

    let root = into.join(dir_name);
    fs::create_dir(&root).map_err(|e| Error::io(root.to_string_lossy(), e))?;

    match write_contents(&root, imported) {
        Ok(()) => Ok(root),
        Err(e) => {
            fs::remove_dir_all(&root).ok();
            Err(e)
        }
    }
}

fn write_contents(root: &Path, imported: &Imported) -> Result<()> {
    let meta = CollectionMeta {
        name: imported.name.clone(),
        version: 1,
        headers: imported.headers.clone(),
        auth: imported.auth.clone(),
        vars: Vec::new(),
    };
    collection::write_yaml(&root.join(COLLECTION_FILE), &meta)?;
    collection::ensure_gitignore(root)?;

    let env_dir = root.join(ENV_DIR);
    fs::create_dir_all(&env_dir).map_err(|e| Error::io(env_dir.to_string_lossy(), e))?;
    for env in &imported.environments {
        collection::write_environment(root, env, None)?;
    }

    write_nodes(root, root, &imported.nodes)
}

fn write_nodes(dir: &Path, root: &Path, nodes: &[ImportNode]) -> Result<()> {
    let mut taken: HashSet<String> = HashSet::from([FOLDER_FILE.to_string()]);
    if dir == root {
        taken.insert(COLLECTION_FILE.to_string());
        taken.insert(ENV_DIR.to_string());
    }

    for (index, node) in nodes.iter().enumerate() {
        let seq = index as u32 + 1;
        match node {
            ImportNode::Folder { name, auth, headers, children } => {
                let dir_name = claim(&mut taken, &collection::folder_dir_name(name), "");
                let path = dir.join(&dir_name);
                fs::create_dir(&path).map_err(|e| Error::io(path.to_string_lossy(), e))?;

                let renamed = dir_name != *name;
                let carries = !matches!(auth, Auth::Inherit) || !headers.is_empty();
                if renamed || seq != 1 || carries {
                    let meta = FolderMeta {
                        name: renamed.then(|| name.clone()),
                        seq,
                        headers: headers.clone(),
                        auth: auth.clone(),
                        vars: Vec::new(),
                    };
                    collection::write_yaml(&path.join(FOLDER_FILE), &meta)?;
                }
                write_nodes(&path, root, children)?;
            }
            ImportNode::Request(request) => {
                let file = claim(&mut taken, &collection::folder_dir_name(&request.name), ".yaml");
                let request = Request { seq, ..(**request).clone() };
                collection::write_yaml(&dir.join(file), &request)?;
            }
        }
    }
    Ok(())
}

fn claim(taken: &mut HashSet<String>, base: &str, extension: &str) -> String {
    let mut candidate = format!("{base}{extension}");
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{base}-{n}{extension}");
        n += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSTMAN_V21: &str = r##"{
      "info": {
        "name": "Shop API",
        "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
      },
      "auth": { "type": "apikey", "apikey": [
        { "key": "key", "value": "X-API-Key", "type": "string" },
        { "key": "value", "value": "live_abc123", "type": "string" },
        { "key": "in", "value": "header", "type": "string" }
      ]},
      "variable": [
        { "key": "baseUrl", "value": "https://shop.test" },
        { "key": "clientSecret", "value": "cs_999" },
        { "key": "old", "value": "x", "disabled": true }
      ],
      "item": [
        {
          "name": "Auth",
          "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "eyJhbGciOi.folder" }] },
          "item": [
            {
              "name": "Log in",
              "event": [
                { "listen": "prerequest", "script": { "exec": ["console.log('before')"] } },
                {
                  "listen": "test",
                  "script": {
                    "exec": [
                      "// Keep the token for the requests after this one.",
                      "var jsonData = pm.response.json();",
                      "pm.environment.set(\"token\", jsonData.data.token);",
                      "pm.test(\"status is 200\", function () {",
                      "    pm.response.to.have.status(200);",
                      "});",
                      "pm.expect(pm.response.responseTime).to.be.below(500);",
                      "for (const row of jsonData.data.rows) { pm.expect(row.id).to.exist; }"
                    ]
                  }
                }
              ],
              "request": {
                "method": "POST",
                "header": [{ "key": "Content-Type", "value": "application/json" }],
                "body": { "mode": "raw", "raw": "{\"user\": \"ada\", \"password\": \"hunter2\"}",
                          "options": { "raw": { "language": "json" } } },
                "url": "{{baseUrl}}/login",
                "auth": { "type": "noauth" }
              }
            },
            {
              "name": "Me",
              "request": { "method": "GET", "url": { "raw": "{{baseUrl}}/me" } }
            }
          ]
        },
        {
          "name": "Users",
          "item": [
            {
              "name": "List users",
              "request": {
                "method": "get",
                "url": {
                  "raw": "{{baseUrl}}/users?limit=10&page=2",
                  "host": ["{{baseUrl}}"], "path": ["users"],
                  "query": [
                    { "key": "limit", "value": "10" },
                    { "key": "page", "value": "2", "disabled": true }
                  ]
                },
                "description": "Paged."
              }
            },
            {
              "name": "Get user",
              "request": {
                "method": "GET",
                "url": { "raw": "{{baseUrl}}/users/:id", "variable": [{ "key": "id", "value": "42" }] },
                "header": [{ "key": "X-Trace", "value": "{{$guid}}" }]
              }
            },
            {
              "name": "Upload avatar",
              "request": {
                "method": "POST",
                "url": "{{baseUrl}}/avatar",
                "body": { "mode": "formdata", "formdata": [
                  { "key": "file", "type": "file", "src": "/Users/ada/me.png" },
                  { "key": "caption", "value": "me", "type": "text" }
                ]}
              }
            },
            {
              "name": "Get user",
              "request": { "method": "GET", "url": "{{baseUrl}}/users/me?token=tok_literal" }
            }
          ]
        },
        {
          "name": "Search",
          "request": {
            "method": "POST",
            "url": "{{baseUrl}}/graphql",
            "body": { "mode": "graphql", "graphql": { "query": "{ products { id } }", "variables": "{\"first\": 3}" } },
            "auth": { "type": "oauth2", "oauth2": [] }
          }
        },
        {
          "name": "Folder",
          "request": { "method": "GET", "url": "{{baseUrl}}/folder" }
        }
      ]
    }"##;

    fn names(nodes: &[ImportNode]) -> Vec<&str> {
        nodes
            .iter()
            .map(|n| match n {
                ImportNode::Folder { name, .. } => name.as_str(),
                ImportNode::Request(r) => r.name.as_str(),
            })
            .collect()
    }

    fn folder<'a>(nodes: &'a [ImportNode], name: &str) -> &'a [ImportNode] {
        nodes
            .iter()
            .find_map(|n| match n {
                ImportNode::Folder { name: n, children, .. } if n == name => Some(children.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no folder {name}"))
    }

    fn request<'a>(nodes: &'a [ImportNode], name: &str) -> &'a Request {
        nodes
            .iter()
            .find_map(|n| match n {
                ImportNode::Request(r) if r.name == name => Some(r),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    fn var<'a>(env: &'a Environment, name: &str) -> &'a EnvVar {
        env.vars.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no var {name} in {:?}", env.vars))
    }

    #[test]
    fn postman_structure_order_and_requests() {
        let imported = parse(POSTMAN_V21).unwrap();
        assert_eq!(imported.format, "Postman v2.1");
        assert_eq!(imported.name, "Shop API");
        assert_eq!(names(&imported.nodes), ["Auth", "Users", "Search", "Folder"]);
        assert_eq!(names(folder(&imported.nodes, "Users")), ["List users", "Get user", "Upload avatar", "Get user"]);

        let users = folder(&imported.nodes, "Users");
        let list = request(users, "List users");
        assert_eq!(list.method, "GET", "method is upper-cased");
        assert_eq!(list.url, "{{baseUrl}}/users", "the query string moves into params");
        assert_eq!(list.params.len(), 2);
        assert!(list.params[0].enabled && !list.params[1].enabled, "disabled params survive as disabled");
        assert_eq!(list.docs.as_deref(), Some("Paged."));

        assert_eq!(request(users, "Get user").url, "{{baseUrl}}/users/42", "path variables are filled in");

        let Body::Form { fields } = &request(users, "Upload avatar").body else { panic!("expected form") };
        assert_eq!(fields.len(), 2, "the file field is kept, as a path");
        let by_name = |name: &str| fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no {name}"));
        assert!(!by_name("caption").file);
        let file = by_name("file");
        assert!(file.file && !file.value.is_empty(), "{file:?}");

        let Body::Json { content } = &request(&imported.nodes, "Search").body else { panic!("expected json") };
        let graphql: Value = serde_json::from_str(content).unwrap();
        assert_eq!(graphql["query"], "{ products { id } }");
        assert_eq!(graphql["variables"]["first"], 3);
    }

    #[test]
    fn postman_auth_lands_on_the_folder_that_declared_it() {
        let imported = parse(POSTMAN_V21).unwrap();
        let auth_folder = folder_node(&imported.nodes, "Auth");
        let ImportNode::Folder { auth, .. } = auth_folder else { panic!("a folder") };

        // The folder keeps its own auth now that `folder.yaml` can hold it.
        assert!(matches!(auth, Auth::Bearer { .. }), "{auth:?}");

        let inside = folder(&imported.nodes, "Auth");
        // "Me" declares nothing, so it inherits — from the folder, at send time.
        assert!(matches!(request(inside, "Me").auth, Auth::Inherit));
        // "Log in" explicitly opts out.
        assert!(matches!(request(inside, "Log in").auth, Auth::None));
        assert!(matches!(request(folder(&imported.nodes, "Users"), "List users").auth, Auth::Inherit));
        assert!(matches!(imported.auth, Auth::ApiKey { location: ApiKeyLocation::Header, .. }));
        // Unsupported auth falls back instead of failing.
        assert!(matches!(request(&imported.nodes, "Search").auth, Auth::Inherit));
    }

    /// The folder it came from, not just its children.
    fn folder_node<'a>(nodes: &'a [ImportNode], name: &str) -> &'a ImportNode {
        nodes
            .iter()
            .find(|node| matches!(node, ImportNode::Folder { name: n, .. } if n == name))
            .unwrap_or_else(|| panic!("no folder `{name}`"))
    }

    #[test]
    fn the_auth_schemes_volt_speaks_are_imported_rather_than_reported() {
        let export = r#"{
          "info": { "name": "Schemes", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
          "item": [
            { "name": "D", "request": { "method": "GET", "url": "https://x.test/d",
              "auth": { "type": "digest", "digest": [
                { "key": "username", "value": "ada" }, { "key": "password", "value": "hunter2secret" }] } } },
            { "name": "N", "request": { "method": "GET", "url": "https://x.test/n",
              "auth": { "type": "ntlm", "ntlm": [
                { "key": "username", "value": "ada" }, { "key": "password", "value": "hunter2secret" },
                { "key": "domain", "value": "CORP" }] } } },
            { "name": "A", "request": { "method": "GET", "url": "https://x.test/a",
              "auth": { "type": "awsv4", "awsv4": [
                { "key": "accessKey", "value": "AKIDEXAMPLE" }, { "key": "secretKey", "value": "wJalrXUtnFEMIK7MDENG" },
                { "key": "region", "value": "eu-west-1" }, { "key": "service", "value": "execute-api" }] } } }
          ]
        }"#;

        let imported = parse(export).unwrap();
        let env = &imported.environments[0];

        let digest = &request(&imported.nodes, "D").auth;
        assert!(
            matches!(digest, Auth::Digest { username, password } if username == "ada" && password == "{{password}}"),
            "{digest:?}"
        );
        assert_eq!(var(env, "password").value, "hunter2secret");
        assert!(var(env, "password").secret);

        let ntlm = &request(&imported.nodes, "N").auth;
        assert!(matches!(ntlm, Auth::Ntlm { domain, .. } if domain == "CORP"), "{ntlm:?}");

        let aws = &request(&imported.nodes, "A").auth;
        let Auth::AwsSigV4 { key_id, secret, region, service, .. } = aws else { panic!("{aws:?}") };
        assert_eq!((key_id.as_str(), region.as_str(), service.as_str()), ("AKIDEXAMPLE", "eu-west-1", "execute-api"));
        assert_eq!(secret, "{{awsSecret}}", "the secret key is the credential, the key id is not");
        assert_eq!(var(env, "awsSecret").value, "wJalrXUtnFEMIK7MDENG");

        assert!(
            !imported.warnings.render().join(" ").contains("not supported"),
            "{:?}",
            imported.warnings.render()
        );
    }

    #[test]
    fn literal_credentials_are_moved_into_secret_variables() {
        let imported = parse(POSTMAN_V21).unwrap();
        let env = &imported.environments[0];

        let Auth::ApiKey { value, .. } = &imported.auth else { panic!() };
        assert_eq!(value, "{{apiKey}}");
        assert_eq!(var(env, "apiKey").value, "live_abc123");
        assert!(var(env, "apiKey").secret);

        // The folder holds the token now, and it is scrubbed there.
        let ImportNode::Folder { auth, .. } = folder_node(&imported.nodes, "Auth") else { panic!() };
        let Auth::Bearer { token } = auth else { panic!("{auth:?}") };
        assert_eq!(token, "{{token}}");
        assert_eq!(var(env, "token").value, "eyJhbGciOi.folder");

        // A query parameter named like a credential, with a clashing name.
        let second_get = folder(&imported.nodes, "Users")
            .iter()
            .filter_map(|n| match n { ImportNode::Request(r) if r.name == "Get user" => Some(r), _ => None })
            .nth(1)
            .unwrap();
        assert_eq!(second_get.params[0].value, "{{token2}}", "a different value does not reuse `token`");
        assert_eq!(var(env, "token2").value, "tok_literal");

        // Names that look secret are marked; ordinary ones are not.
        assert!(var(env, "clientSecret").secret);
        assert!(!var(env, "baseUrl").secret);
        assert!(env.vars.iter().all(|v| v.name != "old"), "disabled variables are skipped");
    }

    #[test]
    fn postman_warnings_say_what_was_left_behind() {
        let w = parse(POSTMAN_V21).unwrap().warnings;
        assert!(w.mentions("Pre-request scripts are not imported"), "{:?}", w.render());
        assert!(w.mentions("Test scripts were read as checks and captures: Log in"));
        assert!(
            w.render().iter().any(|line| line.contains("A line of a test script was not imported")
                && line.contains("for (const row of")),
            "the line that was not understood is quoted: {:?}",
            w.render()
        );
        assert!(w.mentions("A file field keeps the path"));
        assert!(w.mentions("`oauth2` auth is not supported"));
        assert!(w.mentions("GraphQL"));
        assert!(w.mentions("dynamic variables"));
        assert!(
            w.render().iter().any(|line| line.contains("plain-text credential") && line.ends_with(": Log in")),
            "the JSON password is flagged against the right request: {:?}",
            w.render()
        );
    }

    #[test]
    fn the_shapes_a_test_script_shares_with_volt_are_imported() {
        let imported = parse(POSTMAN_V21).unwrap();
        let login = request(folder(&imported.nodes, "Auth"), "Log in");

        assert_eq!(login.captures.len(), 1);
        assert_eq!(login.captures[0].name, "token");
        assert_eq!(login.captures[0].from, "$.data.token");
        assert!(login.captures[0].secret, "what a script captures is a token until proved otherwise");

        assert_eq!(login.checks.len(), 2);
        assert_eq!((login.checks[0].from.as_str(), login.checks[0].value.as_str()), ("status", "200"));
        assert_eq!(login.checks[0].op, crate::checks::Op::Is);
        assert_eq!((login.checks[1].from.as_str(), login.checks[1].value.as_str()), ("time", "500"));
        assert_eq!(login.checks[1].op, crate::checks::Op::Under);
    }

    #[test]
    fn postman_v20_auth_objects_and_v1_rejection() {
        let v20 = r#"{
          "info": { "name": "Old", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json" },
          "item": [{ "name": "R", "request": {
            "url": "https://x.test", "method": "GET",
            "auth": { "type": "basic", "basic": { "username": "ada", "password": "{{pw}}" } }
          }}]
        }"#;
        let imported = parse(v20).unwrap();
        assert_eq!(imported.format, "Postman v2.0");
        let Auth::Basic { username, password } = &request(&imported.nodes, "R").auth else { panic!() };
        assert_eq!((username.as_str(), password.as_str()), ("ada", "{{pw}}"), "a templated password stays as it is");

        let v1 = r#"{ "id": "1", "name": "Ancient", "order": [], "requests": [] }"#;
        let err = parse(v1).unwrap_err().to_string();
        assert!(err.contains("v2.1"), "{err}");

        assert!(parse(r#"{"hello": "world"}"#).unwrap_err().to_string().contains("not a Postman"));
        assert!(parse("{ not json").unwrap_err().to_string().contains("not valid JSON"));
    }

    const INSOMNIA_V4: &str = r##"{
      "_type": "export",
      "__export_format": 4,
      "resources": [
        { "_id": "wrk_1", "_type": "workspace", "name": "Payments" },
        { "_id": "req_b", "_type": "request", "parentId": "fld_1", "name": "Refund", "metaSortKey": 20,
          "method": "POST", "url": "{{ _.baseUrl }}/refunds",
          "body": { "mimeType": "application/x-www-form-urlencoded",
                    "params": [{ "name": "amount", "value": "{{ _.amount }}" }, { "name": "note", "value": "x", "disabled": true }] },
          "headers": [{ "name": "X-Old", "value": "1", "disabled": true }],
          "authentication": {} },
        { "_id": "req_a", "_type": "request", "parentId": "fld_1", "name": "Charge", "metaSortKey": 10,
          "method": "POST", "url": "{{ _.baseUrl }}/charges",
          "body": { "mimeType": "application/json", "text": "{\"amount\": {{ _.amount }}}" },
          "authentication": { "type": "apikey", "key": "api_key", "value": "sk_test_1", "addTo": "queryParams" } },
        { "_id": "fld_1", "_type": "request_group", "parentId": "wrk_1", "name": "Stripe", "metaSortKey": 1,
          "headers": [{ "name": "Stripe-Version", "value": "2024-01-01" }],
          "authentication": { "type": "bearer", "token": "rk_live_folder", "prefix": "Token" } },
        { "_id": "grpc_1", "_type": "grpc_request", "parentId": "wrk_1", "name": "Stream", "metaSortKey": 5 },
        { "_id": "req_c", "_type": "request", "parentId": "wrk_1", "name": "Health", "metaSortKey": 0,
          "method": "GET", "url": "{{ _.baseUrl }}/health?verbose=1",
          "parameters": [{ "name": "region", "value": "{{ _.aws.region }}" }] },
        { "_id": "env_base", "_type": "environment", "parentId": "wrk_1", "name": "Base Environment",
          "data": { "baseUrl": "https://pay.test", "amount": 100, "aws": { "region": "eu-west-1" } } },
        { "_id": "env_dev", "_type": "environment", "parentId": "env_base", "name": "Dev",
          "data": { "baseUrl": "https://dev.pay.test", "apiToken": "dev-token" } },
        { "_id": "env_prod", "_type": "environment", "parentId": "env_base", "name": "prod",
          "data": { "apiToken": "prod-token" } },
        { "_id": "env_dup", "_type": "environment", "parentId": "env_base", "name": "DEV", "data": {} },
        { "_id": "jar_1", "_type": "cookie_jar", "parentId": "wrk_1", "name": "Jar" }
      ]
    }"##;

    #[test]
    fn insomnia_v4_structure_variables_and_inheritance() {
        let imported = parse(INSOMNIA_V4).unwrap();
        assert_eq!(imported.format, "Insomnia v4");
        assert_eq!(imported.name, "Payments");
        // Sorted by metaSortKey, not file order; gRPC is skipped.
        assert_eq!(names(&imported.nodes), ["Health", "Stripe"]);
        let stripe = folder(&imported.nodes, "Stripe");
        assert_eq!(names(stripe), ["Charge", "Refund"]);

        let health = request(&imported.nodes, "Health");
        assert_eq!(health.url, "{{baseUrl}}/health?verbose=1", "Insomnia keeps its query in the URL");
        assert_eq!(health.params[0].value, "{{aws.region}}", "nested variables become dotted names");

        let charge = request(stripe, "Charge");
        assert_eq!(charge.url, "{{baseUrl}}/charges");
        assert!(matches!(&charge.body, Body::Json { content } if content == "{\"amount\": {{amount}}}"));
        let Auth::ApiKey { key, value, location } = &charge.auth else { panic!("{:?}", charge.auth) };
        assert_eq!((key.as_str(), value.as_str()), ("api_key", "{{apiKey}}"));
        assert!(matches!(location, ApiKeyLocation::Query));

        // What the folder adds now lives on the folder: its header and the
        // custom-prefix bearer token that became an Authorization header.
        let ImportNode::Folder { headers, .. } = folder_node(&imported.nodes, "Stripe") else { panic!() };
        assert!(headers.iter().any(|h| h.name == "Stripe-Version"));
        let authz = headers.iter().find(|h| h.name == "Authorization").expect("prefix header");
        assert_eq!(authz.value, "Token {{token}}");

        let refund = request(stripe, "Refund");
        assert!(matches!(refund.auth, Auth::Inherit), "{:?}", refund.auth);
        assert!(refund.headers.iter().any(|h| h.name == "X-Old" && !h.enabled));
        let Body::UrlEncoded { fields } = &refund.body else { panic!() };
        assert_eq!(fields[0].value, "{{amount}}");
        assert!(!fields[1].enabled);

        assert!(imported.warnings.mentions("gRPC and WebSocket requests are not supported and were skipped: Stream"));
    }

    #[test]
    fn insomnia_v4_environments_are_merged_and_deduplicated() {
        let imported = parse(INSOMNIA_V4).unwrap();
        let names: Vec<&str> = imported.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Dev", "prod", "DEV 2"], "`DEV` would overwrite `Dev` on disk");

        let dev = &imported.environments[0];
        assert_eq!(var(dev, "baseUrl").value, "https://dev.pay.test", "sub-environment wins");
        assert_eq!(var(dev, "amount").value, "100", "numbers become strings");
        assert_eq!(var(dev, "aws.region").value, "eu-west-1");
        assert!(var(dev, "apiToken").secret);

        let prod = &imported.environments[1];
        assert_eq!(var(prod, "baseUrl").value, "https://pay.test", "base value underneath");

        // Literal credentials land in every environment.
        for env in &imported.environments {
            assert!(var(env, "token").secret && var(env, "apiKey").secret);
        }
        assert_eq!(var(dev, "apiToken").value, "dev-token");
        assert_eq!(var(prod, "apiToken").value, "prod-token", "each environment keeps its own secret");
    }

    #[test]
    fn per_environment_secrets_survive_the_trip_to_disk() {
        let into = temp("per-env");
        let source = into.join("insomnia.json");
        fs::write(&source, INSOMNIA_V4).unwrap();

        let outcome = import_file(&source, &into).unwrap();
        let root = PathBuf::from(&outcome.report.path);

        let token = |env: &str| {
            outcome
                .collection
                .environments
                .iter()
                .find(|e| e.name == env)
                .and_then(|e| e.vars.iter().find(|v| v.name == "apiToken"))
                .map(|v| v.value.clone())
        };
        assert_eq!(token("Dev").as_deref(), Some("dev-token"));
        assert_eq!(token("prod").as_deref(), Some("prod-token"));

        assert!(fs::read_to_string(root.join(".env.dev")).unwrap().contains("dev-token"));
        assert!(fs::read_to_string(root.join(".env.prod")).unwrap().contains("prod-token"));
        assert!(!root.join(".env").exists(), "no shared file for a new collection");

        fs::remove_dir_all(&into).ok();
    }

    const INSOMNIA_V5: &str = r#"
type: collection.insomnia.rest/5.0
schema_version: "5.1"
name: Weather
meta:
  id: wrk_1
collection:
  - name: Later
    url: "{{ _.host }}/later"
    method: get
    meta:
      id: req_2
      sortKey: -1
  - name: Forecasts
    meta:
      id: fld_1
      sortKey: -5
    children:
      - name: Today
        url: "{{ _.host }}/today"
        method: GET
        headers:
          - name: Accept
            value: application/json
        authentication:
          type: basic
          username: ada
          password: pa55word
        scripts:
          preRequest: "insomnia.environment.set('x', 1)"
        meta:
          id: req_1
  - name: Live
    url: wss://weather.test/live
    meta:
      id: ws-req_1
environments:
  name: Base Environment
  data:
    host: https://weather.test
  subEnvironments:
    - name: Staging
      data:
        host: https://staging.weather.test
      meta:
        id: env_1
"#;

    #[test]
    fn insomnia_v5_yaml() {
        let imported = parse(INSOMNIA_V5).unwrap();
        assert_eq!(imported.format, "Insomnia v5");
        assert_eq!(names(&imported.nodes), ["Forecasts", "Later"], "sorted by meta.sortKey");

        let today = request(folder(&imported.nodes, "Forecasts"), "Today");
        assert_eq!(today.url, "{{host}}/today");
        let Auth::Basic { username, password } = &today.auth else { panic!() };
        assert_eq!((username.as_str(), password.as_str()), ("ada", "{{password}}"));
        assert_eq!(request(&imported.nodes, "Later").method, "GET");

        assert_eq!(imported.environments.len(), 1);
        assert_eq!(imported.environments[0].name, "Staging");
        assert_eq!(var(&imported.environments[0], "host").value, "https://staging.weather.test");
        assert_eq!(var(&imported.environments[0], "password").value, "pa55word");

        assert!(
            imported.warnings.render().iter().any(|line| line
                .starts_with("Pre-request scripts are not imported")
                && line.ends_with(": Today")),
            "{:?}",
            imported.warnings.render()
        );
        assert!(imported.warnings.mentions("were skipped: Live"));
    }

    #[test]
    fn a_non_collection_insomnia_file_is_explained() {
        let err = parse("type: spec.insomnia.rest/5.0\nname: x\n").unwrap_err().to_string();
        assert!(err.contains("not a request collection"), "{err}");
    }

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("volt-import-{label}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn all_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(all_files(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn an_imported_folder_keeps_its_own_headers_and_auth_on_disk() {
        let into = temp("folder-meta");
        let imported = parse(INSOMNIA_V4).unwrap();
        let root = write(&into, &imported).unwrap();

        // The folder that added a header and a token has a folder.yaml saying so,
        // and the requests inside are left inheriting rather than rewritten.
        let loaded = collection::load(&root).unwrap();
        let folder = loaded
            .tree
            .iter()
            .find_map(|node| match node {
                collection::Node::Folder { id, name, .. } if name == "Stripe" => Some(id.clone()),
                _ => None,
            })
            .expect("the Stripe folder");

        let meta = collection::folder_meta(&root, &folder).unwrap();
        assert!(meta.headers.iter().any(|h| h.name == "Stripe-Version"), "{:?}", meta.headers);
        assert!(meta.headers.iter().any(|h| h.name == "Authorization" && h.value.contains("{{token}}")));

        let text = fs::read_to_string(root.join(&folder).join("folder.yaml")).unwrap();
        assert!(!text.contains("dev-token"), "a secret reached folder.yaml: {text}");

        fs::remove_dir_all(&into).ok();
    }

    #[test]
    fn writing_produces_a_loadable_collection_with_no_secret_in_any_yaml() {
        let into = temp("write");
        let source = into.join("export.json");
        fs::write(&source, POSTMAN_V21).unwrap();

        let outcome = import_file(&source, &into).unwrap();
        let root = PathBuf::from(&outcome.report.path);
        assert_eq!(root, into.join("shop-api"));
        assert_eq!(outcome.collection.meta.name, "Shop API");
        assert_eq!((outcome.report.requests, outcome.report.folders), (8, 2));
        assert!(outcome.report.secrets.contains(&"apiKey".to_string()));

        // The collection loads, in the original order, through the normal path.
        let tree = &outcome.collection.tree;
        let top: Vec<&str> = tree
            .iter()
            .map(|n| match n {
                collection::Node::Folder { name, .. } | collection::Node::Request { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(top, ["Auth", "Users", "Search", "Folder"]);

        // Duplicate and reserved names get their own files.
        assert!(root.join("users/get-user.yaml").is_file());
        assert!(root.join("users/get-user-2.yaml").is_file());
        assert!(root.join("folder-2.yaml").is_file(), "`folder.yaml` is reserved");

        // The whole point: no credential in anything that would be committed.
        for file in all_files(&root) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(&file).unwrap();
            if name == ".env.local" {
                assert!(text.contains("live_abc123") && text.contains("eyJhbGciOi.folder"));
                continue;
            }
            for secret in ["live_abc123", "eyJhbGciOi.folder", "cs_999", "tok_literal"] {
                assert!(!text.contains(secret), "`{secret}` leaked into {}", file.display());
            }
        }
        assert!(crate::secrets::is_ignored(&fs::read_to_string(root.join(".gitignore")).unwrap(), ".env.local"));

        // And loading re-attaches the secrets from the environment's file.
        let env = &outcome.collection.environments[0];
        assert_eq!(env.vars.iter().find(|v| v.name == "apiKey").unwrap().value, "live_abc123");

        // Importing again does not overwrite the first one.
        let again = import_file(&source, &into).unwrap();
        assert!(again.report.path.ends_with("shop-api-2"));

        fs::remove_dir_all(&into).ok();
    }

    #[test]
    fn writing_needs_an_existing_folder() {
        let imported = parse(INSOMNIA_V5).unwrap();
        assert!(write(Path::new("definitely/not/here"), &imported).is_err());
    }

    /// Two exports concatenated is enough to repeat an `_id`. Before the path
    /// check this recursed until the stack ran out, which cannot be caught:
    /// the whole app went down with every open tab.
    #[test]
    fn an_insomnia_export_whose_folders_contain_themselves_is_skipped_not_fatal() {
        let doc = r#"{
          "_type": "export",
          "__export_format": 4,
          "resources": [
            { "_type": "workspace", "_id": "w", "name": "Loop" },
            { "_type": "request_group", "_id": "a", "parentId": "w", "name": "Outer" },
            { "_type": "request_group", "_id": "a", "parentId": "a", "name": "Inner" },
            { "_type": "request", "_id": "r", "parentId": "a", "name": "Ping", "method": "GET", "url": "https://api.test/ping" }
          ]
        }"#;

        let imported = parse(doc).expect("a cycle is a warning, not a failure");
        assert!(
            imported.warnings.mentions("listed inside itself"),
            "the report says what was skipped: {:?}",
            imported.warnings.render()
        );
        // The request inside the folder still comes across.
        let outer = folder(&imported.nodes, "Outer");
        assert_eq!(request(outer, "Ping").method, "GET");
    }
}

//! OpenAPI and Swagger import.
//!
//! The most asked-for import source, and the one that reads least like a
//! collection: a spec describes an API, not a set of requests someone made.
//! So this is a translation with opinions, and each one is written down here
//! rather than left for the reader to infer.
//!
//! - Paths become folders by their first segment, because a flat list of forty
//!   endpoints is not a collection anyone can use.
//! - `servers[0]` becomes `{{baseUrl}}`; the rest go in the report.
//! - A path parameter becomes `{{name}}`, so it lands in the same variable
//!   marking everything else uses.
//! - A request body's schema becomes an example body, built from `example`,
//!   `default`, `enum`, and finally the type. It is a starting point, not a
//!   claim about what the server wants.
//! - `securitySchemes` become auth with `{{variable}}` placeholders. A spec
//!   never carries a real token, and if one is in there it is a mistake we do
//!   not want to copy.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::import::{ImportNode, Imported, Warnings};
use crate::model::{ApiKeyLocation, Auth, Body, Environment, EnvVar, KeyValue, Request};

/// Does this look like a spec rather than a collection export?
pub fn looks_like(value: &Value) -> bool {
    value.get("openapi").and_then(Value::as_str).is_some_and(|v| v.starts_with('3'))
        || value.get("swagger").and_then(Value::as_str).is_some_and(|v| v.starts_with('2'))
}

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

pub fn parse(spec: &Value) -> Imported {
    let mut warnings = Warnings::default();
    let swagger = spec.get("swagger").is_some();

    let info = spec.get("info").cloned().unwrap_or(Value::Null);
    let name = info.get("title").and_then(Value::as_str).unwrap_or("API").to_string();

    let (base, extra) = base_url(spec, swagger);
    for other in extra {
        warnings.add("Only the first server became {{baseUrl}}; the others are here", &other);
    }

    let schemes = security_schemes(spec, swagger);
    let collection_auth = spec
        .get("security")
        .and_then(Value::as_array)
        .and_then(|requirements| auth_for(requirements, &schemes));

    // Group by the first path segment, keeping the order the spec had.
    let mut folders: BTreeMap<String, Vec<ImportNode>> = BTreeMap::new();
    let mut loose: Vec<ImportNode> = Vec::new();
    let mut seq = 0u32;

    let paths = spec.get("paths").and_then(Value::as_object).cloned().unwrap_or_default();
    for (path, item) in paths {
        let shared: Vec<Value> = item.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();

        for method in METHODS {
            let Some(operation) = item.get(method) else { continue };
            seq += 1;
            let request =
                operation_request(spec, &path, method, operation, &shared, &schemes, swagger, seq, &mut warnings);

            match path.split('/').find(|part| !part.is_empty() && !part.starts_with('{')) {
                Some(group) => folders.entry(group.to_string()).or_default().push(ImportNode::Request(Box::new(request))),
                None => loose.push(ImportNode::Request(Box::new(request))),
            }
        }
    }

    let mut nodes: Vec<ImportNode> = folders
        .into_iter()
        .map(|(name, children)| ImportNode::Folder {
            name,
            // A spec's security is expressed per operation or per collection;
            // there is no folder level to carry.
            auth: Auth::Inherit,
            headers: Vec::new(),
            children,
        })
        .collect();
    nodes.extend(loose);

    let environment = Environment {
        name: "local".into(),
        vars: vec![EnvVar { name: "baseUrl".into(), value: base, secret: false }],
    };

    Imported {
        format: if swagger { "Swagger 2.0" } else { "OpenAPI 3" },
        name,
        headers: Vec::new(),
        auth: collection_auth.unwrap_or(Auth::None),
        nodes,
        environments: vec![environment],
        warnings,
    }
}

/// `servers[0].url` in OpenAPI 3, or scheme+host+basePath in Swagger 2.
fn base_url(spec: &Value, swagger: bool) -> (String, Vec<String>) {
    if swagger {
        let scheme = spec
            .get("schemes")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
            .and_then(Value::as_str)
            .unwrap_or("https");
        let host = spec.get("host").and_then(Value::as_str).unwrap_or("");
        let base = spec.get("basePath").and_then(Value::as_str).unwrap_or("");
        let url = if host.is_empty() { base.to_string() } else { format!("{scheme}://{host}{base}") };
        return (url, Vec::new());
    }

    let servers = spec.get("servers").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut urls: Vec<String> = servers
        .iter()
        .map(|server| {
            let url = server.get("url").and_then(Value::as_str).unwrap_or("").to_string();
            // `{version}` in a server URL is the same idea as our own variables.
            url.replace('{', "{{").replace('}', "}}")
        })
        .filter(|url| !url.is_empty())
        .collect();

    if urls.is_empty() {
        return (String::new(), Vec::new());
    }
    let first = urls.remove(0);
    (first, urls)
}

#[allow(clippy::too_many_arguments)]
fn operation_request(
    spec: &Value,
    path: &str,
    method: &str,
    operation: &Value,
    shared: &[Value],
    schemes: &BTreeMap<String, Value>,
    swagger: bool,
    seq: u32,
    warnings: &mut Warnings,
) -> Request {
    let name = operation
        .get("summary")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| operation.get("operationId").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| format!("{} {path}", method.to_uppercase()));

    // Path parameters become our own variables, so they mark up like the rest.
    let url = format!("{{{{baseUrl}}}}{}", path.replace('{', "{{").replace('}', "}}"));

    let mut parameters: Vec<&Value> = shared.iter().collect();
    if let Some(own) = operation.get("parameters").and_then(Value::as_array) {
        parameters.extend(own.iter());
    }

    let mut params = Vec::new();
    let mut headers = Vec::new();
    let mut form_body = None;

    for parameter in parameters {
        let parameter = resolve(spec, parameter);
        let name = parameter.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let description = parameter.get("description").and_then(Value::as_str).map(str::to_string);
        let required = parameter.get("required").and_then(Value::as_bool).unwrap_or(false);
        let value = parameter_example(&parameter);

        match parameter.get("in").and_then(Value::as_str).unwrap_or("") {
            "query" => params.push(KeyValue { name, value, enabled: required, description }),
            "header" => headers.push(KeyValue { name, value, enabled: required, description }),
            // A path parameter is already `{{name}}` in the URL.
            "path" => {}
            "formData" => {
                let fields: &mut Vec<crate::model::FormField> = form_body.get_or_insert_with(Vec::new);
                fields.push(crate::model::FormField {
                    name,
                    value,
                    enabled: required,
                    description,
                    file: parameter.get("type").and_then(Value::as_str) == Some("file"),
                });
            }
            "body" if swagger => {}
            other => warnings.add(format!("Parameters in `{other}` were not imported"), &name),
        }
    }

    let (body, content_type) = if swagger {
        swagger_body(spec, operation, form_body)
    } else {
        openapi_body(spec, operation)
    };
    if let Some(mime) = content_type {
        if !headers.iter().any(|h| h.name.eq_ignore_ascii_case("content-type")) {
            // Only when it is not the one the body already implies, so the
            // usual json case stays quiet.
            if !matches!(body, Body::Json { .. }) {
                headers.push(KeyValue {
                    name: "Content-Type".into(),
                    value: mime,
                    enabled: true,
                    description: None,
                });
            }
        }
    }

    let auth = operation
        .get("security")
        .and_then(Value::as_array)
        .and_then(|requirements| auth_for(requirements, schemes))
        .unwrap_or(Auth::Inherit);

    if operation.get("deprecated").and_then(Value::as_bool) == Some(true) {
        warnings.add("Deprecated in the spec, imported anyway", &name);
    }

    let docs = operation
        .get("description")
        .and_then(Value::as_str)
        .filter(|d| !d.trim().is_empty())
        .map(str::to_string);

    Request {
        name,
        kind: crate::model::Kind::Http,
        seq,
        method: method.to_uppercase(),
        url,
        headers,
        params,
        body,
        auth,
        captures: Vec::new(),
        checks: Vec::new(),
        options: None,
        docs,
    }
}

/// `$ref: '#/components/schemas/User'`, resolved within the same document.
/// A reference to another file is left alone: we have one file.
fn resolve(spec: &Value, value: &Value) -> Value {
    let mut at = value.clone();
    // Chains are legal and short; five is past anything real and stops a loop.
    for _ in 0..5 {
        let Some(pointer) = at.get("$ref").and_then(Value::as_str) else { break };
        let Some(local) = pointer.strip_prefix("#/") else { break };
        let mut found = spec;
        for segment in local.split('/') {
            let segment = segment.replace("~1", "/").replace("~0", "~");
            match found.get(&segment) {
                Some(next) => found = next,
                None => return at,
            }
        }
        at = found.clone();
    }
    at
}

/// OpenAPI 3: `requestBody.content["application/json"].schema`.
fn openapi_body(spec: &Value, operation: &Value) -> (Body, Option<String>) {
    let Some(request_body) = operation.get("requestBody") else { return (Body::None, None) };
    let request_body = resolve(spec, request_body);
    let Some(content) = request_body.get("content").and_then(Value::as_object) else {
        return (Body::None, None);
    };

    // Prefer JSON, then a form, then whatever is first: the order people mean.
    let pick = |wanted: &str| content.iter().find(|(mime, _)| mime.starts_with(wanted));
    let (mime, media) = match pick("application/json").or_else(|| pick("application/x-www-form-urlencoded")).or_else(|| pick("multipart/form-data")).or_else(|| content.iter().next()) {
        Some(found) => found,
        None => return (Body::None, None),
    };
    let mime = mime.clone();

    let schema = media.get("schema").map(|schema| resolve(spec, schema)).unwrap_or(Value::Null);
    let example = media
        .get("example")
        .cloned()
        .or_else(|| media.get("examples").and_then(Value::as_object).and_then(|all| all.values().next()).and_then(|first| first.get("value").cloned()))
        .unwrap_or_else(|| sample(spec, &schema, 0, &mut SAMPLE_BUDGET.clone()));

    if mime.starts_with("application/json") {
        return (Body::Json { content: pretty(&example) }, Some(mime));
    }
    if mime.starts_with("application/x-www-form-urlencoded") || mime.starts_with("multipart/form-data") {
        let fields = field_list(spec, &schema);
        let body = if mime.starts_with("multipart") {
            Body::Form { fields: fields.into_iter().map(|(name, value)| crate::model::FormField { name, value, enabled: true, description: None, file: false }).collect() }
        } else {
            Body::UrlEncoded { fields: fields.into_iter().map(|(name, value)| KeyValue { name, value, enabled: true, description: None }).collect() }
        };
        return (body, Some(mime));
    }
    if mime.starts_with("text/") || mime.contains("xml") {
        let text = if example.is_string() { example.as_str().unwrap_or("").to_string() } else { pretty(&example) };
        let body = if mime.contains("xml") { Body::Xml { content: text } } else { Body::Text { content: text } };
        return (body, Some(mime));
    }
    (Body::Text { content: pretty(&example) }, Some(mime))
}

/// Swagger 2: a `body` parameter, or `formData` ones gathered by the caller.
fn swagger_body(spec: &Value, operation: &Value, form: Option<Vec<crate::model::FormField>>) -> (Body, Option<String>) {
    if let Some(fields) = form {
        let multipart = operation
            .get("consumes")
            .and_then(Value::as_array)
            .is_some_and(|all| all.iter().any(|m| m.as_str().is_some_and(|m| m.starts_with("multipart"))));
        return if multipart || fields.iter().any(|f| f.file) {
            (Body::Form { fields }, Some("multipart/form-data".into()))
        } else {
            let pairs = fields
                .into_iter()
                .map(|f| KeyValue { name: f.name, value: f.value, enabled: f.enabled, description: f.description })
                .collect();
            (Body::UrlEncoded { fields: pairs }, Some("application/x-www-form-urlencoded".into()))
        };
    }

    let body = operation
        .get("parameters")
        .and_then(Value::as_array)
        .and_then(|all| all.iter().find(|p| resolve(spec, p).get("in").and_then(Value::as_str) == Some("body")).cloned());

    let Some(parameter) = body else { return (Body::None, None) };
    let parameter = resolve(spec, &parameter);
    let schema = parameter.get("schema").map(|s| resolve(spec, s)).unwrap_or(Value::Null);
    (Body::Json { content: pretty(&sample(spec, &schema, 0, &mut SAMPLE_BUDGET.clone())) }, Some("application/json".into()))
}

fn field_list(spec: &Value, schema: &Value) -> Vec<(String, String)> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(name, property)| {
                    let property = resolve(spec, property);
                    let value = sample(spec, &property, 0, &mut SAMPLE_BUDGET.clone());
                    let text = if value.is_string() { value.as_str().unwrap_or("").to_string() } else { value.to_string() };
                    (name.clone(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
}

/// A believable value for a schema: whatever the spec says first, then the
/// shape of the type. Never a claim about what the server wants — a starting
/// How many nodes one generated example body may contain.
const SAMPLE_BUDGET: usize = 2_000;

/// point the user edits.
/// `budget` bounds the total number of nodes, not just the depth. A schema
/// whose `items` refers back to itself fans out `n` ways per level, so a depth
/// limit alone still let a tiny spec produce hundreds of megabytes.
fn sample(spec: &Value, schema: &Value, depth: usize, budget: &mut usize) -> Value {
    if *budget == 0 {
        return Value::Null;
    }
    *budget -= 1;
    if depth > 6 {
        return Value::Null;
    }
    let schema = resolve(spec, schema);

    for key in ["example", "default"] {
        if let Some(given) = schema.get(key) {
            return given.clone();
        }
    }
    if let Some(first) = schema.get("enum").and_then(Value::as_array).and_then(|all| all.first()) {
        return first.clone();
    }
    // `allOf` is a merge; the others are a choice, and the first is as good a
    // guess as any.
    if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        for part in all {
            if let Some(object) = sample(spec, part, depth + 1, budget).as_object() {
                merged.extend(object.clone());
            }
        }
        return Value::Object(merged);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = schema.get(key).and_then(Value::as_array).and_then(|all| all.first()) {
            return sample(spec, first, depth + 1, budget);
        }
    }

    let kind = schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(if schema.get("properties").is_some() { "object" } else { "string" });

    match kind {
        "object" => {
            let mut out = Map::new();
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    out.insert(name.clone(), sample(spec, property, depth + 1, budget));
                }
            }
            Value::Object(out)
        }
        "array" => {
            let item = schema.get("items").map(|items| sample(spec, items, depth + 1, budget)).unwrap_or(Value::Null);
            Value::Array(vec![item])
        }
        "integer" => Value::from(0),
        "number" => Value::from(0.0),
        "boolean" => Value::Bool(false),
        _ => Value::String(string_for(&schema)),
    }
}

/// A string that shows the shape when the spec gives a format.
fn string_for(schema: &Value) -> String {
    match schema.get("format").and_then(Value::as_str).unwrap_or("") {
        "date-time" => "1970-01-01T00:00:00Z".into(),
        "date" => "1970-01-01".into(),
        "uuid" => "00000000-0000-0000-0000-000000000000".into(),
        "email" => "name@example.com".into(),
        "uri" | "url" => "https://example.com".into(),
        "byte" => "aGVsbG8=".into(),
        "binary" => String::new(),
        "password" => String::new(),
        _ => String::new(),
    }
}

fn parameter_example(parameter: &Value) -> String {
    for key in ["example", "default"] {
        if let Some(given) = parameter.get(key).or_else(|| parameter.get("schema").and_then(|s| s.get(key))) {
            return match given {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
        }
    }
    parameter
        .get("schema")
        .and_then(|s| s.get("enum"))
        .or_else(|| parameter.get("enum"))
        .and_then(Value::as_array)
        .and_then(|all| all.first())
        .map(|first| match first {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

fn security_schemes(spec: &Value, swagger: bool) -> BTreeMap<String, Value> {
    let source = if swagger {
        spec.get("securityDefinitions")
    } else {
        spec.get("components").and_then(|c| c.get("securitySchemes"))
    };
    source
        .and_then(Value::as_object)
        .map(|all| all.iter().map(|(name, scheme)| (name.clone(), resolve(spec, scheme))).collect())
        .unwrap_or_default()
}

/// The first requirement volt can express. A spec's placeholders are
/// `{{variables}}`: a token in a spec would be a leak, not a gift.
fn auth_for(requirements: &[Value], schemes: &BTreeMap<String, Value>) -> Option<Auth> {
    for requirement in requirements {
        let Some(object) = requirement.as_object() else { continue };
        for name in object.keys() {
            let Some(scheme) = schemes.get(name) else { continue };
            let kind = scheme.get("type").and_then(Value::as_str).unwrap_or("");
            let flavour = scheme.get("scheme").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();

            return Some(match (kind, flavour.as_str()) {
                ("http", "basic") | ("basic", _) => {
                    Auth::Basic { username: "{{username}}".into(), password: "{{password}}".into() }
                }
                ("http", "bearer") | ("oauth2", _) => Auth::Bearer { token: "{{token}}".into() },
                ("apiKey", _) => Auth::ApiKey {
                    key: scheme.get("name").and_then(Value::as_str).unwrap_or("X-API-Key").to_string(),
                    value: "{{apiKey}}".into(),
                    location: match scheme.get("in").and_then(Value::as_str) {
                        Some("query") => ApiKeyLocation::Query,
                        _ => ApiKeyLocation::Header,
                    },
                },
                _ => continue,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Value {
        serde_json::from_str(include_str!("../../examples/openapi-sample.json")).unwrap()
    }

    fn find<'a>(nodes: &'a [ImportNode], name: &str) -> Option<&'a Request> {
        for node in nodes {
            match node {
                ImportNode::Folder { children, .. } => {
                    if let Some(found) = find(children, name) {
                        return Some(found);
                    }
                }
                ImportNode::Request(request) if request.name == name => return Some(request),
                _ => {}
            }
        }
        None
    }

    #[test]
    fn a_spec_becomes_folders_requests_and_a_base_url() {
        let imported = parse(&spec());

        assert_eq!(imported.name, "Shop API");
        assert_eq!(imported.format, "OpenAPI 3");
        assert_eq!(imported.environments[0].vars[0].name, "baseUrl");
        assert_eq!(
            imported.environments[0].vars[0].value, "https://api.shop.test/{{version}}",
            "a server template becomes our own variable"
        );

        let folders: Vec<&str> = imported
            .nodes
            .iter()
            .filter_map(|node| match node {
                ImportNode::Folder { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(folders, ["health", "orders"], "grouped by the first path segment");

        let list = find(&imported.nodes, "List orders").expect("the summary is the name");
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{baseUrl}}/orders");
        assert_eq!(list.params[0].name, "page");
        assert_eq!(list.params[0].value, "2", "a default becomes the value");
        assert!(!list.params[0].enabled, "optional parameters arrive switched off");
        assert!(list.headers.iter().any(|h| h.name == "X-Trace" && h.enabled), "a required one is on");
    }

    #[test]
    fn a_path_parameter_becomes_a_variable_and_the_operation_id_names_it() {
        let imported = parse(&spec());

        let cancel = find(&imported.nodes, "Cancel an order").unwrap();
        assert_eq!(cancel.url, "{{baseUrl}}/orders/{{orderId}}");
        assert!(cancel.params.is_empty(), "a path parameter is not a query one");

        let create = find(&imported.nodes, "createOrder").expect("no summary: the operationId names it");
        assert_eq!(create.docs.as_deref(), Some("Creates one."));
    }

    #[test]
    fn a_schema_becomes_a_body_worth_editing() {
        let imported = parse(&spec());
        let create = find(&imported.nodes, "createOrder").unwrap();

        let Body::Json { content } = &create.body else { panic!("expected json, got {:?}", create.body) };
        let body: Value = serde_json::from_str(content).expect("valid JSON");

        assert_eq!(body["status"], "paid", "an enum offers its first value");
        assert_eq!(body["id"], "00000000-0000-0000-0000-000000000000", "a format shows the shape");
        assert_eq!(body["total"], 0.0);
        assert_eq!(body["items"][0]["sku"], "A-1", "through a $ref, keeping its example");
        assert_eq!(body["items"][0]["qty"], 0);
    }

    #[test]
    fn security_becomes_auth_with_placeholders_never_a_token() {
        let imported = parse(&spec());
        assert!(matches!(&imported.auth, Auth::Bearer { token } if token == "{{token}}"));

        let cancel = find(&imported.nodes, "Cancel an order").unwrap();
        assert!(
            matches!(&cancel.auth, Auth::ApiKey { key, value, .. } if key == "X-Shop-Key" && value == "{{apiKey}}"),
            "an operation's own scheme wins: {:?}",
            cancel.auth
        );

        let list = find(&imported.nodes, "List orders").unwrap();
        assert!(matches!(list.auth, Auth::Inherit), "everything else inherits the collection's");
    }

    #[test]
    fn what_did_not_come_across_is_reported() {
        let imported = parse(&spec());
        let said = imported.warnings.render().join("\n");
        assert!(said.contains("first server"), "{said}");
        assert!(said.contains("Deprecated"), "{said}");
    }

    #[test]
    fn swagger_2_reads_too() {
        let spec: Value = serde_json::from_str(
            r#"{"swagger":"2.0","info":{"title":"Old"},"host":"api.old.test","basePath":"/v1","schemes":["https"],
                "paths":{"/pets":{"post":{"summary":"Add","consumes":["application/json"],
                "parameters":[{"in":"body","name":"body","schema":{"type":"object","properties":{"name":{"type":"string"}}}}]}}}}"#,
        )
        .unwrap();

        assert!(looks_like(&spec));
        let imported = parse(&spec);
        assert_eq!(imported.format, "Swagger 2.0");
        assert_eq!(imported.environments[0].vars[0].value, "https://api.old.test/v1");
        let add = find(&imported.nodes, "Add").unwrap();
        assert!(matches!(&add.body, Body::Json { content } if content.contains("name")), "{:?}", add.body);
    }

    /// A schema whose array items refer back to itself fans out at every level.
    /// A depth limit alone still let a handful of lines become hundreds of
    /// megabytes, taking the import — and the app — down with it.
    #[test]
    fn a_self_referencing_schema_cannot_run_away() {
        let spec = r##"{
          "openapi": "3.0.0",
          "info": { "title": "Loop", "version": "1" },
          "components": {
            "schemas": {
              "Node": {
                "type": "object",
                "properties": {
                  "a": { "$ref": "#/components/schemas/Node" },
                  "b": { "$ref": "#/components/schemas/Node" },
                  "c": { "$ref": "#/components/schemas/Node" },
                  "d": { "$ref": "#/components/schemas/Node" },
                  "e": { "$ref": "#/components/schemas/Node" },
                  "f": { "$ref": "#/components/schemas/Node" },
                  "g": { "$ref": "#/components/schemas/Node" },
                  "h": { "$ref": "#/components/schemas/Node" },
                  "i": { "$ref": "#/components/schemas/Node" },
                  "j": { "$ref": "#/components/schemas/Node" }
                }
              }
            }
          },
          "paths": {
            "/nodes": {
              "post": {
                "requestBody": {
                  "content": {
                    "application/json": {
                      "schema": { "$ref": "#/components/schemas/Node" }
                    }
                  }
                },
                "responses": { "200": { "description": "ok" } }
              }
            }
          }
        }"##;

        let imported = parse(&serde_json::from_str(spec).expect("valid JSON"));
        let body = match &imported.nodes[0] {
            crate::import::ImportNode::Folder { children, .. } => match &children[0] {
                crate::import::ImportNode::Request(request) => match &request.body {
                    crate::model::Body::Json { content } => content.clone(),
                    other => panic!("expected a JSON body, got {other:?}"),
                },
                other => panic!("expected a request, got {other:?}"),
            },
            crate::import::ImportNode::Request(request) => match &request.body {
                crate::model::Body::Json { content } => content.clone(),
                other => panic!("expected a JSON body, got {other:?}"),
            },
        };
        assert!(
            body.len() < 500_000,
            "the example body is bounded, not exponential: {} bytes",
            body.len()
        );
    }
}

//! Bruno import: a folder of `.bru` files, as a collection.
//!
//! Bruno is the nearest thing to volt there is — a collection is a directory,
//! a request is a file, variables are `{{name}}` — so most of this is a walk
//! that keeps the shape it finds: a folder stays a folder with its own
//! headers, auth and variables, a request stays a request, and
//! `environments/*.bru` become environments, with the names under
//! `vars:secret` marked secret and left empty, exactly as Bruno keeps them.
//!
//! Where the two differ, the same rule as every other import applies: what
//! has an equivalent is translated, and what has none is named in the report.
//!
//! - `assert` lines become checks where the operator has one that means the
//!   same; the rest are quoted in the report.
//! - `vars:post-response` and `bru.setEnvVar("x", res.body.a)` are captures.
//! - `tests` and `script:post-response` go through `scripts::translate`, after
//!   Bruno's names for things are rewritten to the ones it reads.
//! - `vars:pre-request` on a request has nowhere to live — a volt request has
//!   no variables of its own — so a value the request uses is written into
//!   it, and the report says so.
//! - A pre-request script, OAuth 2 and gRPC or WebSocket requests are
//!   reported, never guessed at.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::bru::{self, Document, Entry};
use crate::checks::Op;
use crate::error::{Error, Result};
use crate::import::{split_query, ImportNode, Imported, Warnings};
use crate::model::{ApiKeyLocation, Auth, Body, Capture, Check, EnvVar, Environment, FormField, KeyValue, Kind, Request};
use crate::scripts;

const MANIFEST: &str = "bruno.json";

/// The Bruno collection `source` names, if it names one: the folder that
/// holds a `bruno.json`, or that file itself.
pub fn collection_dir(source: &Path) -> Option<PathBuf> {
    if source.is_dir() {
        return source.join(MANIFEST).is_file().then(|| source.to_path_buf());
    }
    let named = source.file_name().is_some_and(|name| name.eq_ignore_ascii_case(MANIFEST));
    (named && source.is_file()).then(|| source.parent().map(Path::to_path_buf)).flatten()
}

/// How deep the walk goes. Far past any collection anybody keeps, and well
/// short of the stack: a directory that loops back on itself through a link
/// must not be able to take the app down.
const MAX_DEPTH: usize = 24;

pub fn read(dir: &Path) -> Result<Imported> {
    let manifest_path = dir.join(MANIFEST);
    let manifest = fs::read_to_string(&manifest_path).map_err(|e| Error::io(manifest_path.to_string_lossy(), e))?;
    let manifest: Value = serde_json::from_str(manifest.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::parse(manifest_path.to_string_lossy(), e))?;
    let name = match manifest.get("name").and_then(Value::as_str).map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported collection".into()),
    };

    let mut w = Warnings::default();
    let mut headers = Vec::new();
    let mut auth = Auth::None;
    let mut vars = Vec::new();
    if let Ok(text) = fs::read_to_string(dir.join("collection.bru")) {
        let doc = bru::parse(&text);
        headers = key_values(doc.pairs("headers"));
        // A collection has nothing above it to inherit from.
        auth = match scope_auth(&doc, &name, &mut w) {
            Auth::Inherit => Auth::None,
            other => other,
        };
        vars = key_values(doc.pairs("vars:pre-request"));
        scope_leftovers(&doc, &name, &mut w);
    }

    let nodes = walk(dir, true, 0, &mut w);
    let environments = environments(&dir.join("environments"), &mut w);

    Ok(Imported { format: "Bruno", name, headers, auth, vars, nodes, environments, warnings: w })
}

/// One directory: its folders and its requests, in Bruno's order.
fn walk(dir: &Path, root: bool, depth: usize, w: &mut Warnings) -> Vec<ImportNode> {
    let Ok(listing) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(u32, String, ImportNode)> = Vec::new();

    for entry in listing.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        // Not followed: a link is how a tree comes to contain itself.
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() || file_name.starts_with('.') {
            continue;
        }

        if kind.is_dir() {
            if file_name == "node_modules" || (root && file_name == "environments") {
                continue;
            }
            if depth + 1 >= MAX_DEPTH {
                w.add("A folder nested too deeply to be a collection's was not read", &file_name);
                continue;
            }
            let doc = fs::read_to_string(path.join("folder.bru")).map(|text| bru::parse(&text)).unwrap_or_default();
            let name = doc.value("meta", "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&file_name).to_string();
            let seq = doc.value("meta", "seq").and_then(|seq| seq.trim().parse().ok()).unwrap_or(u32::MAX);
            let auth = scope_auth(&doc, &name, w);
            scope_leftovers(&doc, &name, w);
            let children = walk(&path, false, depth + 1, w);
            let folder = ImportNode::Folder {
                name: name.clone(),
                auth,
                headers: key_values(doc.pairs("headers")),
                vars: key_values(doc.pairs("vars:pre-request")),
                children,
            };
            found.push((seq, name.to_lowercase(), folder));
            continue;
        }

        let Some(stem) = file_name.strip_suffix(".bru") else { continue };
        if matches!(stem, "folder" | "collection") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            w.add("A file could not be read as text and was skipped", &file_name);
            continue;
        };
        let doc = bru::parse(&text);
        let seq = doc.value("meta", "seq").and_then(|seq| seq.trim().parse().ok()).unwrap_or(u32::MAX);
        if let Some(request) = request(&doc, stem, w) {
            found.push((seq, request.name.to_lowercase(), ImportNode::Request(Box::new(request))));
        }
    }

    // Folders first and then requests, each by `seq` and then by name: the
    // order Bruno's own sidebar shows them in.
    let is_request = |node: &ImportNode| matches!(node, ImportNode::Request(_));
    found.sort_by(|a, b| (is_request(&a.2), a.0, &a.1).cmp(&(is_request(&b.2), b.0, &b.1)));
    found.into_iter().map(|(_, _, node)| node).collect()
}

/// What a folder or the collection carries that volt has no place for.
fn scope_leftovers(doc: &Document, subject: &str, w: &mut Warnings) {
    if doc.text("script:pre-request").is_some() || doc.text("script:post-response").is_some() || doc.text("tests").is_some() {
        w.add("Scripts on a collection or a folder are not imported", subject);
    }
    if !doc.pairs("vars:post-response").is_empty() {
        w.add("Post-response variables on a collection or a folder are not imported: a capture belongs to a request", subject);
    }
    if doc.text("docs").is_some() {
        w.add("Docs on a collection or a folder are not imported: only a request has docs", subject);
    }
}

fn key_values(entries: &[Entry]) -> Vec<KeyValue> {
    entries
        .iter()
        .map(|entry| KeyValue {
            // `@name` is Bruno's mark for a variable that is local to a run.
            name: entry.name.trim_start_matches('@').to_string(),
            value: entry.value.clone(),
            enabled: entry.enabled,
            description: None,
        })
        .collect()
}

fn environments(dir: &Path, w: &mut Warnings) -> Vec<Environment> {
    let Ok(listing) = fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> =
        listing.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|e| e == "bru")).collect();
    files.sort();

    let mut out = Vec::new();
    for path in files {
        let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let Ok(text) = fs::read_to_string(&path) else { continue };
        out.push(environment(&name, &bru::parse(&text)));
    }
    if out.iter().any(|env: &Environment| env.vars.iter().any(|var| var.secret && var.value.is_empty())) {
        w.add("Secret variables came across by name only — Bruno keeps their values out of the collection, so fill them in", "");
    }
    out
}

/// `vars { }` are values; `vars:secret [ ]` are names whose values Bruno keeps
/// somewhere else, so they arrive secret and empty.
fn environment(name: &str, doc: &Document) -> Environment {
    let mut vars: Vec<EnvVar> = doc
        .pairs("vars")
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| EnvVar { name: entry.name.clone(), value: entry.value.clone(), secret: false })
        .collect();
    for secret in doc.list("vars:secret").iter().filter(|entry| entry.enabled) {
        if !vars.iter().any(|var| var.name == secret.name) {
            vars.push(EnvVar { name: secret.name.clone(), value: String::new(), secret: true });
        }
    }
    Environment { name: name.to_string(), vars }
}

// ---------------------------------------------------------------------------
// One request
// ---------------------------------------------------------------------------

const METHODS: [&str; 9] = ["get", "post", "put", "patch", "delete", "head", "options", "trace", "connect"];

/// A `.bru` request as a volt one, or `None` when it is a kind volt does not
/// import — which the report then says.
pub fn request(doc: &Document, file_stem: &str, w: &mut Warnings) -> Option<Request> {
    let name = doc.value("meta", "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(file_stem).to_string();

    let kind = doc.value("meta", "type").unwrap_or("http").trim().to_ascii_lowercase();
    if matches!(kind.as_str(), "grpc" | "ws" | "websocket") || doc.has("grpc") || doc.has("ws") {
        w.add("gRPC and WebSocket requests are not supported and were skipped", &name);
        return None;
    }

    // The block named for the method holds the address and says which body
    // and which auth are the ones in use; newer files spell it `http { method }`.
    let (method, block) = match METHODS.iter().find(|method| doc.has(method)) {
        Some(method) => (method.to_uppercase(), *method),
        None if doc.has("http") => (doc.value("http", "method").unwrap_or("get").trim().to_uppercase(), "http"),
        None => {
            w.add("A file with no request in it was skipped", &name);
            return None;
        }
    };

    let (mut url, from_url) = split_query(doc.value(block, "url").unwrap_or_default().trim());
    // The query block is the real list — it also has the switched-off ones —
    // and the address only repeats it.
    let query = if doc.has("params:query") { doc.pairs("params:query") } else { doc.pairs("query") };
    let params = if query.is_empty() { from_url } else { key_values(query) };

    for entry in doc.pairs("params:path").iter().filter(|entry| entry.enabled) {
        let segment = format!(":{}", entry.name);
        if !url.split('/').any(|part| part == segment) {
            w.add("A path parameter with no `:name` in the URL was left out", &name);
            continue;
        }
        // A value is written in; an empty one becomes a variable to fill.
        let replacement = if entry.value.is_empty() { format!("{{{{{}}}}}", entry.name) } else { entry.value.clone() };
        url = url.split('/').map(|part| if part == segment { replacement.as_str() } else { part }).collect::<Vec<_>>().join("/");
    }

    let body = body(doc, doc.value(block, "body").map(str::trim), &name, w);
    let auth = match doc.value(block, "auth").map(str::trim) {
        // A request says `inherit` when it means it; saying nothing is none.
        None | Some("") => Auth::None,
        Some(mode) => auth(doc, mode, &name, w),
    };

    let mut captures: Vec<Capture> = Vec::new();
    for entry in doc.pairs("vars:post-response") {
        match source(&entry.value) {
            Some(from) => captures.push(Capture {
                name: entry.name.trim_start_matches('@').to_string(),
                from,
                enabled: entry.enabled,
                // The same rule a script's capture gets: a token until proved
                // otherwise, and so kept out of the committed YAML.
                secret: true,
            }),
            None => w.add(
                format!("A post-response variable was not imported — `{}: {}`", entry.name, shorten(&entry.value)),
                &name,
            ),
        }
    }

    let mut checks: Vec<Check> = Vec::new();
    for entry in doc.pairs("assert") {
        match assertion(entry) {
            Some(check) => checks.push(check),
            None => w.add(
                format!("An assertion was not imported — `{}: {}`", entry.name, shorten(&entry.value)),
                &name,
            ),
        }
    }

    if doc.text("script:pre-request").is_some() {
        w.add("Pre-request scripts are not imported: nothing in volt runs before a send", &name);
    }
    let code = [doc.text("script:post-response"), doc.text("tests")].into_iter().flatten().collect::<Vec<_>>().join("\n");
    if !code.trim().is_empty() {
        let read = scripts::translate(&in_postman_terms(&code));
        if read.found_something() {
            w.add("Test scripts were read as checks and captures", &name);
        }
        for line in &read.left {
            w.add(format!("A line of a test script was not imported — `{}`", shorten(&in_bruno_terms(line))), &name);
        }
        captures.extend(read.captures);
        checks.extend(read.checks);
    }

    let mut request = Request {
        name,
        kind: Kind::Http,
        seq: 1,
        method,
        url,
        headers: key_values(doc.pairs("headers")),
        params,
        body,
        auth,
        captures,
        checks,
        options: None,
        docs: doc.text("docs").map(str::to_string),
    };

    let mut written = false;
    let mut unused = false;
    for entry in doc.pairs("vars:pre-request").iter().filter(|entry| entry.enabled) {
        let placeholder = format!("{{{{{}}}}}", entry.name.trim_start_matches('@'));
        let mut used = false;
        each_text(&mut request, &mut |text| {
            if text.contains(&placeholder) {
                *text = text.replace(&placeholder, &entry.value);
                used = true;
            }
        });
        written |= used;
        unused |= !used;
    }
    if written {
        w.add("Pre-request variables were written into the request where it uses them: a request has no variables of its own", &request.name);
    }
    if unused {
        w.add("A pre-request variable the request itself does not use was left out", &request.name);
    }

    if serde_json::to_string(&request).is_ok_and(|json| json.contains("{{process.env.")) {
        w.add("`process.env` variables are Bruno's own and will show as undefined; define them in an environment", &request.name);
    }
    Some(request)
}

/// Every piece of text in a request a `{{variable}}` can sit in.
fn each_text(request: &mut Request, visit: &mut dyn FnMut(&mut String)) {
    visit(&mut request.url);
    for pair in request.headers.iter_mut().chain(request.params.iter_mut()) {
        visit(&mut pair.name);
        visit(&mut pair.value);
    }
    match &mut request.body {
        Body::Text { content } | Body::Json { content } | Body::Xml { content } => visit(content),
        Body::UrlEncoded { fields } => fields.iter_mut().for_each(|field| visit(&mut field.value)),
        Body::Form { fields } => fields.iter_mut().for_each(|field| visit(&mut field.value)),
        Body::GraphQl { query, variables } => {
            visit(query);
            visit(variables);
        }
        Body::None | Body::Binary { .. } | Body::Grpc { .. } => {}
    }
    match &mut request.auth {
        Auth::Bearer { token } => visit(token),
        Auth::Basic { username, password } | Auth::Digest { username, password } => {
            visit(username);
            visit(password);
        }
        Auth::ApiKey { key, value, .. } => {
            visit(key);
            visit(value);
        }
        _ => {}
    }
}

fn shorten(text: &str) -> String {
    let flat = text.replace(['\n', '\t'], " ");
    match flat.chars().count() > 60 {
        true => flat.chars().take(57).collect::<String>() + "…",
        false => flat,
    }
}

fn body(doc: &Document, mode: Option<&str>, subject: &str, w: &mut Warnings) -> Body {
    // An older file may not say which body is in use; then it is the one
    // that is there.
    let mode = match mode.filter(|mode| !mode.is_empty()) {
        Some(mode) => mode.to_string(),
        None => ["json", "text", "xml", "form-urlencoded", "multipart-form", "graphql"]
            .iter()
            .find(|kind| doc.has(&format!("body:{kind}")))
            .map(|kind| kind.to_string())
            .unwrap_or_else(|| "none".into()),
    };
    let content = |block: &str| doc.text(block).unwrap_or_default().to_string();

    match mode.as_str() {
        "none" => Body::None,
        "json" => Body::Json { content: content("body:json") },
        "text" => Body::Text { content: content("body:text") },
        "xml" => Body::Xml { content: content("body:xml") },
        "sparql" => Body::Text { content: content("body:sparql") },
        "formUrlEncoded" | "form-urlencoded" => Body::UrlEncoded { fields: key_values(doc.pairs("body:form-urlencoded")) },
        "multipartForm" | "multipart-form" => {
            let fields: Vec<FormField> = doc
                .pairs("body:multipart-form")
                .iter()
                .map(|entry| {
                    // `@file(path)` is a file part; a trailing
                    // `@contentType(…)` is the part's own type, which volt
                    // works out for itself.
                    let value = entry.value.split(" @contentType(").next().unwrap_or_default().trim();
                    let file = value.strip_prefix("@file(").and_then(|rest| rest.strip_suffix(')'));
                    FormField {
                        name: entry.name.clone(),
                        value: file.unwrap_or(value).to_string(),
                        enabled: entry.enabled,
                        description: None,
                        file: file.is_some(),
                    }
                })
                .collect();
            if fields.iter().any(|field| field.file) {
                w.add("A file field keeps the path from the export; point it at a file on this machine", subject);
            }
            Body::Form { fields }
        }
        "graphql" => Body::GraphQl { query: content("body:graphql"), variables: content("body:graphql:vars").trim().to_string() },
        "file" => {
            w.add("File bodies were not imported, since their path belongs to another machine", subject);
            Body::None
        }
        other => {
            w.add(format!("A `{other}` body is not a kind volt has, so it was left out"), subject);
            Body::None
        }
    }
}

/// A folder's or the collection's auth: `auth { mode: bearer }` and the block
/// that mode names. Saying nothing passes the question further out.
fn scope_auth(doc: &Document, subject: &str, w: &mut Warnings) -> Auth {
    match doc.value("auth", "mode").map(str::trim) {
        None | Some("") => Auth::Inherit,
        Some(mode) => auth(doc, mode, subject, w),
    }
}

fn auth(doc: &Document, mode: &str, subject: &str, w: &mut Warnings) -> Auth {
    let field = |block: &str, key: &str| doc.value(block, key).unwrap_or_default().to_string();
    match mode {
        "none" => Auth::None,
        "inherit" => Auth::Inherit,
        "bearer" => Auth::Bearer { token: field("auth:bearer", "token") },
        "basic" => Auth::Basic { username: field("auth:basic", "username"), password: field("auth:basic", "password") },
        "digest" => Auth::Digest { username: field("auth:digest", "username"), password: field("auth:digest", "password") },
        "ntlm" => Auth::Ntlm {
            username: field("auth:ntlm", "username"),
            password: field("auth:ntlm", "password"),
            domain: field("auth:ntlm", "domain"),
        },
        "apikey" => Auth::ApiKey {
            key: field("auth:apikey", "key"),
            value: field("auth:apikey", "value"),
            location: match field("auth:apikey", "placement").to_ascii_lowercase().as_str() {
                "queryparams" | "query" => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            },
        },
        "awsv4" => Auth::AwsSigV4 {
            key_id: field("auth:awsv4", "accessKeyId"),
            secret: field("auth:awsv4", "secretAccessKey"),
            region: match field("auth:awsv4", "region").trim() {
                "" => "us-east-1".to_string(),
                region => region.to_string(),
            },
            service: field("auth:awsv4", "service"),
            session_token: field("auth:awsv4", "sessionToken"),
        },
        "oauth2" => {
            w.add(
                "OAuth 2.0 is a step in volt, not a kind of auth: get a token from the OAuth dialog and use it as a bearer token. These use the auth above them instead",
                subject,
            );
            Auth::Inherit
        }
        other => {
            w.add(format!("`{other}` auth is not supported yet, so these use the auth above them instead"), subject);
            Auth::Inherit
        }
    }
}

// ---------------------------------------------------------------------------
// Expressions, assertions and scripts
// ---------------------------------------------------------------------------

/// A Bruno expression as one of volt's sources: `res.status` is `status`,
/// `res.body.data[0].id` is `$.data[0].id`, `res.headers.etag` is
/// `header:etag`. Anything with a call or an operator in it is not a place
/// to read from, and is refused rather than approximated.
fn source(expression: &str) -> Option<String> {
    let expression = expression.trim();
    match expression {
        "res.status" | "res.getStatus()" => return Some("status".into()),
        "res.responseTime" | "res.getResponseTime()" => return Some("time".into()),
        "res.body" | "res.getBody()" => return Some("body".into()),
        _ => {}
    }
    for prefix in ["res.getHeader(", "res.headers["] {
        if let Some(rest) = expression.strip_prefix(prefix) {
            let inside = rest.strip_suffix(')').or_else(|| rest.strip_suffix(']'))?;
            return quoted(inside).map(|name| format!("header:{name}"));
        }
    }
    if let Some(name) = expression.strip_prefix("res.headers.") {
        let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        return plain.then(|| format!("header:{name}"));
    }
    let rest = expression.strip_prefix("res.body").or_else(|| expression.strip_prefix("res.getBody()"))?;
    path(rest)
}

/// `.data[0]["id"]` → `$.data[0].id`: names and list indices, nothing else.
fn path(rest: &str) -> Option<String> {
    let mut out = String::from("$");
    let mut left = rest;
    while !left.is_empty() {
        if let Some(tail) = left.strip_prefix('.') {
            let end = tail.find(['.', '[']).unwrap_or(tail.len());
            let (key, tail) = tail.split_at(end);
            if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                return None;
            }
            out.push('.');
            out.push_str(key);
            left = tail;
        } else {
            let (inside, tail) = left.strip_prefix('[')?.split_once(']')?;
            let inside = inside.trim();
            match inside.parse::<usize>() {
                Ok(index) => out.push_str(&format!("[{index}]")),
                Err(_) => {
                    let key = quoted(inside)?;
                    if key.is_empty() || key.contains(['.', '[', ']']) {
                        return None;
                    }
                    out.push('.');
                    out.push_str(key);
                }
            }
            left = tail;
        }
    }
    Some(out)
}

/// What is between a pair of matching quotes, when that is all there is.
fn quoted(text: &str) -> Option<&str> {
    let text = text.trim();
    let open = text.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let inside = text.strip_prefix(open)?.strip_suffix(open)?;
    (!inside.contains(open)).then_some(inside)
}

/// One line of an `assert` block: `res.status: eq 200`.
///
/// Only the operators volt has a check that means the same thing for. `gte`
/// is not `over`, and a check that passes where the original would have
/// failed is a test that lies — so those are left for the report.
fn assertion(entry: &Entry) -> Option<Check> {
    let from = source(&entry.name)?;
    let (operator, operand) = match entry.value.trim().split_once(char::is_whitespace) {
        Some((operator, operand)) => (operator, operand.trim()),
        None => (entry.value.trim(), ""),
    };
    let operand = quoted(operand).unwrap_or(operand).to_string();

    let (op, value) = match operator {
        "eq" => (Op::Is, operand),
        "neq" => (Op::IsNot, operand),
        "contains" => (Op::Contains, operand),
        "matches" => (Op::Matches, operand.strip_prefix('/').and_then(|p| p.strip_suffix('/')).map(str::to_string).unwrap_or(operand)),
        "startsWith" => (Op::Matches, format!("^{}", regex::escape(&operand))),
        "endsWith" => (Op::Matches, format!("{}$", regex::escape(&operand))),
        "gt" => (Op::Over, operand),
        "lt" => (Op::Under, operand),
        "isDefined" => (Op::Exists, String::new()),
        "isUndefined" => (Op::Missing, String::new()),
        // `res.status: 200` with no operator is Bruno's shorthand for `eq`.
        other if operand.is_empty() && !other.is_empty() && !OPERATORS.contains(&other) => {
            (Op::Is, quoted(other).unwrap_or(other).to_string())
        }
        _ => return None,
    };
    // A comparison needs something to compare with.
    if value.is_empty() && !matches!(op, Op::Exists | Op::Missing) {
        return None;
    }
    Some(Check { from, op, value, enabled: entry.enabled })
}

/// Every operator Bruno's assertions have, so that one volt cannot express is
/// told apart from a bare value.
const OPERATORS: [&str; 28] = [
    "eq", "neq", "gt", "gte", "lt", "lte", "in", "notIn", "contains", "notContains", "length", "matches",
    "notMatches", "startsWith", "endsWith", "between", "isEmpty", "isNotEmpty", "isNull", "isUndefined",
    "isDefined", "isTruthy", "isFalsy", "isJson", "isNumber", "isString", "isBoolean", "isArray",
];

/// Bruno's names for the things a test script touches, and Postman's for the
/// same things. `scripts::translate` reads the second set, and the shapes are
/// otherwise identical — chai is chai — so a rename is the whole translation.
/// Longest first, so `res.getBody()` is not half-renamed as `res.body`.
const NAMES: [(&str, &str); 12] = [
    ("res.getResponseTime()", "pm.response.responseTime"),
    ("res.getStatus()", "pm.response.code"),
    ("res.getBody()", "pm.response.json()"),
    ("res.getHeader(", "pm.response.headers.get("),
    ("res.responseTime", "pm.response.responseTime"),
    ("res.status", "pm.response.code"),
    ("res.body", "pm.response.json()"),
    ("bru.setGlobalEnvVar(", "pm.globals.set("),
    ("bru.setEnvVar(", "pm.environment.set("),
    ("bru.setVar(", "pm.variables.set("),
    ("expect(", "pm.expect("),
    ("test(", "pm.test("),
];

fn in_postman_terms(code: &str) -> String {
    NAMES.iter().fold(code.to_string(), |code, (bruno, postman)| rename(&code, bruno, postman))
}

/// A line the translation left, put back the way its author wrote it.
fn in_bruno_terms(line: &str) -> String {
    // The shorter spelling of each pair, which is the one people write.
    let back: [(&str, &str); 8] = [
        ("pm.response.responseTime", "res.responseTime"),
        ("pm.response.code", "res.status"),
        ("pm.response.json()", "res.body"),
        ("pm.response.headers.get(", "res.getHeader("),
        ("pm.globals.set(", "bru.setGlobalEnvVar("),
        ("pm.environment.set(", "bru.setEnvVar("),
        ("pm.variables.set(", "bru.setVar("),
        ("pm.expect(", "expect("),
    ];
    back.iter().fold(line.replace("pm.test(", "test("), |line, (postman, bruno)| line.replace(postman, bruno))
}

/// Replace `from` where it stands as a name of its own: not as the tail of a
/// longer one (`myres.body`, `pm.expect(`) and not as the head of one
/// (`res.statusText`).
fn rename(code: &str, from: &str, to: &str) -> String {
    let part_of_a_name = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(at) = rest.find(from) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + from.len()..].chars().next();
        let ends_in_a_name = from.chars().next_back().is_some_and(part_of_a_name);
        let whole = !before.is_some_and(|c| part_of_a_name(c) || c == '.')
            && !(ends_in_a_name && after.is_some_and(part_of_a_name));
        out.push_str(&rest[..at]);
        out.push_str(if whole { to } else { from });
        rest = &rest[at + from.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import;

    const CREATE: &str = r#"meta {
  name: Create user
  type: http
  seq: 2
}

post {
  url: {{baseUrl}}/teams/:team/users?notify=true
  body: json
  auth: bearer
}

params:query {
  notify: true
  ~dry_run: 1
}

params:path {
  team: core
  ghost: 1
}

headers {
  accept: application/json
  ~x-debug: 1
  x-tenant: {{tenant}}
}

auth:bearer {
  token: eyJ.literal.token
}

body:json {
  {
    "name": "ada",
    "plan": "{{plan}}"
  }
}

vars:pre-request {
  plan: pro
  ~tenant: off
  unused: 1
}

vars:post-response {
  userId: res.body.data.id
  first: res.body.items[0]["name"]
  etag: res.headers.etag
  computed: res.body.items.length + 1
}

assert {
  res.status: eq 201
  res.body.data.name: neq "bob"
  res.body.data.email: contains @
  res.body.data.id: matches /^[0-9]+$/
  res.body.data.slug: startsWith u.
  res.responseTime: lt 500
  res.body.data.age: gt 17
  res.body.data.id: isDefined
  res.body.data.deleted: isUndefined
  res.body.count: gte 1
  res.body.tags: length 2
  res.headers.content-type: contains json
  ~res.body.data.flag: eq true
  res.status: 201
}

script:pre-request {
  req.setHeader("x-now", Date.now());
}

script:post-response {
  bru.setEnvVar("token", res.body.access_token);
  if (res.status === 500) { console.log("oh"); }
}

tests {
  test("it was created", function () {
    expect(res.getStatus()).to.equal(201);
    expect(res.getBody().data.name).to.eql("ada");
  });
}

docs {
  Creates a user.

  Needs an admin token.
}
"#;

    fn read_one(text: &str) -> (Option<Request>, Warnings) {
        let mut w = Warnings::default();
        let request = request(&bru::parse(text), "file-name", &mut w);
        (request, w)
    }

    fn said(w: &Warnings, needle: &str) -> bool {
        w.render().iter().any(|line| line.contains(needle))
    }

    #[test]
    fn a_request_file_becomes_a_request() {
        let (request, w) = read_one(CREATE);
        let request = request.expect("an http request");

        assert_eq!(request.name, "Create user");
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "{{baseUrl}}/teams/core/users", "the query moved out and the path parameter was written in");
        assert_eq!(
            request.params.iter().map(|p| (p.name.as_str(), p.enabled)).collect::<Vec<_>>(),
            [("notify", true), ("dry_run", false)],
            "from the query block, which also has the switched-off one"
        );
        assert!(said(&w, "A path parameter with no `:name` in the URL was left out"));

        assert_eq!(request.headers.len(), 3);
        assert!(!request.headers[1].enabled, "~ is off");
        assert!(matches!(&request.auth, Auth::Bearer { token } if token == "eyJ.literal.token"));
        assert_eq!(request.docs.as_deref(), Some("Creates a user.\n\nNeeds an admin token."));

        let Body::Json { content } = &request.body else { panic!("{:?}", request.body) };
        assert_eq!(content, "{\n  \"name\": \"ada\",\n  \"plan\": \"pro\"\n}", "dedented, and the pre-request value written in");
        assert_eq!(request.headers[2].value, "{{tenant}}", "a switched-off variable is not written anywhere");
        assert!(said(&w, "Pre-request variables were written into the request"));
        assert!(said(&w, "A pre-request variable the request itself does not use was left out"));
    }

    #[test]
    fn assertions_become_checks_where_an_operator_means_the_same() {
        let (request, w) = read_one(CREATE);
        let checks = request.unwrap().checks;
        let check = |from: &str, op: Op| checks.iter().find(|c| c.from == from && c.op == op).unwrap_or_else(|| panic!("no {from} {op:?} in {checks:?}"));

        assert_eq!(check("status", Op::Is).value, "201");
        assert_eq!(check("$.data.name", Op::IsNot).value, "bob", "quotes are the script's, not the value's");
        assert_eq!(check("$.data.email", Op::Contains).value, "@");
        assert_eq!(check("$.data.id", Op::Matches).value, "^[0-9]+$");
        assert_eq!(check("$.data.slug", Op::Matches).value, r"^u\.", "startsWith is a pattern anchored at the start");
        assert_eq!(check("time", Op::Under).value, "500");
        assert_eq!(check("$.data.age", Op::Over).value, "17");
        check("$.data.id", Op::Exists);
        check("$.data.deleted", Op::Missing);
        assert_eq!(check("header:content-type", Op::Contains).value, "json");
        assert!(!check("$.data.flag", Op::Is).enabled, "a switched-off assertion stays one");
        assert_eq!(checks.iter().filter(|c| c.from == "status" && c.value == "201").count(), 3, "bare `201` is eq, and the test said it again");

        assert!(said(&w, "An assertion was not imported — `res.body.count: gte 1`"), "{:?}", w.render());
        assert!(said(&w, "An assertion was not imported — `res.body.tags: length 2`"));
    }

    #[test]
    fn what_a_script_and_post_response_variables_keep_become_captures() {
        let (request, w) = read_one(CREATE);
        let request = request.unwrap();
        let capture = |name: &str| request.captures.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {name}"));

        assert_eq!(capture("userId").from, "$.data.id");
        assert_eq!(capture("first").from, "$.items[0].name", "a list index is a path volt can read");
        assert_eq!(capture("etag").from, "header:etag");
        assert_eq!(capture("token").from, "$.access_token", "bru.setEnvVar is a capture");
        assert!(request.captures.iter().all(|c| c.secret), "kept out of the YAML until proved harmless");

        assert!(said(&w, "A post-response variable was not imported — `computed: res.body.items.length + 1`"), "{:?}", w.render());
        assert!(said(&w, "Test scripts were read as checks and captures"));
        assert!(request.checks.iter().any(|c| c.from == "$.data.name" && c.op == Op::Is && c.value == "ada"), "{:?}", request.checks);
        // What was not understood is quoted the way it was written.
        assert!(
            w.render().iter().any(|line| line.contains("A line of a test script was not imported") && line.contains("res.status === 500")),
            "{:?}",
            w.render()
        );
        assert!(said(&w, "Pre-request scripts are not imported"));
    }

    #[test]
    fn every_kind_of_body_and_auth_is_read() {
        let form = read_one("post {\n  url: https://x.test\n  body: formUrlEncoded\n  auth: basic\n}\nbody:form-urlencoded {\n  a: 1\n  ~b: 2\n}\nauth:basic {\n  username: ada\n  password: hunter2\n}\n").0.unwrap();
        let Body::UrlEncoded { fields } = &form.body else { panic!("{:?}", form.body) };
        assert_eq!((fields.len(), fields[1].enabled), (2, false));
        assert!(matches!(&form.auth, Auth::Basic { username, password } if username == "ada" && password == "hunter2"));

        let (multi, w) = read_one("post {\n  url: https://x.test\n  body: multipartForm\n  auth: apikey\n}\nbody:multipart-form {\n  caption: me\n  photo: @file(pics/me.png) @contentType(image/png)\n}\nauth:apikey {\n  key: api_key\n  value: k\n  placement: queryparams\n}\n");
        let multi = multi.unwrap();
        let Body::Form { fields } = &multi.body else { panic!("{:?}", multi.body) };
        assert!(!fields[0].file && fields[1].file);
        assert_eq!(fields[1].value, "pics/me.png");
        assert!(said(&w, "A file field keeps the path"));
        assert!(matches!(&multi.auth, Auth::ApiKey { location: ApiKeyLocation::Query, .. }));

        let graphql = read_one("meta {\n  type: graphql\n}\npost {\n  url: https://x.test/graphql\n  body: graphql\n  auth: inherit\n}\nbody:graphql {\n  query ($id: ID!) {\n    user(id: $id) { name }\n  }\n}\nbody:graphql:vars {\n  { \"id\": 7 }\n}\n").0.unwrap();
        assert_eq!(graphql.name, "file-name", "no name in the file: the file's own");
        assert!(matches!(&graphql.body, Body::GraphQl { query, variables } if query.starts_with("query ($id: ID!) {\n  user") && variables == "{ \"id\": 7 }"), "{:?}", graphql.body);
        assert!(matches!(graphql.auth, Auth::Inherit));

        let aws = read_one("get {\n  url: https://x.test\n  auth: awsv4\n}\nauth:awsv4 {\n  accessKeyId: AKID\n  secretAccessKey: shh\n  service: s3\n}\n").0.unwrap();
        assert!(matches!(&aws.auth, Auth::AwsSigV4 { key_id, region, service, .. } if key_id == "AKID" && region == "us-east-1" && service == "s3"));
        assert!(matches!(aws.body, Body::None));
        assert!(matches!(read_one("get {\n  url: https://x.test\n}\n").0.unwrap().auth, Auth::None), "saying nothing is none");

        // An older file that names no body mode still has the body it has.
        let old = read_one("post {\n  url: https://x.test\n}\nbody:xml {\n  <a/>\n}\nquery {\n  q: 1\n}\n").0.unwrap();
        assert!(matches!(&old.body, Body::Xml { content } if content == "<a/>"));
        assert_eq!(old.params.len(), 1, "and `query` is what `params:query` used to be called");
    }

    #[test]
    fn what_volt_does_not_import_is_said_rather_than_guessed() {
        let (oauth, w) = read_one("get {\n  url: https://x.test\n  auth: oauth2\n}\nauth:oauth2 {\n  grant_type: client_credentials\n}\n");
        assert!(matches!(oauth.unwrap().auth, Auth::Inherit));
        assert!(said(&w, "OAuth 2.0 is a step in volt"));

        let (grpc, w) = read_one("meta {\n  name: Watch\n  type: grpc\n}\ngrpc {\n  url: x.test:443\n}\n");
        assert!(grpc.is_none());
        assert!(said(&w, "gRPC and WebSocket requests are not supported and were skipped: Watch"));

        let (none, w) = read_one("meta {\n  name: Notes\n}\n");
        assert!(none.is_none());
        assert!(said(&w, "A file with no request in it was skipped"));
    }

    #[test]
    fn a_source_is_read_only_when_it_is_a_place_and_not_a_computation() {
        assert_eq!(source("res.body").as_deref(), Some("body"));
        assert_eq!(source("res.body.a.b").as_deref(), Some("$.a.b"));
        assert_eq!(source("res.body[2].id").as_deref(), Some("$[2].id"));
        assert_eq!(source("res.body['a-b']").as_deref(), Some("$.a-b"));
        assert_eq!(source("res.body['a.b']"), None, "a key volt's paths cannot spell");
        assert_eq!(source("res.getHeader('X-Id')").as_deref(), Some("header:X-Id"));
        assert_eq!(source("res.headers[\"x-id\"]").as_deref(), Some("header:x-id"));
        assert_eq!(source("res.body.items.filter(x => x.ok)"), None);
        assert_eq!(source("res.body.a + 1"), None);
        assert_eq!(source("req.url"), None);
        assert_eq!(source("res('data.token')"), None);
    }

    #[test]
    fn renaming_touches_whole_names_only() {
        assert_eq!(in_postman_terms("expect(res.status).to.equal(200)"), "pm.expect(pm.response.code).to.equal(200)");
        assert_eq!(in_postman_terms("res.statusText"), "res.statusText", "the head of a longer name is left alone");
        assert_eq!(in_postman_terms("myres.body"), "myres.body", "and so is the tail of one");
        assert_eq!(in_postman_terms("chai.expect(x)"), "chai.expect(x)");
        assert_eq!(in_postman_terms("res.getBody().a"), "pm.response.json().a");
        assert_eq!(in_bruno_terms("if (pm.response.code === 500) { pm.expect(x) }"), "if (res.status === 500) { expect(x) }");
    }

    /// A whole collection on disk, read and then written as a volt one.
    #[test]
    fn a_bruno_folder_imports_as_a_collection_with_its_tree_scopes_and_environments() {
        let base = std::env::temp_dir().join(format!("volt-bruno-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let source = base.join("shop-bruno");
        let write = |relative: &str, text: &str| {
            let path = source.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("bruno.json", r#"{ "version": "1", "name": "Shop", "type": "collection", "ignore": ["node_modules", ".git"] }"#);
        write("collection.bru", "headers {\n  x-client: bruno\n}\n\nauth {\n  mode: bearer\n}\n\nauth:bearer {\n  token: collection-literal-token\n}\n\nvars:pre-request {\n  apiVersion: v2\n  apiSecret: live_s3cret_value\n}\n\nscript:pre-request {\n  console.log(1)\n}\n");
        write("environments/Local.bru", "vars {\n  baseUrl: http://localhost:3000\n}\n");
        write("environments/Prod.bru", "vars {\n  baseUrl: https://api.shop.test\n}\nvars:secret [\n  token,\n  ~retired\n]\n");
        write("Users/folder.bru", "meta {\n  name: Users & teams\n  seq: 1\n}\n\nheaders {\n  x-area: users\n}\n\nauth {\n  mode: basic\n}\n\nauth:basic {\n  username: svc\n  password: folder-pass-123\n}\n\nvars:pre-request {\n  pageSize: 50\n}\n");
        write("Users/list.bru", "meta {\n  name: List users\n  type: http\n  seq: 2\n}\n\nget {\n  url: {{baseUrl}}/users\n  auth: inherit\n}\n");
        write("Users/create.bru", CREATE);
        write("Users/get.bru", "meta {\n  name: Get user\n  seq: 1\n}\n\nget {\n  url: {{baseUrl}}/users/{{userId}}\n  auth: inherit\n}\n");
        write("Admin/ping.bru", "meta {\n  name: Ping\n}\n\nget {\n  url: {{baseUrl}}/ping\n  auth: none\n}\n");
        write("Admin/stream.bru", "meta {\n  name: Live\n  type: ws\n}\n");
        write("health.bru", "meta {\n  name: Health\n  seq: 1\n}\n\nget {\n  url: {{baseUrl}}/health\n}\n");
        write("node_modules/pkg/x.bru", "get {\n  url: https://never.test\n}\n");
        write("README.md", "not a request");

        let into = base.join("out");
        fs::create_dir_all(&into).unwrap();

        // The folder and its `bruno.json` are two ways of naming the same thing.
        assert_eq!(collection_dir(&source), Some(source.clone()));
        assert_eq!(collection_dir(&source.join("bruno.json")), Some(source.clone()));
        assert_eq!(collection_dir(&source.join("Users")), None);
        assert_eq!(collection_dir(&source.join("health.bru")), None);

        let outcome = import::import_file(&source.join("bruno.json"), &into).expect("it imports");
        let report = &outcome.report;
        assert_eq!(report.format, "Bruno");
        assert_eq!((report.requests, report.folders), (5, 2), "{:?}", report.warnings);
        assert_eq!(report.environments, ["Local", "Prod"]);

        // Folders before requests; `seq` first, then names, with none last.
        let names = |nodes: &[crate::collection::Node]| -> Vec<String> {
            nodes
                .iter()
                .map(|node| match node {
                    crate::collection::Node::Folder { name, .. } | crate::collection::Node::Request { name, .. } => name.clone(),
                })
                .collect()
        };
        let tree = &outcome.collection.tree;
        assert_eq!(names(tree), ["Users & teams", "Admin", "Health"]);
        let crate::collection::Node::Folder { id: users_id, children, .. } = &tree[0] else { panic!() };
        assert_eq!(names(children), ["Get user", "Create user", "List users"]);

        // The collection and the folder keep what they pass down.
        let root = PathBuf::from(&report.path);
        let meta = &outcome.collection.meta;
        assert_eq!(meta.name, "Shop");
        assert_eq!(meta.headers[0].name, "x-client");
        assert!(matches!(&meta.auth, Auth::Bearer { token } if token == "{{token2}}"), "{:?}", meta.auth);
        assert_eq!(meta.vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["apiVersion"], "the secret one went to the environments");
        let folder = crate::collection::folder_meta(&root, users_id).unwrap();
        assert_eq!(folder.headers[0].value, "users");
        assert!(matches!(&folder.auth, Auth::Basic { username, password } if username == "svc" && password == "{{password}}"));
        assert_eq!(folder.vars[0].name, "pageSize");

        // Environments: values as written, secret names empty and secret.
        let prod = outcome.collection.environments.iter().find(|env| env.name == "Prod").unwrap();
        let var = |name: &str| prod.vars.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no {name} in {:?}", prod.vars));
        assert_eq!(var("baseUrl").value, "https://api.shop.test");
        assert!(var("token").secret && var("token").value.is_empty(), "Bruno's own secret: the name, and no value");
        assert_eq!(var("token2").value, "collection-literal-token", "a literal does not take over a name that is in use");
        assert_eq!(var("apiSecret").value, "live_s3cret_value");
        assert!(var("apiSecret").secret && var("password").secret);
        assert!(!prod.vars.iter().any(|v| v.name == "retired"));

        // And nothing literal reached a committed file.
        let mut yaml = String::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "yaml") {
                    yaml.push_str(&fs::read_to_string(&path).unwrap());
                }
            }
        }
        for literal in ["collection-literal-token", "folder-pass-123", "live_s3cret_value", "eyJ.literal.token"] {
            assert!(!yaml.contains(literal), "`{literal}` was written into the YAML");
        }
        assert!(!yaml.contains("never.test"), "node_modules is not part of a collection");

        let warnings = report.warnings.join("\n");
        assert!(warnings.contains("gRPC and WebSocket requests are not supported and were skipped: Live"), "{warnings}");
        assert!(warnings.contains("Scripts on a collection or a folder are not imported: Shop"), "{warnings}");
        assert!(warnings.contains("Secret variables came across by name only"), "{warnings}");

        // A folder that is not a Bruno collection says so.
        let said = import::import_file(&source.join("Users"), &into).err().expect("refused").to_string();
        assert!(said.contains("bruno.json"), "{said}");

        let _ = fs::remove_dir_all(&base);
    }
}

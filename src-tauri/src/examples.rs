//! Saved example responses.
//!
//! A response you want to keep — "this is what a 404 looks like here" — stored
//! as a file in the collection so it is reviewable and shareable like the
//! requests are. They live under `.examples/`, which the tree skips because it
//! ignores dotted names, so an example never turns up as a request.
//!
//! Secrets go through the same redaction history uses: an example is committed,
//! and a token in a committed file is the thing volt exists to avoid.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::collection;
use crate::error::{Error, Result};
use crate::history;
use crate::http::HttpResponse;
use crate::model::{EnvVar, KeyValue};

const DIR: &str = ".examples";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub name: String,
    /// Unix milliseconds, so the UI can say when it was taken.
    pub at: u64,
    pub status: u16,
    #[serde(default)]
    pub status_text: String,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub body_is_base64: bool,
}

impl Example {
    pub fn of(name: &str, response: &HttpResponse) -> Example {
        Example {
            name: name.trim().to_string(),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response.headers.clone(),
            body: response.body.clone(),
            body_is_base64: response.body_is_base64,
        }
    }
}

/// `users/list-users.yaml` → `<root>/.examples/users/list-users.yaml`.
fn file_for(root: &Path, id: &str) -> Result<PathBuf> {
    // Through `resolve`, so an id cannot climb out of the collection.
    let inside = collection::resolve_id(root, id)?;
    let relative = inside.strip_prefix(root).map_err(|_| Error::Invalid("that request is not in this collection".into()))?;
    Ok(root.join(DIR).join(relative))
}

pub fn list(root: &Path, id: &str) -> Result<Vec<Example>> {
    let path = file_for(root, id)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
    collection::from_yaml(&text)
}

/// Keep a response. `vars` is the active environment, for redaction.
pub fn save(root: &Path, id: &str, name: &str, response: &HttpResponse, vars: &[EnvVar]) -> Result<Vec<Example>> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("an example needs a name".into()));
    }
    let mut examples = list(root, id)?;
    // Two passes, because they catch different things: the secret pass replaces
    // values the user declared, and the cookie pass hides the ones the server
    // set, which are in no environment and so invisible to the first.
    let mut example = history::redacted(&Example::of(name, response), vars);
    history::hide_cookies(&mut example.headers);

    match examples.iter_mut().find(|e| e.name == example.name) {
        Some(existing) => *existing = example,
        None => examples.push(example),
    }
    write(root, id, &examples)?;
    Ok(examples)
}


/// Carry a node's kept examples with it when it moves.
///
/// One rename covers a folder's whole subtree as well as a single request,
/// because the path under `.examples/` mirrors the id exactly.
pub(crate) fn move_for(root: &Path, old_id: &str, new_id: &str) -> Result<()> {
    let from = file_for(root, old_id)?;
    if !from.exists() {
        return Ok(());
    }
    let to = file_for(root, new_id)?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent.to_string_lossy().into_owned(), e))?;
    }
    match fs::rename(&from, &to) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::io(to.to_string_lossy().into_owned(), e))
        }
        _ => Ok(()),
    }
}

pub fn delete(root: &Path, id: &str, name: &str) -> Result<Vec<Example>> {
    let mut examples = list(root, id)?;
    examples.retain(|e| e.name != name);
    write(root, id, &examples)?;
    Ok(examples)
}

fn write(root: &Path, id: &str, examples: &[Example]) -> Result<()> {
    let path = file_for(root, id)?;
    if examples.is_empty() {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(Error::io(path.to_string_lossy().into_owned(), e))
            }
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent.to_string_lossy().into_owned(), e))?;
    }
    fs::write(&path, collection::to_yaml(&examples.to_vec())?).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))
}

/// What changed between a kept example and the response that just came back.
///
/// Structure, not lines: a JSON diff of two bodies is easy to render and hard
/// to read, and the question an example answers is "is this still the same
/// shape". So: the status, the content type, which paths appeared or vanished,
/// and which hold a different kind of thing now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub example: String,
    /// `[kept, now]` when the status is not the one that was kept.
    pub status: Option<[u16; 2]>,
    pub content_type: Option<[String; 2]>,
    /// Paths into the body, as `$.data.items[].id`.
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub retyped: Vec<Retyped>,
    /// Nothing above found anything.
    pub same: bool,
    /// Why the bodies could not be compared as structures, when they could not.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Retyped {
    pub path: String,
    pub was: String,
    pub now: String,
}

pub fn compare(root: &Path, id: &str, name: &str, response: &HttpResponse) -> Result<Comparison> {
    let examples = list(root, id)?;
    let kept = examples
        .into_iter()
        .find(|example| example.name == name)
        .ok_or_else(|| Error::Invalid(format!("no example named {name}")))?;

    let mut out = Comparison {
        example: kept.name.clone(),
        status: (kept.status != response.status).then_some([kept.status, response.status]),
        content_type: None,
        added: Vec::new(),
        removed: Vec::new(),
        retyped: Vec::new(),
        same: false,
        note: None,
    };

    let was_type = content_type(&kept.headers);
    let now_type = content_type(&response.headers);
    if was_type != now_type {
        out.content_type = Some([was_type, now_type]);
    }

    if kept.body_is_base64 || response.body_is_base64 {
        out.note = Some("One of these is binary, so only the status and the type are compared.".into());
    } else {
        match (
            serde_json::from_str::<serde_json::Value>(&kept.body),
            serde_json::from_str::<serde_json::Value>(&response.body),
        ) {
            (Ok(was), Ok(now)) => {
                let (was, now) = (shape_of(&was), shape_of(&now));
                out.added = only_in(&now, &was);
                out.removed = only_in(&was, &now);
                out.retyped = was
                    .iter()
                    .filter_map(|(path, kind)| {
                        let other = now.get(path)?;
                        (other != kind).then(|| Retyped {
                            path: path.clone(),
                            was: (*kind).to_string(),
                            now: (*other).to_string(),
                        })
                    })
                    .collect();
            }
            _ => {
                let note = if kept.body == response.body {
                    "Not JSON, and the two bodies are the same text."
                } else {
                    "Not JSON, so only the status and the type are compared."
                };
                out.note = Some(note.into());
            }
        }
    }

    out.same = out.status.is_none()
        && out.content_type.is_none()
        && out.added.is_empty()
        && out.removed.is_empty()
        && out.retyped.is_empty();
    Ok(out)
}

fn content_type(headers: &[KeyValue]) -> String {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Every path in a body and the kind of thing it holds. A list contributes one
/// path per field across all of its items, so a hundred rows read as one shape
/// and a field missing from some of them still shows up.
fn shape_of(value: &serde_json::Value) -> BTreeMap<String, &'static str> {
    let mut out = BTreeMap::new();
    walk(value, "$", &mut out);
    out
}

fn walk(value: &serde_json::Value, path: &str, out: &mut BTreeMap<String, &'static str>) {
    let kind = kind_of(value);
    match out.entry(path.to_string()) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(kind);
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            if *slot.get() != kind {
                // A list whose items disagree has no one shape; saying so beats
                // reporting whichever item happened to come last.
                slot.insert("mixed");
            }
        }
    }

    match value {
        serde_json::Value::Object(fields) => {
            for (name, field) in fields {
                walk(field, &format!("{path}.{name}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

fn kind_of(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Paths in `these` and not in `those`, with anything below a path that is
/// itself missing left out: one line saying `$.meta` is gone beats five saying
/// its fields are.
fn only_in(
    these: &BTreeMap<String, &'static str>,
    those: &BTreeMap<String, &'static str>,
) -> Vec<String> {
    these
        .keys()
        .filter(|path| !those.contains_key(*path))
        .filter(|path| parent_of(path).is_none_or(|parent| those.contains_key(parent)))
        .cloned()
        .collect()
}

fn parent_of(path: &str) -> Option<&str> {
    if let Some(head) = path.strip_suffix("[]") {
        return Some(head);
    }
    let at = path.rfind('.')?;
    (at > 0).then(|| &path[..at])
}
#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("volt-ex-{label}-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join("users")).unwrap();
        fs::write(root.join("users/list.yaml"), "name: List\nmethod: GET\nurl: /u\n").unwrap();
        root
    }

    fn response(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue {
                name: "Set-Cookie".into(),
                value: "session=live-token-value".into(),
                enabled: true,
                description: None,
            }],
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: 3,
            time_to_first_byte_ms: 1,
            final_url: String::new(),
            sent_url: String::new(),
            sent_method: "GET".into(),
            sent_headers: Vec::new(),
            sent_body_bytes: 0,
            redirects: Vec::new(),
            version: "HTTP/1.1".into(),
            missing_vars: Vec::new(),
        }
    }

    #[test]
    fn an_example_is_a_file_beside_the_collection_not_inside_the_tree() {
        let root = root("save");
        save(&root, "users/list.yaml", "Happy path", &response("{}"), &[]).unwrap();

        let path = root.join(".examples/users/list.yaml");
        assert!(path.is_file(), "it lands under a dotted directory the tree skips");
        assert_eq!(list(&root, "users/list.yaml").unwrap().len(), 1);

        // Saving under the same name replaces rather than piles up.
        save(&root, "users/list.yaml", "Happy path", &response("{\"v\":2}"), &[]).unwrap();
        let kept = list(&root, "users/list.yaml").unwrap();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].body.contains("\"v\":2"));

        delete(&root, "users/list.yaml", "Happy path").unwrap();
        assert!(list(&root, "users/list.yaml").unwrap().is_empty());
        assert!(!path.exists(), "the file goes when the last example does");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_secret_never_reaches_a_saved_example() {
        let root = root("redact");
        let vars = vec![EnvVar { name: "token".into(), value: "live-token-value".into(), secret: true }];
        save(&root, "users/list.yaml", "With auth", &response("{\"t\":\"live-token-value\"}"), &vars).unwrap();

        let text = fs::read_to_string(root.join(".examples/users/list.yaml")).unwrap();
        assert!(!text.contains("live-token-value"), "a committed file would carry the token: {text}");
        assert!(text.contains("{{token}}"), "{text}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_id_cannot_climb_out_of_the_collection() {
        let root = root("escape");
        assert!(save(&root, "../elsewhere.yaml", "x", &response("{}"), &[]).is_err());
        assert!(list(&root, "..").is_err());
        fs::remove_dir_all(&root).ok();
    }

    /// Same helper, but with a status and a content type of its own.
    fn reply(status: u16, content_type: &str, body: &str) -> HttpResponse {
        let mut out = response(body);
        out.status = status;
        out.headers = vec![KeyValue {
            name: "Content-Type".into(),
            value: content_type.into(),
            enabled: true,
            description: None,
        }];
        out
    }

    #[test]
    fn comparing_against_an_example_reads_the_shape_not_the_lines() {
        let root = root("compare");
        let kept = reply(200, "application/json", r#"{"id":1,"name":"Ada","tags":["x"]}"#);
        save(&root, "users/list.yaml", "Kept", &kept, &[]).unwrap();

        // The same shape with different values is not a change.
        let same = reply(200, "application/json; charset=utf-8", r#"{"id":7,"name":"Grace","tags":["y","z"]}"#);
        let out = compare(&root, "users/list.yaml", "Kept", &same).unwrap();
        assert!(out.same, "different values, same shape: {out:?}");

        // A field gone, a field new, a field that is a different kind of thing.
        let drifted = reply(200, "application/json", r#"{"id":"7","email":"a@b.c","tags":["y"]}"#);
        let out = compare(&root, "users/list.yaml", "Kept", &drifted).unwrap();
        assert!(!out.same);
        assert_eq!(out.removed, vec!["$.name"]);
        assert_eq!(out.added, vec!["$.email"]);
        assert_eq!(out.retyped.len(), 1);
        assert_eq!(out.retyped[0].path, "$.id");
        assert_eq!((out.retyped[0].was.as_str(), out.retyped[0].now.as_str()), ("number", "string"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_status_or_a_type_that_changed_is_the_first_thing_said() {
        let root = root("compare-status");
        save(&root, "users/list.yaml", "Kept", &reply(200, "application/json", "{}"), &[]).unwrap();

        let out = compare(&root, "users/list.yaml", "Kept", &reply(404, "text/html", "<p>no</p>")).unwrap();
        assert_eq!(out.status, Some([200, 404]));
        assert_eq!(out.content_type, Some(["application/json".into(), "text/html".into()]));
        assert!(out.note.as_deref().unwrap().contains("Not JSON"));
        assert!(!out.same);

        assert!(compare(&root, "users/list.yaml", "Nope", &reply(200, "application/json", "{}")).is_err());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_list_reads_as_one_shape_and_a_whole_branch_is_reported_once() {
        let root = root("compare-list");
        let kept = reply(
            200,
            "application/json",
            r#"{"data":[{"id":1,"meta":{"seen":true}},{"id":2,"meta":{"seen":false}}]}"#,
        );
        save(&root, "users/list.yaml", "Kept", &kept, &[]).unwrap();

        // A hundred rows, no meta on any of them.
        let rows: Vec<String> = (0..100).map(|n| format!(r#"{{"id":{n}}}"#)).collect();
        let now = reply(200, "application/json", &format!(r#"{{"data":[{}]}}"#, rows.join(",")));
        let out = compare(&root, "users/list.yaml", "Kept", &now).unwrap();

        assert_eq!(out.removed, vec!["$.data[].meta"], "the branch once, not its fields as well");
        assert!(out.added.is_empty());
        assert!(out.retyped.is_empty());

        // A list whose items disagree has no one shape.
        let mixed = reply(200, "application/json", r#"{"data":[{"id":1,"meta":{}},"just text"]}"#);
        let out = compare(&root, "users/list.yaml", "Kept", &mixed).unwrap();
        assert_eq!(out.retyped[0].path, "$.data[]");
        assert_eq!(out.retyped[0].now, "mixed");

        fs::remove_dir_all(&root).ok();
    }

    /// An example is written into the collection and committed, so the cookie a
    /// server set has to be hidden here too — the secret pass cannot see it,
    /// because the user never declared it as a variable.
    #[test]
    fn a_session_cookie_never_reaches_a_saved_example() {
        let root = root("cookie");
        let mut response = response("{}");
        response.headers = vec![
            KeyValue {
                name: "Set-Cookie".into(),
                value: "session=eyJhbGciOiJIUzI1NiJ9.aaaaaaaa; Path=/; HttpOnly; Secure".into(),
                enabled: true,
                description: None,
            },
            KeyValue {
                name: "Set-Cookie".into(),
                value: "theme=dark".into(),
                enabled: true,
                description: None,
            },
        ];

        // No environment at all: the secret pass returns the response untouched,
        // which is exactly when the cookie pass has to carry it.
        save(&root, "users/list.yaml", "Kept", &response, &[]).unwrap();

        let written = fs::read_to_string(root.join(".examples/users/list.yaml")).unwrap();
        assert!(!written.contains("eyJhbGciOiJIUzI1NiJ9"), "the token is gone:\n{written}");
        assert!(written.contains("session=…"), "the name stays, so the reader knows what was set");
        assert!(written.contains("Path=/; HttpOnly; Secure"), "and so do the attributes");
        assert!(written.contains("theme=…"), "every cookie, not just the one that looks secret");

        fs::remove_dir_all(&root).ok();
    }
}

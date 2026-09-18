//! Request history.
//!
//! Lives in the app's data directory, never in the collection: history is
//! personal, would churn every diff, and holds real responses. Each collection
//! gets one append-only JSON Lines file, named by a hash of its root path.
//!
//! Appending is O(1). The file is only rewritten when it outgrows
//! `MAX_FILE_BYTES`, at which point the newest entries that fit in half that
//! budget are kept.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http::HttpResponse;
use crate::model::{EnvVar, KeyValue, Request};

/// How many entries the history list shows.
pub const MAX_ENTRIES: usize = 200;
/// Response bodies beyond this are cut, so one large download cannot crowd
/// out everything else.
pub const MAX_BODY_BYTES: usize = 128 * 1024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Secrets shorter than this are not redacted: replacing every `1` or `ok` in a
/// response would wreck it, and a value that short protects nothing anyway.
const MIN_REDACT_LEN: usize = 4;

/// Two sends finishing together must not interleave their lines.
static FILE_LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    /// Unix milliseconds.
    pub at: u64,
    /// The collection file it was sent from; `None` when sent from history.
    pub request_id: Option<String>,
    pub environment: Option<String>,
    /// The request as written in the editor, `{{variables}}` unresolved.
    pub request: Request,
    pub response: Option<HttpResponse>,
    #[serde(default)]
    pub body_truncated: bool,
    pub error: Option<String>,
}

/// What the history list needs, without the bodies.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub at: u64,
    pub request_id: Option<String>,
    pub environment: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

impl From<&Entry> for Summary {
    fn from(entry: &Entry) -> Self {
        Summary {
            id: entry.id.clone(),
            at: entry.at,
            request_id: entry.request_id.clone(),
            environment: entry.environment.clone(),
            name: entry.request.name.clone(),
            method: entry.request.method.to_uppercase(),
            url: entry.request.url.clone(),
            status: entry.response.as_ref().map(|r| r.status),
            duration_ms: entry.response.as_ref().map(|r| r.duration_ms),
            error: entry.error.clone(),
        }
    }
}

impl Entry {
    /// Build an entry for a send that just finished, successfully or not.
    pub fn new(
        request: &Request,
        request_id: Option<String>,
        environment: Option<String>,
        outcome: std::result::Result<&HttpResponse, &Error>,
    ) -> Self {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);

        let (response, error, body_truncated) = match outcome {
            Ok(response) => {
                let mut response = response.clone();
                let truncated = truncate_body(&mut response);
                (Some(response), None, truncated)
            }
            Err(error) => (None, Some(error.to_string()), false),
        };

        Entry {
            id: format!("{at}-{n}"),
            at,
            request_id,
            environment,
            request: request.clone(),
            response,
            body_truncated,
            error,
        }
    }

    /// Replace every secret value with its `{{name}}`, everywhere in the entry.
    ///
    /// A secret can reach the record through the resolved URL, a header the
    /// server echoed, or the body of a login response. History is local, but
    /// the project's promise is that secret values live in the `.env` files only.
    pub fn redact(&mut self, vars: &[EnvVar]) {
        // A cookie the server set is a credential the user never declared, so
        // it is not in `vars` and the pass below would not touch it. The name
        // and the attributes stay: "why am I not getting a session cookie" is
        // the reason to look at this, and hiding the whole line answers it with
        // silence.
        if let Some(response) = &mut self.response {
            hide_cookies(&mut response.headers);
            // Basic auth is base64 on the wire, so the literal pass below can
            // never match the password inside it — and a credential typed
            // straight into the field was never a declared secret at all.
            for header in response.sent_headers.iter_mut() {
                let basic = header.name.eq_ignore_ascii_case("authorization")
                    && header.value.len() > 6
                    && header.value[..6].eq_ignore_ascii_case("basic ");
                if basic {
                    header.value = "Basic …".into();
                }
            }
        }

        let mut secrets: Vec<(&str, String)> = vars
            .iter()
            .filter(|v| v.secret && v.value.chars().count() >= MIN_REDACT_LEN)
            .map(|v| (v.value.as_str(), format!("{{{{{}}}}}", v.name)))
            .collect();
        if secrets.is_empty() {
            return;
        }
        // Longest first, so a secret that contains another is replaced whole.
        secrets.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));

        let Ok(mut value) = serde_json::to_value(&*self) else { return };
        redact_strings(&mut value, &secrets);
        if let Ok(redacted) = serde_json::from_value(value) {
            *self = redacted;
        }
    }
}

/// `session=abc123; Path=/; HttpOnly` → `session=…; Path=/; HttpOnly`.
/// Hide the value of every `Set-Cookie` in a list of response headers.
///
/// Shared with saved examples, which are the files that actually get
/// committed: a session cookie is a credential the user never declared, so the
/// secret pass below cannot reach it, and leaving it to each caller is how the
/// committed artefact ended up the one without the protection.
pub(crate) fn hide_cookies(headers: &mut [KeyValue]) {
    for header in headers.iter_mut() {
        if header.name.eq_ignore_ascii_case("set-cookie") {
            header.value = hide_cookie_value(&header.value);
        }
    }
}

fn hide_cookie_value(header: &str) -> String {
    let (pair, attributes) = header.split_once(';').unwrap_or((header, ""));
    let Some((name, value)) = pair.split_once('=') else { return header.to_string() };
    if value.trim().is_empty() {
        return header.to_string();
    }

    let mut out = format!("{}=…", name.trim());
    if !attributes.is_empty() {
        out.push(';');
        out.push_str(attributes);
    }
    out
}

/// Replace every secret value with its `{{name}}` throughout any serializable
/// value. Shared with saved examples, which are files in the collection and so
/// must not carry a token any more than history may.
pub(crate) fn redacted<T: serde::Serialize + serde::de::DeserializeOwned + Clone>(value: &T, vars: &[EnvVar]) -> T {
    let mut secrets: Vec<(&str, String)> = vars
        .iter()
        .filter(|v| v.secret && v.value.chars().count() >= MIN_REDACT_LEN)
        .map(|v| (v.value.as_str(), format!("{{{{{}}}}}", v.name)))
        .collect();
    if secrets.is_empty() {
        return value.clone();
    }
    // Longest first, so a secret that contains another is replaced whole.
    secrets.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));

    let Ok(mut json) = serde_json::to_value(value) else { return value.clone() };
    redact_strings(&mut json, &secrets);
    serde_json::from_value(json).unwrap_or_else(|_| value.clone())
}


fn redact_strings(value: &mut serde_json::Value, secrets: &[(&str, String)]) {
    match value {
        serde_json::Value::String(text) => {
            for (secret, placeholder) in secrets {
                if text.contains(secret) {
                    *text = text.replace(secret, placeholder);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| redact_strings(v, secrets)),
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                // Never rewrite a serde discriminant. If a secret happened to
                // be the string "json" or "bearer", the round-trip back into
                // `Entry` failed and `redacted` fell back to the *unredacted*
                // value — redaction failing open is the one way it must not
                // fail. These three are the complete set inside an `Entry`
                // (`Body`/`Auth` type, `Request` kind, API key location) and
                // none of them can hold a credential.
                if item.is_string() && matches!(key.as_str(), "type" | "kind" | "location") {
                    continue;
                }
                redact_strings(item, secrets);
            }
        }
        _ => {}
    }
}

/// Returns whether anything was cut. Cuts on a character boundary for text.
fn truncate_body(response: &mut HttpResponse) -> bool {
    if response.body.len() <= MAX_BODY_BYTES {
        return false;
    }
    let mut cut = MAX_BODY_BYTES;
    while !response.body.is_char_boundary(cut) {
        cut -= 1;
    }
    response.body.truncate(cut);
    true
}

/// The history file for one collection.
pub fn file_for(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("{:016x}.jsonl", fnv1a(&normalize_root(root))))
}

/// `C:\api` and `c:/api/` are the same collection.
fn normalize_root(root: &str) -> String {
    let unified = root.replace('\\', "/");
    let trimmed = unified.trim_end_matches('/');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// A stable hash: `DefaultHasher` may change between Rust releases, which would
/// orphan every existing history file.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn record(dir: &Path, root: &str, entry: &Entry) -> Result<()> {
    let _guard = FILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    fs::create_dir_all(dir).map_err(|e| Error::io(dir.to_string_lossy(), e))?;
    let path = file_for(dir, root);

    let mut line = serde_json::to_string(entry).map_err(|e| Error::parse(path.to_string_lossy(), e))?;
    line.push('\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| Error::io(path.to_string_lossy(), e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| Error::io(path.to_string_lossy(), e))?;
    drop(file);

    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or_default();
    if size > MAX_FILE_BYTES {
        compact(&path, MAX_FILE_BYTES / 2)?;
    }
    Ok(())
}

/// Newest first, at most `MAX_ENTRIES`.
pub fn list(dir: &Path, root: &str) -> Result<Vec<Summary>> {
    let _guard = FILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let entries = read_all(&file_for(dir, root))?;
    Ok(entries.iter().rev().take(MAX_ENTRIES).map(Summary::from).collect())
}

pub fn get(dir: &Path, root: &str, id: &str) -> Result<Entry> {
    let _guard = FILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    read_all(&file_for(dir, root))?
        .into_iter()
        .rev()
        .find(|entry| entry.id == id)
        .ok_or_else(|| Error::Invalid("that history entry no longer exists".into()))
}

pub fn clear(dir: &Path, root: &str) -> Result<()> {
    let _guard = FILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = file_for(dir, root);
    match fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path.to_string_lossy(), e)),
        _ => Ok(()),
    }
}

/// Oldest first. A missing file is an empty history; a line that does not parse
/// (a crash mid-write) is skipped rather than losing everything after it.
fn read_all(path: &Path) -> Result<Vec<Entry>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io(path.to_string_lossy(), e)),
    };
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

/// Keep the newest entries that fit in `budget` bytes (and never more than
/// `MAX_ENTRIES`), written to a temp file first so a crash cannot truncate it.
fn compact(path: &Path, budget: u64) -> Result<()> {
    let entries = read_all(path)?;

    let mut kept: Vec<String> = Vec::new();
    let mut used: u64 = 0;
    for entry in entries.iter().rev().take(MAX_ENTRIES) {
        let Ok(line) = serde_json::to_string(entry) else { continue };
        used += line.len() as u64 + 1;
        if used > budget && !kept.is_empty() {
            break;
        }
        kept.push(line);
    }
    kept.reverse();

    let tmp = path.with_extension("jsonl.tmp");
    let mut out = kept.join("\n");
    out.push('\n');
    fs::write(&tmp, out).map_err(|e| Error::io(tmp.to_string_lossy(), e))?;
    fs::rename(&tmp, path).map_err(|e| Error::io(path.to_string_lossy(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyValue;

    fn dir(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("volt-history-{label}-{}", std::process::id()));
        fs::remove_dir_all(&d).ok();
        d
    }

    fn request(name: &str) -> Request {
        Request {
            name: name.into(),
            method: "get".into(),
            url: "{{baseUrl}}/users?key={{apiKey}}".into(),
            ..Default::default()
        }
    }

    fn response(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: 12,
            time_to_first_byte_ms: 5,
            final_url: "https://api.test/users".into(),
            sent_url: "https://api.test/users".into(),
            sent_method: "GET".into(),
            sent_headers: vec![],
            sent_body_bytes: 0,
            redirects: vec![],
            version: "HTTP/1.1".into(),
            missing_vars: vec![],
        }
    }

    fn ok_entry(name: &str, body: &str) -> Entry {
        Entry::new(&request(name), Some(format!("{name}.yaml")), Some("local".into()), Ok(&response(body)))
    }

    #[test]
    fn a_cookie_the_server_set_keeps_its_name_and_loses_its_value() {
        assert_eq!(
            hide_cookie_value("session=abc123def; Path=/; HttpOnly; Secure"),
            "session=…; Path=/; HttpOnly; Secure",
            "the attributes are the part worth reading"
        );
        assert_eq!(hide_cookie_value("t=xyz"), "t=…");
        // Nothing to hide, or nothing that parses: left as it is.
        assert_eq!(hide_cookie_value("cleared=; Max-Age=0"), "cleared=; Max-Age=0");
        assert_eq!(hide_cookie_value("nonsense"), "nonsense");
    }

    #[test]
    fn a_recorded_response_does_not_keep_the_cookie_it_was_given() {
        let mut response = response("{}");
        response.headers = vec![
            KeyValue {
                name: "Set-Cookie".into(),
                value: "session=s3ss10n-value; Path=/; HttpOnly".into(),
                enabled: true,
                description: None,
            },
            KeyValue { name: "Content-Type".into(), value: "application/json".into(), enabled: true, description: None },
        ];

        let mut entry = Entry::new(&request("Login"), None, None, Ok(&response));
        // No declared secrets at all: the cookie is hidden on its own account.
        entry.redact(&[]);

        let recorded = entry.response.as_ref().unwrap();
        let cookie = recorded.headers.iter().find(|h| h.name == "Set-Cookie").unwrap();
        assert_eq!(cookie.value, "session=…; Path=/; HttpOnly");
        assert!(recorded.headers.iter().any(|h| h.name == "Content-Type" && h.value.contains("json")), "the rest is untouched");
    }

    #[test]
    fn entries_come_back_newest_first_with_a_summary() {
        let d = dir("order");
        record(&d, "C:/api", &ok_entry("first", "{}")).unwrap();
        record(&d, "C:/api", &ok_entry("second", "{}")).unwrap();

        let list = list(&d, "C:/api").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "second");
        assert_eq!(list[0].method, "GET", "method is normalised for display");
        assert_eq!(list[0].status, Some(200));
        assert_eq!(list[0].request_id.as_deref(), Some("second.yaml"));

        let full = get(&d, "C:/api", &list[1].id).unwrap();
        assert_eq!(full.request.name, "first");
        assert_eq!(full.response.unwrap().body, "{}");

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_failed_send_is_recorded_with_its_error() {
        let d = dir("failure");
        let error = Error::Http("could not connect: refused".into());
        let entry = Entry::new(&request("down"), None, None, Err(&error));
        record(&d, "C:/api", &entry).unwrap();

        let list = list(&d, "C:/api").unwrap();
        assert_eq!(list[0].status, None);
        assert!(list[0].error.as_deref().unwrap().contains("refused"));

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn collections_do_not_share_history_but_path_spelling_does_not_matter() {
        let d = dir("isolation");
        record(&d, "C:/api", &ok_entry("a", "{}")).unwrap();
        record(&d, "C:/other", &ok_entry("b", "{}")).unwrap();

        assert_eq!(list(&d, "C:/api").unwrap().len(), 1);
        assert_eq!(list(&d, "C:/other").unwrap().len(), 1);
        // Same directory, spelled differently.
        assert_eq!(file_for(&d, "C:/api/"), file_for(&d, "C:/api"));
        assert_eq!(file_for(&d, "C:\\api"), file_for(&d, "C:/api"));

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn secret_values_are_replaced_with_their_placeholder_everywhere() {
        let vars = vec![
            EnvVar { name: "apiKey".into(), value: "sk_live_12345".into(), secret: true },
            EnvVar { name: "baseUrl".into(), value: "https://api.test".into(), secret: false },
            EnvVar { name: "pin".into(), value: "42".into(), secret: true },
        ];

        let mut res = response(r#"{"echo":"sk_live_12345","count":42}"#);
        res.sent_url = "https://api.test/users?key=sk_live_12345".into();
        res.headers = vec![KeyValue {
            name: "x-echo".into(),
            value: "Bearer sk_live_12345".into(),
            enabled: true,
            description: None,
        }];

        let mut entry = Entry::new(&request("leaky"), None, None, Ok(&res));
        entry.redact(&vars);

        let stored = serde_json::to_string(&entry).unwrap();
        assert!(!stored.contains("sk_live_12345"), "secret leaked: {stored}");
        let res = entry.response.unwrap();
        assert_eq!(res.sent_url, "https://api.test/users?key={{apiKey}}");
        assert_eq!(res.headers[0].value, "Bearer {{apiKey}}");
        assert!(res.body.contains(r#""echo":"{{apiKey}}""#));

        // Not secret: left alone. Too short to redact safely: left alone.
        assert!(res.sent_url.starts_with("https://api.test"));
        assert!(res.body.contains(r#""count":42"#));
    }

    #[test]
    fn a_large_body_is_truncated_on_a_character_boundary() {
        // Multi-byte characters straddling the limit must not panic.
        let body = "ğ".repeat(MAX_BODY_BYTES);
        let entry = ok_entry("big", &body);
        assert!(entry.body_truncated);
        let stored = entry.response.unwrap().body;
        assert!(stored.len() <= MAX_BODY_BYTES);
        assert!(stored.chars().all(|c| c == 'ğ'));
    }

    #[test]
    fn the_list_is_capped_and_an_oversized_file_is_compacted() {
        let d = dir("cap");
        for i in 0..(MAX_ENTRIES + 20) {
            record(&d, "C:/api", &ok_entry(&format!("r{i}"), "{}")).unwrap();
        }
        let summaries = list(&d, "C:/api").unwrap();
        assert_eq!(summaries.len(), MAX_ENTRIES);
        assert_eq!(summaries[0].name, format!("r{}", MAX_ENTRIES + 19), "newest kept");

        // Compaction keeps the newest entries and drops the rest.
        let path = file_for(&d, "C:/api");
        compact(&path, 2_000).unwrap();
        let after = read_all(&path).unwrap();
        assert!(!after.is_empty() && after.len() < 20, "kept {}", after.len());
        assert_eq!(after.last().unwrap().request.name, format!("r{}", MAX_ENTRIES + 19));

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_corrupt_line_does_not_lose_the_rest() {
        let d = dir("corrupt");
        record(&d, "C:/api", &ok_entry("before", "{}")).unwrap();
        let path = file_for(&d, "C:/api");
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"id\":\"half-writ").unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);
        record(&d, "C:/api", &ok_entry("after", "{}")).unwrap();

        let names: Vec<String> = list(&d, "C:/api").unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["after", "before"]);

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn clearing_removes_the_history_and_tolerates_nothing_to_clear() {
        let d = dir("clear");
        clear(&d, "C:/api").unwrap();
        record(&d, "C:/api", &ok_entry("x", "{}")).unwrap();
        clear(&d, "C:/api").unwrap();
        assert!(list(&d, "C:/api").unwrap().is_empty());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_hash_is_stable_across_releases() {
        // Pinned: if this changes, every user's history file is orphaned.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    /// Base64 hides the password from the literal pass, so the shape has to be
    /// replaced rather than the value searched for.
    #[test]
    fn basic_auth_does_not_reach_the_history_file() {
        let request = Request {
            name: "Log in".into(),
            method: "GET".into(),
            url: "{{baseUrl}}/me".into(),
            ..Default::default()
        };
        let mut response = response("{}");
        response.sent_headers = vec![
            KeyValue {
                name: "authorization".into(),
                // base64 of `ada:hunter2-live`
                value: "Basic YWRhOmh1bnRlcjItbGl2ZQ==".into(),
                enabled: true,
                description: None,
            },
            KeyValue { name: "accept".into(), value: "*/*".into(), enabled: true, description: None },
        ];

        let ok = Ok(response);
        let mut entry = Entry::new(&request, None, None, ok.as_ref());
        // No declared secrets at all: a credential typed straight into the
        // field was never a variable, and that is the case that used to leak.
        entry.redact(&[]);

        let written = serde_json::to_string(&entry).unwrap();
        assert!(!written.contains("YWRhOmh1bnRlcjItbGl2ZQ"), "the credential is gone:\n{written}");
        assert!(written.contains("Basic …"), "the shape stays, so it is clear auth was sent");
        assert!(written.contains("*/*"), "other headers are untouched");
    }
}

//! Taking a value out of a response and keeping it.
//!
//! This is the chaining problem — log in, then use the token — solved without
//! a scripting engine. A capture is three fields in the request's YAML, so it
//! reads in a pull request like everything else, and there is no sandbox to
//! reason about.
//!
//! What can be read: `status`, `header:Name`, `body`, and a path into a JSON
//! body (`$.data.token`, `$.items[0].id`). Deliberately a subset: a full
//! JSONPath brings filters and wildcards, which turn "what will this capture"
//! into a question you cannot answer by looking.

use crate::http::HttpResponse;
use crate::model::{Capture, EnvVar};

/// The values a response yields, and a note for every capture that found
/// nothing — silence would be how people ship a broken chain.
pub fn extract(captures: &[Capture], response: &HttpResponse) -> (Vec<EnvVar>, Vec<String>) {
    let wanted: Vec<&Capture> =
        captures.iter().filter(|c| c.enabled && !c.name.trim().is_empty()).collect();
    // Parsing is the expensive part, and most requests capture nothing. On a
    // 20 MB response this was two full parses per send for no reason.
    if wanted.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut found = Vec::new();
    let mut notes = Vec::new();
    let body = json_body(response);

    for capture in wanted {
        match read(capture.from.trim(), response, body.as_ref()) {
            Some(value) => found.push(EnvVar {
                name: capture.name.trim().to_string(),
                value,
                secret: capture.secret,
            }),
            None => notes.push(format!("`{}` found nothing at `{}`", capture.name.trim(), capture.from.trim())),
        }
    }

    (found, notes)
}

/// Read one thing out of a response: `status`, `time`, `header:Name`, `body`,
/// or a path into a JSON body. Shared with checks, so a capture and an
/// assertion can never disagree about where a value comes from.
pub(crate) fn read(from: &str, response: &HttpResponse, body: Option<&serde_json::Value>) -> Option<String> {
    if from.eq_ignore_ascii_case("status") {
        return Some(response.status.to_string());
    }
    if from.eq_ignore_ascii_case("time") {
        return Some(response.duration_ms.to_string());
    }
    if from.eq_ignore_ascii_case("body") {
        return (!response.body_is_base64).then(|| response.body.clone());
    }
    if let Some(name) = from.strip_prefix("header:").map(str::trim) {
        return response
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.clone());
    }
    json_path(body?, from).map(render)
}

/// `$.a.b[0].c`, `a.b`, `$[2]`. Nothing else.
pub(crate) fn json_path<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut at = root;
    let path = path.strip_prefix('$').unwrap_or(path);

    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        // `items[0][1]` — a name, then any number of indices.
        let (name, rest) = match segment.find('[') {
            Some(bracket) => segment.split_at(bracket),
            None => (segment, ""),
        };
        if !name.is_empty() {
            at = at.get(name)?;
        }
        for index in rest.split(']').filter(|s| !s.is_empty()) {
            let index: usize = index.trim_start_matches('[').trim().parse().ok()?;
            at = at.get(index)?;
        }
    }
    Some(at)
}

/// The JSON body, parsed once, or `None` when there is not one.
pub(crate) fn json_body(response: &HttpResponse) -> Option<serde_json::Value> {
    (!response.body_is_base64).then(|| serde_json::from_str(&response.body).ok()).flatten()
}

/// A string captures as its contents, not as `"quoted"`; anything else keeps
/// the JSON it had, so an object or a list can be passed along whole.
fn render(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyValue;

    fn response(body: &str) -> HttpResponse {
        HttpResponse {
            status: 201,
            status_text: "Created".into(),
            headers: vec![KeyValue {
                name: "Location".into(),
                value: "/orders/42".into(),
                enabled: true,
                description: None,
            }],
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: 1,
            time_to_first_byte_ms: 1,
            final_url: "https://x.test/orders".into(),
            sent_url: "https://x.test/orders".into(),
            sent_method: "GET".into(),
            sent_headers: Vec::new(),
            sent_body_bytes: 0,
            redirects: Vec::new(),
            version: "HTTP/1.1".into(),
            missing_vars: Vec::new(),
        }
    }

    fn capture(name: &str, from: &str) -> Capture {
        Capture { name: name.into(), from: from.into(), enabled: true, secret: false }
    }

    #[test]
    fn a_token_travels_from_one_response_to_the_next_request() {
        let body = r#"{"data":{"token":"eyJ.abc","expires":3600},"items":[{"id":"a-1"},{"id":"a-2"}]}"#;
        let mut secret = capture("token", "$.data.token");
        secret.secret = true;

        let (found, notes) = extract(
            &[
                secret,
                capture("expires", "$.data.expires"),
                capture("second", "$.items[1].id"),
                capture("code", "status"),
                capture("created", "header:location"),
            ],
            &response(body),
        );

        let value = |name: &str| found.iter().find(|v| v.name == name).map(|v| v.value.as_str());
        assert_eq!(value("token"), Some("eyJ.abc"), "a string captures without its quotes");
        assert_eq!(value("expires"), Some("3600"), "a number keeps its JSON");
        assert_eq!(value("second"), Some("a-2"), "indices work");
        assert_eq!(value("code"), Some("201"));
        assert_eq!(value("created"), Some("/orders/42"), "headers match whatever the case");
        assert!(found.iter().find(|v| v.name == "token").unwrap().secret, "and a secret stays a secret");
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_capture_that_finds_nothing_says_so_instead_of_writing_an_empty_variable() {
        let (found, notes) = extract(
            &[capture("token", "$.data.token"), capture("gone", "header:X-Nope")],
            &response(r#"{"data":{}}"#),
        );

        assert!(found.is_empty(), "{found:?}");
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("token") && notes[0].contains("$.data.token"), "{notes:?}");
    }

    #[test]
    fn a_body_that_is_not_json_does_not_take_the_send_down_with_it() {
        let (found, notes) = extract(&[capture("t", "$.token"), capture("all", "body")], &response("<html>"));
        assert_eq!(found.len(), 1, "the whole body is still readable");
        assert_eq!(found[0].value, "<html>");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn a_disabled_or_unnamed_capture_is_left_alone() {
        let mut off = capture("token", "$.token");
        off.enabled = false;
        let (found, notes) = extract(&[off, capture("  ", "$.token")], &response(r#"{"token":"x"}"#));
        assert!(found.is_empty() && notes.is_empty());
    }
}

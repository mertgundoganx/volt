//! Assertions on a response.
//!
//! Declarative, like captures, and for the same reason: a check written in
//! YAML reads in a pull request, and "what does this test actually assert" is
//! a question you can answer by looking. A scripting engine would buy
//! expressiveness and cost exactly that.
//!
//! One shape covers everything: where to look, what to compare, and the value.
//! Where to look is the same vocabulary a capture uses — `status`, `time`,
//! `header:Name`, `body`, or a path into a JSON body.

use serde::{Deserialize, Serialize};

use crate::capture;
use crate::http::HttpResponse;
use crate::model::Check;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    /// The check as written, for the UI to show without rebuilding it.
    pub from: String,
    pub op: String,
    pub expected: String,
    /// What was actually there. `None` when nothing was.
    pub actual: Option<String>,
    /// Why it failed, in words, when "expected x, got y" is not enough.
    pub note: Option<String>,
}

pub fn run(checks: &[Check], response: &HttpResponse) -> Vec<Outcome> {
    let wanted: Vec<&Check> =
        checks.iter().filter(|check| check.enabled && !check.from.trim().is_empty()).collect();
    // Same reason as `capture::extract`: do not parse a body nobody asked about.
    if wanted.is_empty() {
        return Vec::new();
    }

    let body = capture::json_body(response);
    wanted.into_iter().map(|check| one(check, response, body.as_ref())).collect()
}

fn one(check: &Check, response: &HttpResponse, body: Option<&serde_json::Value>) -> Outcome {
    let from = check.from.trim();
    let actual = capture::read(from, response, body);
    let expected = check.value.trim().to_string();

    let mut note = None;
    let ok = match check.op {
        Op::Is => actual.as_deref() == Some(expected.as_str()),
        Op::IsNot => actual.as_deref() != Some(expected.as_str()),
        Op::Contains => actual.as_deref().is_some_and(|value| value.contains(&expected)),
        Op::Exists => actual.is_some(),
        Op::Missing => actual.is_none(),
        Op::Under | Op::Over => match (number(actual.as_deref()), number(Some(&expected))) {
            (Some(value), Some(limit)) => {
                if matches!(check.op, Op::Under) {
                    value < limit
                } else {
                    value > limit
                }
            }
            (None, _) => {
                note = Some(format!("`{}` is not a number", actual.as_deref().unwrap_or("nothing")));
                false
            }
            (_, None) => {
                note = Some(format!("`{expected}` is not a number to compare against"));
                false
            }
        },
    };

    Outcome { ok, from: from.to_string(), op: check.op.label().to_string(), expected, actual, note }
}

fn number(text: Option<&str>) -> Option<f64> {
    text?.trim().parse().ok()
}

/// What a check compares with. `Is` by default, because that is what most
/// checks are and a missing `op` should read as the obvious one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    #[default]
    Is,
    IsNot,
    Contains,
    Exists,
    Missing,
    Under,
    Over,
}

impl Op {
    pub fn label(self) -> &'static str {
        match self {
            Op::Is => "is",
            Op::IsNot => "is not",
            Op::Contains => "contains",
            Op::Exists => "exists",
            Op::Missing => "is missing",
            Op::Under => "under",
            Op::Over => "over",
        }
    }

    /// Whether the operator takes a value at all, for the UI.
    pub fn takes_value(self) -> bool {
        !matches!(self, Op::Exists | Op::Missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyValue;

    fn response(status: u16, body: &str, ms: u64) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![KeyValue {
                name: "Content-Type".into(),
                value: "application/json; charset=utf-8".into(),
                enabled: true,
                description: None,
            }],
            body: body.into(),
            body_is_base64: false,
            size_bytes: body.len(),
            duration_ms: ms,
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

    fn check(from: &str, op: Op, value: &str) -> Check {
        Check { from: from.into(), op, value: value.into(), enabled: true }
    }

    #[test]
    fn the_checks_people_actually_write() {
        let response = response(200, r#"{"data":{"id":"a-1","count":3},"error":null}"#, 120);
        let results = run(
            &[
                check("status", Op::Is, "200"),
                check("header:content-type", Op::Contains, "application/json"),
                check("$.data.id", Op::Exists, ""),
                check("$.data.count", Op::Over, "1"),
                check("time", Op::Under, "500"),
                check("$.oops", Op::Missing, ""),
            ],
            &response,
        );

        assert!(results.iter().all(|r| r.ok), "{results:#?}");
        assert_eq!(results[0].actual.as_deref(), Some("200"));
        assert_eq!(results[1].op, "contains");
    }

    #[test]
    fn a_failure_says_what_was_there_instead() {
        let response = response(500, r#"{"data":{"id":"a-1"}}"#, 900);
        let results = run(
            &[check("status", Op::Is, "200"), check("time", Op::Under, "500"), check("$.nope", Op::Exists, "")],
            &response,
        );

        assert!(results.iter().all(|r| !r.ok));
        assert_eq!(results[0].actual.as_deref(), Some("500"), "the real value comes back for the message");
        assert_eq!(results[1].actual.as_deref(), Some("900"));
        assert_eq!(results[2].actual, None, "nothing found is nothing, not an empty string");
    }

    #[test]
    fn comparing_something_that_is_not_a_number_says_so_rather_than_failing_quietly() {
        let response = response(200, r#"{"name":"Ada"}"#, 10);
        let results = run(&[check("$.name", Op::Under, "5")], &response);

        assert!(!results[0].ok);
        assert!(results[0].note.as_deref().is_some_and(|note| note.contains("not a number")), "{:?}", results[0].note);
    }

    #[test]
    fn a_disabled_check_is_not_run_at_all() {
        let response = response(500, "{}", 10);
        let mut off = check("status", Op::Is, "200");
        off.enabled = false;
        assert!(run(&[off, check("  ", Op::Exists, "")], &response).is_empty());
    }

    #[test]
    fn the_default_operator_is_the_obvious_one() {
        let parsed: Check = serde_yaml_ng::from_str("from: status\nvalue: '200'\n").unwrap();
        assert_eq!(parsed.op, Op::Is);
        assert!(parsed.enabled, "and a check is on unless it says otherwise");
    }
}

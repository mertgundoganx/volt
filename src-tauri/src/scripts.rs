//! Reading the common shapes out of an imported Postman or Insomnia script.
//!
//! Most of what people write in a test script is not really a script. It is
//! two things volt already has:
//!
//! ```js
//! pm.environment.set("token", pm.response.json().access_token);  // a capture
//! pm.test("ok", () => pm.response.to.have.status(200));          // a check
//! ```
//!
//! So those are read, and everything else is handed back verbatim for the
//! import report to list. **The line is deliberate**: a fixed set of one-line
//! statements is understood, and no attempt is made at control flow, at
//! variables other than the one holding the parsed body, or at anything that
//! needs a value to be computed. Translating *most* scripts would mean writing
//! a JavaScript interpreter, and a half-working one is worse than an honest
//! list of what was left behind — a check that quietly does not mean what the
//! script meant is a test that lies.
//!
//! Only after-response scripts are read. A pre-request script runs before the
//! send and volt has nothing that does, so it is reported, never guessed at.

use crate::checks::Op;
use crate::model::{Capture, Check};

#[derive(Debug, Default)]
pub struct Translated {
    pub captures: Vec<Capture>,
    pub checks: Vec<Check>,
    /// Statements that were not one of the shapes above, as they were written.
    pub left: Vec<String>,
}

impl Translated {
    pub fn found_something(&self) -> bool {
        !self.captures.is_empty() || !self.checks.is_empty()
    }
}

/// Read an after-response script.
pub fn translate(code: &str) -> Translated {
    let mut out = Translated::default();
    // Names bound to the parsed body — `var jsonData = pm.response.json()` is
    // the first line of half the test scripts ever written.
    let mut bodies: Vec<String> = Vec::new();

    for statement in statements(code) {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        if let Some(name) = binds_the_body(statement) {
            bodies.push(name);
            continue;
        }
        if let Some(capture) = capture_in(statement, &bodies) {
            out.captures.push(capture);
            continue;
        }
        if let Some(check) = check_in(statement, &bodies) {
            out.checks.push(check);
            continue;
        }
        if is_scaffolding(statement) {
            continue;
        }
        let shown = statement.replace(['\n', '\t'], " ");
        if !out.left.contains(&shown) {
            out.left.push(shown);
        }
    }
    out
}

/// Split on `;` and newlines, but never inside a string. Comments go first, so
/// a `;` inside one cannot split anything either.
fn statements(code: &str) -> Vec<String> {
    let code = without_comments(code);
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in code.chars() {
        if let Some(open) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\u{5c}' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            ';' | '\n' => out.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    out.push(current);
    out
}

fn without_comments(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut chars = code.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if let Some(open) = quote {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\u{5c}' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                quote = Some(ch);
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for ch in chars.by_ref() {
                    if last == '*' && ch == '/' {
                        break;
                    }
                    last = ch;
                }
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }
    out
}

/// `var jsonData = pm.response.json()` → `jsonData`.
fn binds_the_body(statement: &str) -> Option<String> {
    let rest =
        ["var ", "let ", "const "].iter().find_map(|kw| statement.trim_start().strip_prefix(*kw))?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return None;
    }
    let value = value.trim();
    let reads_body = value.starts_with("pm.response.json()")
        || value.starts_with("JSON.parse(responseBody)")
        || value.starts_with("JSON.parse(pm.response.text())");
    reads_body.then(|| name.to_string())
}

/// The scaffolding a test script is wrapped in, which carries no meaning of its
/// own: the `pm.test(…)` wrapper, arrow and function heads, closing brackets.
fn is_scaffolding(statement: &str) -> bool {
    let bare: String = statement.chars().filter(|c| !c.is_whitespace()).collect();
    if !bare.is_empty() && bare.chars().all(|c| "(){}[],=>".contains(c)) {
        return true;
    }
    if bare == "function()" || bare == "asyncfunction()" {
        return true;
    }
    // `pm.test("name", function () {` with nothing asserted on the same line.
    statement.trim_start().starts_with("pm.test(")
        && !statement.contains("pm.expect")
        && !statement.contains(".to.")
}

// ---------------------------------------------------------------------------
// Captures
// ---------------------------------------------------------------------------

/// `pm.environment.set("token", pm.response.json().access_token)`.
fn capture_in(statement: &str, bodies: &[String]) -> Option<Capture> {
    const SETTERS: [&str; 7] = [
        "pm.environment.set(",
        "pm.collectionVariables.set(",
        "pm.globals.set(",
        "pm.variables.set(",
        "postman.setEnvironmentVariable(",
        "postman.setGlobalVariable(",
        "insomnia.environment.set(",
    ];
    let at = SETTERS.iter().find_map(|setter| statement.find(setter).map(|at| at + setter.len()))?;
    let (name, rest) = quoted(&statement[at..])?;
    if name.trim().is_empty() {
        return None;
    }
    let value = rest.trim_start().strip_prefix(',')?.trim();
    let value = value.strip_suffix(')').unwrap_or(value).trim();
    let from = source_of(value, bodies)?;

    Some(Capture {
        name: name.to_string(),
        from,
        enabled: true,
        // What a script captures is a token until proved otherwise, and the
        // safe mistake is the one that keeps a value out of the committed YAML.
        secret: true,
    })
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

fn check_in(statement: &str, bodies: &[String]) -> Option<Check> {
    if let Some(at) = statement.find("to.have.status(") {
        let inside = until_close(&statement[at + "to.have.status(".len()..])?;
        // `to.have.status("OK")` names the status text, which volt does not keep.
        return unquote(inside.trim()).parse::<u16>().ok().map(|status| Check {
            from: "status".into(),
            op: Op::Is,
            value: status.to_string(),
            enabled: true,
        });
    }
    if let Some(at) = statement.find("to.have.header(") {
        let inside = until_close(&statement[at + "to.have.header(".len()..])?;
        let (name, _) = quoted(inside)?;
        return Some(Check {
            from: format!("header:{name}"),
            op: Op::Exists,
            value: String::new(),
            enabled: true,
        });
    }

    let at = statement.find("pm.expect(").map(|at| at + "pm.expect(".len())?;
    let subject = until_close(&statement[at..])?;
    let from = source_of(subject.trim(), bodies)?;
    let chain = &statement[at + subject.len()..];
    let negated = chain.contains(".not.");

    let (op, value) = if let Some(value) = call_after(chain, &["eql(", "equal(", "equals(", "eq("]) {
        (if negated { Op::IsNot } else { Op::Is }, value)
    } else if let Some(value) = call_after(chain, &["include(", "includes(", "contain(", "contains("])
    {
        (Op::Contains, value)
    } else if let Some(value) = call_after(chain, &["below(", "lessThan(", "lt("]) {
        (Op::Under, value)
    } else if let Some(value) = call_after(chain, &["above(", "greaterThan(", "gt("]) {
        (Op::Over, value)
    } else if chain.contains("to.exist") || chain.contains("to.not.be.undefined") {
        (Op::Exists, String::new())
    } else if chain.contains("to.be.undefined") || chain.contains("to.not.exist") {
        (Op::Missing, String::new())
    } else {
        return None;
    };

    // `not contains`, `not below` and `not above` have no operator of their
    // own, and a check that quietly means the opposite is worse than one that
    // was left for the report.
    if negated && matches!(op, Op::Contains | Op::Under | Op::Over) {
        return None;
    }
    Some(Check { from, op, value, enabled: true })
}

/// The argument of the first of `names` the chain calls.
fn call_after(chain: &str, names: &[&str]) -> Option<String> {
    let at = names.iter().find_map(|name| chain.find(name).map(|at| at + name.len()))?;
    let inside = until_close(&chain[at..])?;
    Some(unquote(inside.trim()).to_string())
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

/// A JavaScript expression as one of volt's sources: `status`, `time`,
/// `header:Name`, `body`, or a path into the body.
fn source_of(expr: &str, bodies: &[String]) -> Option<String> {
    let expr = expr.trim();
    match expr {
        "pm.response.code" | "responseCode.code" | "pm.response.status" => {
            return Some("status".into())
        }
        "pm.response.responseTime" | "pm.response.time" => return Some("time".into()),
        "pm.response.text()" | "responseBody" | "pm.response.body" | "pm.response.json()" => {
            return Some("body".into())
        }
        _ => {}
    }
    if let Some(rest) = expr.strip_prefix("pm.response.headers.get(") {
        let (name, _) = quoted(rest)?;
        return Some(format!("header:{name}"));
    }
    if let Some(rest) = expr.strip_prefix("pm.response.json()") {
        return path_of(rest);
    }
    for name in bodies {
        let Some(rest) = expr.strip_prefix(name.as_str()) else { continue };
        if rest.is_empty() {
            return Some("body".into());
        }
        if rest.starts_with('.') || rest.starts_with('[') {
            return path_of(rest);
        }
    }
    None
}

/// `.data.token` → `$.data.token`, and `["data"]["token"]` with it. A computed
/// key or a list index is refused: volt's JSONPath subset has neither, and a
/// path that quietly means something else is worse than none at all.
fn path_of(rest: &str) -> Option<String> {
    let mut path = String::from("$");
    let mut left = rest.trim();

    while !left.is_empty() {
        let (key, tail) = if let Some(tail) = left.strip_prefix('.') {
            let end = tail.find(['.', '[']).unwrap_or(tail.len());
            let (key, tail) = tail.split_at(end);
            (key.to_string(), tail)
        } else {
            // `[0]` is an index and `[key]` is computed; neither is a path.
            let tail = left.strip_prefix('[')?;
            let (inside, tail) = tail.split_once(']')?;
            let (key, _) = quoted(inside.trim())?;
            (key.to_string(), tail)
        };

        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            return None;
        }
        path.push('.');
        path.push_str(&key);
        left = tail;
    }
    Some(path)
}

/// The first quoted string and what follows it.
fn quoted(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let open = text.chars().next().filter(|c| *c == '"' || *c == '\'' || *c == '`')?;
    let rest = &text[open.len_utf8()..];
    let end = rest.find(open)?;
    Some((&rest[..end], &rest[end + open.len_utf8()..]))
}

fn unquote(text: &str) -> &str {
    quoted(text).map(|(inside, _)| inside).unwrap_or(text)
}

/// What is inside a call, given the text just after its opening bracket.
fn until_close(text: &str) -> Option<&str> {
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (at, ch) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\u{5c}' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[..at]);
                }
            }
            _ => {}
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_shapes_volt_already_has_are_read() {
        let out = translate(
            r#"
            // The token for everything after this.
            var jsonData = pm.response.json();
            pm.environment.set("token", jsonData.data.access_token);
            pm.collectionVariables.set("etag", pm.response.headers.get("ETag"));
            pm.test("status is 201", function () {
                pm.response.to.have.status(201);
            });
            pm.test("fast enough", () => {
                pm.expect(pm.response.responseTime).to.be.below(800);
            });
            "#,
        );

        assert!(out.left.is_empty(), "nothing should have been left: {:?}", out.left);
        assert_eq!(out.captures.len(), 2);
        assert_eq!((out.captures[0].name.as_str(), out.captures[0].from.as_str()), ("token", "$.data.access_token"));
        assert_eq!((out.captures[1].name.as_str(), out.captures[1].from.as_str()), ("etag", "header:ETag"));
        assert!(out.captures.iter().all(|c| c.secret));

        assert_eq!(out.checks.len(), 2);
        assert_eq!((out.checks[0].from.as_str(), out.checks[0].op, out.checks[0].value.as_str()), ("status", Op::Is, "201"));
        assert_eq!((out.checks[1].from.as_str(), out.checks[1].op, out.checks[1].value.as_str()), ("time", Op::Under, "800"));
    }

    #[test]
    fn every_source_a_check_can_read() {
        let out = translate(
            r#"
            const body = pm.response.json();
            pm.expect(pm.response.code).to.eql(200);
            pm.expect(pm.response.text()).to.include("welcome");
            pm.expect(body.user.name).to.not.eql("anonymous");
            pm.expect(body["meta"]["page"]).to.be.above(0);
            pm.expect(body.token).to.exist;
            pm.response.to.have.header("Content-Type");
            "#,
        );

        assert!(out.left.is_empty(), "{:?}", out.left);
        let seen: Vec<(String, Op, String)> =
            out.checks.iter().map(|c| (c.from.clone(), c.op, c.value.clone())).collect();
        assert_eq!(
            seen,
            vec![
                ("status".into(), Op::Is, "200".into()),
                ("body".into(), Op::Contains, "welcome".into()),
                ("$.user.name".into(), Op::IsNot, "anonymous".into()),
                ("$.meta.page".into(), Op::Over, "0".into()),
                ("$.token".into(), Op::Exists, String::new()),
                ("header:Content-Type".into(), Op::Exists, String::new()),
            ]
        );
    }

    #[test]
    fn what_volt_cannot_mean_is_left_for_the_report() {
        let out = translate(
            r#"
            const now = Date.now();
            if (pm.response.code === 200) { pm.environment.set("ok", "yes"); }
            pm.expect(body.rows[0].id).to.eql(1);
            pm.expect(pm.response.text()).to.not.include("error");
            pm.environment.set("signature", CryptoJS.HmacSHA256(now, key).toString());
            "#,
        );

        assert!(out.captures.is_empty(), "a computed value is not a capture: {:?}", out.captures);
        assert!(out.checks.is_empty(), "{:?}", out.checks);
        assert_eq!(out.left.len(), 5, "each one is reported as written: {:?}", out.left);
        assert!(out.left.iter().any(|line| line.contains("CryptoJS")));
        assert!(
            out.left.iter().any(|line| line.contains("rows[0]")),
            "a list index is not a path volt has: {:?}",
            out.left
        );
        assert!(
            out.left.iter().any(|line| line.contains("to.not.include")),
            "`not contains` has no operator, so it is not guessed at: {:?}",
            out.left
        );
    }

    #[test]
    fn a_semicolon_inside_a_string_does_not_split_a_statement() {
        let out = translate(r#"pm.environment.set("cookie", pm.response.headers.get("Set-Cookie"));"#);
        assert_eq!(out.captures.len(), 1);
        assert_eq!(out.captures[0].from, "header:Set-Cookie");

        // A comment mentioning a setter is a comment, not a capture.
        let out = translate("// pm.environment.set(\"x\", pm.response.code)\npm.expect(pm.response.code).to.eql(200);");
        assert!(out.captures.is_empty());
        assert_eq!(out.checks.len(), 1);
        assert!(out.left.is_empty());
    }

    #[test]
    fn an_empty_test_is_nothing_at_all() {
        let out = translate("pm.test('ok', () => {});");
        assert!(!out.found_something());
        assert!(out.left.is_empty(), "scaffolding is not a finding: {:?}", out.left);
    }
}

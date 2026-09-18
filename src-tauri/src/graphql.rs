//! What a GraphQL endpoint said it can do, and what a query asks it for.
//!
//! `lib.rs` runs one introspection query; this reads the answer into something
//! small enough to hand the UI, and then walks a query against it so the editor
//! can complete a field name and mark one the server does not have.
//!
//! The walk is not a GraphQL parser. It follows selection sets, aliases and
//! inline fragments, and skips arguments, directives and variable definitions
//! whole — enough to know which type is in scope at a point in the text, which
//! is the only question being asked. A named fragment spread is not followed:
//! its definition is checked on its own, where its `on Type` says what it is
//! about. Anything it cannot follow it stays quiet about, because a red mark
//! under valid GraphQL is worse than no mark at all.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    /// The type a bare query starts at, usually `Query`.
    pub query_type: String,
    pub mutation_type: String,
    pub subscription_type: String,
    pub types: Vec<GraphType>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphType {
    pub name: String,
    pub description: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// As it reads in the schema: `[User!]!`.
    pub type_name: String,
    /// The named type underneath the list and non-null wrappers, for the walk.
    pub of: String,
    pub description: String,
    pub args: Vec<String>,
}

/// A field a query asks for that the schema does not have.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Where it is in the query text, in UTF-16 units — the way the editor
    /// counts, so a marked field lines up when the query holds a non-ASCII
    /// character.
    pub at: usize,
    pub len: usize,
    pub message: String,
}

impl Schema {
    fn type_named(&self, name: &str) -> Option<&GraphType> {
        self.types.iter().find(|kind| kind.name == name)
    }

    fn field(&self, type_name: &str, field: &str) -> Option<&Field> {
        self.type_named(type_name)?.fields.iter().find(|f| f.name == field)
    }
}

/// Read the `__schema` an introspection query answered with.
pub fn read(value: &Value) -> Schema {
    let root = |key: &str| {
        value.get(key).and_then(|v| v.get("name")).and_then(Value::as_str).unwrap_or("").to_string()
    };

    let types = value
        .get("types")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        // Introspection's own types start with `__` and are noise in a browser.
        .filter(|kind| !text(kind, "name").starts_with("__"))
        .filter(|kind| kind.get("fields").is_some_and(|fields| fields.is_array()))
        .map(|kind| GraphType {
            name: text(kind, "name"),
            description: text(kind, "description"),
            fields: kind
                .get("fields")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|field| {
                    let of = field.get("type");
                    Field {
                        name: text(field, "name"),
                        type_name: render_type(of),
                        of: named_type(of),
                        description: text(field, "description"),
                        args: field
                            .get("args")
                            .and_then(Value::as_array)
                            .map(Vec::as_slice)
                            .unwrap_or_default()
                            .iter()
                            .map(|arg| text(arg, "name"))
                            .collect(),
                    }
                })
                .collect(),
        })
        .collect();

    Schema {
        query_type: root("queryType"),
        mutation_type: root("mutationType"),
        subscription_type: root("subscriptionType"),
        types,
    }
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// `{kind: NON_NULL, ofType: {kind: LIST, ofType: {name: User}}}` → `[User]!`.
fn render_type(value: Option<&Value>) -> String {
    let Some(value) = value else { return String::new() };
    match value.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") => format!("{}!", render_type(value.get("ofType"))),
        Some("LIST") => format!("[{}]", render_type(value.get("ofType"))),
        _ => text(value, "name"),
    }
}

/// The same type with the wrappers taken off, which is what a selection set
/// under the field is about.
fn named_type(value: Option<&Value>) -> String {
    let Some(value) = value else { return String::new() };
    match value.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") | Some("LIST") => named_type(value.get("ofType")),
        _ => text(value, "name"),
    }
}

// ---------------------------------------------------------------------------
// Walking a query
// ---------------------------------------------------------------------------

/// Every field the query asks for that the schema does not have.
///
/// Offsets come back in UTF-16 units, which is how a textarea counts them; the
/// walk itself works in bytes.
pub fn check(schema: &Schema, query: &str) -> Vec<Problem> {
    let mut found = walk(schema, query, None).problems;
    for problem in &mut found {
        let end = byte_to_utf16(query, problem.at + problem.len);
        problem.at = byte_to_utf16(query, problem.at);
        problem.len = end - problem.at;
    }
    found
}

/// A byte offset as the editor counts it.
fn byte_to_utf16(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    // Round down to a boundary rather than slicing through a character.
    let at = (0..=at).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
    text[..at].chars().map(char::len_utf16).sum()
}

/// A UTF-16 caret offset as a byte offset, always on a character boundary.
///
/// The webview counts in UTF-16 code units. Treating that number as a byte
/// index panics the moment the query holds a non-ASCII character — `{ ıııı }`
/// was enough — and a panic in a command unwinds the task, so the promise never
/// settles and the editor freezes with no error.
fn utf16_to_byte(text: &str, at: usize) -> usize {
    let mut units = 0usize;
    for (index, ch) in text.char_indices() {
        if units >= at {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}
/// A field that could come next where the cursor is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub name: String,
    pub type_name: String,
    pub description: String,
    pub args: Vec<String>,
    /// The bytes already typed, which the editor replaces.
    pub prefix: String,
}

/// The fields that belong where the cursor is, narrowed by what is typed there.
pub fn complete(schema: &Schema, query: &str, at: usize) -> Vec<Completion> {
    // The caret arrives from the webview in UTF-16 code units; the walk is bytes.
    let at = utf16_to_byte(query, at);
    let prefix = word_before(query, at);
    // Walk the text up to the start of the word, so a half-typed name is not
    // itself read as a field that does not exist.
    let walked = walk(schema, query, Some(at - prefix.len()));
    let Some(scope) = walked.scope else { return Vec::new() };
    let Some(kind) = schema.type_named(&scope) else { return Vec::new() };

    kind.fields
        .iter()
        .filter(|field| field.name.starts_with(&prefix))
        .map(|field| Completion {
            name: field.name.clone(),
            type_name: field.type_name.clone(),
            description: field.description.clone(),
            args: field.args.clone(),
            prefix: prefix.clone(),
        })
        .collect()
}

fn word_before(text: &str, at: usize) -> String {
    let head = &text[..at];
    let start = head
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    head[start..].to_string()
}

#[derive(Default)]
struct Walked {
    problems: Vec<Problem>,
    /// The type in scope at the offset asked about, if one was asked for.
    scope: Option<String>,
}

/// One pass over the query text. `stop_at` asks which type is in scope there.
fn walk(schema: &Schema, query: &str, stop_at: Option<usize>) -> Walked {
    let mut out = Walked::default();
    let bytes = query.as_bytes();
    // The named fragments, so `fragment F on User { … }` is checked as a User.
    let fragments = fragment_types(query);

    // The type each open selection set is about. `None` means "somewhere the
    // walk lost track", and nothing inside it is marked.
    let mut stack: Vec<Option<String>> = Vec::new();
    // The type the next `{` opens, set by the field just read.
    let mut pending: Option<String> = None;
    let mut at = 0usize;
    // Before the first `{` of a definition, what the next set is about.
    let mut root: Option<String> = Some(schema.query_type.clone());
    let mut reached = false;

    while at < bytes.len() {
        if let Some(stop) = stop_at {
            if at >= stop && !reached {
                reached = true;
                out.scope = stack.last().cloned().flatten();
            }
        }
        let ch = bytes[at];
        match ch {
            b'#' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'"' => at = skip_string(bytes, at),
            b'(' | b'[' => at = skip_brackets(bytes, at),
            b'@' => {
                // A directive, with or without arguments.
                at += 1;
                while at < bytes.len() && is_name(bytes[at]) {
                    at += 1;
                }
            }
            b'{' => {
                stack.push(pending.take().or_else(|| root.take()).filter(|name| !name.is_empty()));
                at += 1;
            }
            b'}' => {
                stack.pop();
                if stack.is_empty() {
                    root = Some(schema.query_type.clone());
                }
                at += 1;
            }
            b'.' if bytes[at..].starts_with(b"...") => {
                at = spread(schema, query, at, &stack, &mut pending, &mut out);
            }
            _ if is_name(ch) => {
                let start = at;
                while at < bytes.len() && is_name(bytes[at]) {
                    at += 1;
                }
                let word = &query[start..at];
                let after = next_visible(bytes, at);

                // `alias: field` — the name before the colon names nothing.
                if after == Some(b':') {
                    at = skip_ws(bytes, at) + 1;
                    continue;
                }
                match word {
                    // An operation header says which root the next set is about.
                    "query" if stack.is_empty() => root = Some(schema.query_type.clone()),
                    "mutation" if stack.is_empty() => root = Some(schema.mutation_type.clone()),
                    "subscription" if stack.is_empty() => {
                        root = Some(schema.subscription_type.clone())
                    }
                    "fragment" if stack.is_empty() => {
                        root = fragment_after(query, at, &fragments);
                    }
                    _ if stack.is_empty() => {}
                    _ => {
                        let Some(Some(scope)) = stack.last() else {
                            pending = None;
                            continue;
                        };
                        match schema.field(scope, word) {
                            Some(field) => pending = Some(field.of.clone()),
                            None => {
                                pending = None;
                                // `__typename` is on every type and in no schema.
                                if !word.starts_with("__") {
                                    out.problems.push(Problem {
                                        at: start,
                                        len: word.len(),
                                        message: format!("{scope} has no field {word}"),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => at += 1,
        }
    }

    if stop_at.is_some() && !reached {
        out.scope = stack.last().cloned().flatten();
    }
    out
}

/// `... on User {` pushes User; `...Name` is left alone, because the fragment
/// is checked where it is defined.
fn spread(
    schema: &Schema,
    query: &str,
    at: usize,
    stack: &[Option<String>],
    pending: &mut Option<String>,
    out: &mut Walked,
) -> usize {
    let bytes = query.as_bytes();
    let mut next = skip_ws(bytes, at + 3);
    let start = next;
    while next < bytes.len() && is_name(bytes[next]) {
        next += 1;
    }
    if &query[start..next] != "on" {
        // A named spread, or `... {` with a directive. Either way, nothing here.
        *pending = stack.last().cloned().flatten();
        return next;
    }

    let after = skip_ws(bytes, next);
    let mut end = after;
    while end < bytes.len() && is_name(bytes[end]) {
        end += 1;
    }
    let name = &query[after..end];
    if schema.type_named(name).is_none() && !name.is_empty() {
        out.problems.push(Problem {
            at: after,
            len: name.len(),
            message: format!("the schema has no type {name}"),
        });
        *pending = None;
    } else {
        *pending = Some(name.to_string());
    }
    end
}

/// `fragment F on User` → the type after `on`.
fn fragment_after(query: &str, at: usize, fragments: &BTreeMap<String, String>) -> Option<String> {
    let bytes = query.as_bytes();
    let start = skip_ws(bytes, at);
    let mut end = start;
    while end < bytes.len() && is_name(bytes[end]) {
        end += 1;
    }
    fragments.get(&query[start..end]).cloned()
}

fn fragment_types(query: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = query.as_bytes();
    let mut at = 0;
    while let Some(found) = query[at..].find("fragment ") {
        let mut next = skip_ws(bytes, at + found + "fragment".len());
        let name_start = next;
        while next < bytes.len() && is_name(bytes[next]) {
            next += 1;
        }
        let name = &query[name_start..next];
        let on = skip_ws(bytes, next);
        let mut on_end = on;
        while on_end < bytes.len() && is_name(bytes[on_end]) {
            on_end += 1;
        }
        if &query[on..on_end] == "on" {
            let type_start = skip_ws(bytes, on_end);
            let mut type_end = type_start;
            while type_end < bytes.len() && is_name(bytes[type_end]) {
                type_end += 1;
            }
            out.insert(name.to_string(), query[type_start..type_end].to_string());
        }
        at = next.max(at + found + 1);
    }
    out
}

fn is_name(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

fn skip_ws(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b',') {
        at += 1;
    }
    at
}

fn next_visible(bytes: &[u8], at: usize) -> Option<u8> {
    let at = skip_ws(bytes, at);
    bytes.get(at).copied()
}

fn skip_string(bytes: &[u8], at: usize) -> usize {
    let mut next = at + 1;
    while next < bytes.len() {
        match bytes[next] {
            0x5c => next += 2, // a backslash escapes the next character
            b'"' => return next + 1,
            _ => next += 1,
        }
    }
    next
}

/// Everything from `(` to its `)`, brackets inside counted.
fn skip_brackets(bytes: &[u8], at: usize) -> usize {
    let open = bytes[at];
    let close = if open == b'(' { b')' } else { b']' };
    let mut depth = 0usize;
    let mut next = at;
    while next < bytes.len() {
        match bytes[next] {
            b'"' => {
                next = skip_string(bytes, next);
                continue;
            }
            ch if ch == open => depth += 1,
            ch if ch == close => {
                depth -= 1;
                if depth == 0 {
                    return next + 1;
                }
            }
            _ => {}
        }
        next += 1;
    }
    next
}
#[cfg(test)]
mod tests {
    use super::*;

    /// An introspection answer in the shape the endpoint sends it.
    fn schema() -> Schema {
        let named = |name: &str| serde_json::json!({ "kind": "OBJECT", "name": name });
        let list_of = |name: &str| serde_json::json!({
            "kind": "NON_NULL",
            "ofType": { "kind": "LIST", "ofType": { "kind": "NON_NULL", "ofType": { "kind": "OBJECT", "name": name } } }
        });
        let field = |name: &str, kind: serde_json::Value, args: Vec<&str>| serde_json::json!({
            "name": name,
            "type": kind,
            "description": "",
            "args": args.iter().map(|a| serde_json::json!({ "name": a })).collect::<Vec<_>>(),
        });

        read(&serde_json::json!({
            "queryType": { "name": "Query" },
            "mutationType": { "name": "Mutation" },
            "types": [
                {
                    "kind": "OBJECT",
                    "name": "Query",
                    "description": "The root.",
                    "fields": [
                        field("me", named("User"), vec![]),
                        field("users", list_of("User"), vec!["first", "after"]),
                    ],
                },
                {
                    "kind": "OBJECT",
                    "name": "Mutation",
                    "description": "",
                    "fields": [field("signUp", named("User"), vec!["email"])],
                },
                {
                    "kind": "OBJECT",
                    "name": "User",
                    "description": "Somebody.",
                    "fields": [
                        field("id", serde_json::json!({ "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "ID" } }), vec![]),
                        field("name", serde_json::json!({ "kind": "SCALAR", "name": "String" }), vec![]),
                        field("friends", list_of("User"), vec![]),
                    ],
                },
                // No `fields`, so not something a selection set can be about.
                { "kind": "SCALAR", "name": "String", "description": "", "fields": null },
                // Introspection's own types are noise in a browser.
                { "kind": "OBJECT", "name": "__Type", "description": "", "fields": [] },
            ],
        }))
    }

    #[test]
    fn a_schema_reads_as_its_types_with_the_wrappers_kept_for_show() {
        let schema = schema();
        assert_eq!(schema.query_type, "Query");
        assert_eq!(schema.mutation_type, "Mutation");
        assert_eq!(
            schema.types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["Query", "Mutation", "User"],
            "scalars and introspection's own types are left out"
        );

        let users = schema.field("Query", "users").unwrap();
        assert_eq!(users.type_name, "[User!]!", "shown as the schema writes it");
        assert_eq!(users.of, "User", "and unwrapped for the walk");
        assert_eq!(users.args, vec!["first", "after"]);
    }

    #[test]
    fn a_field_the_schema_does_not_have_is_named_with_its_place_in_the_text() {
        let schema = schema();
        assert!(check(&schema, "{ me { id name } }").is_empty());
        assert!(check(&schema, "query Q($n: Int) { users(first: $n) { id friends { name } } }").is_empty());
        assert!(check(&schema, "mutation { signUp(email: \"a@b.c\") { id } }").is_empty());
        assert!(check(&schema, "{ me { __typename } }").is_empty(), "__typename is on every type");

        let query = "{ me { id emial } }";
        let found = check(&schema, query);
        assert_eq!(found.len(), 1);
        assert_eq!(&query[found[0].at..found[0].at + found[0].len], "emial");
        assert_eq!(found[0].message, "User has no field emial");
    }

    #[test]
    fn aliases_arguments_and_fragments_are_followed_or_left_alone() {
        let schema = schema();

        assert!(check(&schema, "{ who: me { handle: name } }").is_empty(), "an alias names nothing");
        assert!(
            check(&schema, "{ users(first: 10, after: \"me { nope }\") { id } }").is_empty(),
            "an argument is skipped whole, string and all"
        );
        assert!(check(&schema, "{ me { ... on User { name } } }").is_empty());
        assert!(
            check(&schema, "{ me { ...bits } }\nfragment bits on User { id nope }").len() == 1,
            "a spread is not followed, but its definition is checked where it is written"
        );
        assert!(
            check(&schema, "{ me { ... on Ghost { name } } }")
                .iter()
                .any(|p| p.message.contains("no type Ghost")),
            "a type that is not in the schema is said once, and nothing under it is marked"
        );
        assert!(check(&schema, "{ me @include(if: true) { id } }").is_empty());
        assert!(check(&schema, "{ me { id } }\n# me { nope }").is_empty(), "a comment is not a query");
    }

    #[test]
    fn completion_answers_with_the_fields_of_the_type_in_scope() {
        let schema = schema();

        let at_root = complete(&schema, "{  }", 2);
        assert_eq!(at_root.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["me", "users"]);

        let query = "{ me { na } }";
        let inside = complete(&schema, query, query.find("na").unwrap() + 2);
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].name, "name");
        assert_eq!(inside[0].prefix, "na", "what is typed is what the editor replaces");

        let deep = "{ me { friends {  } } }";
        let at_deep = complete(&schema, deep, deep.find("{  }").unwrap() + 2);
        assert_eq!(
            at_deep.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["id", "name", "friends"]
        );

        // A mutation starts somewhere else entirely.
        let mutation = "mutation {  }";
        assert_eq!(
            complete(&schema, mutation, mutation.len() - 2)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["signUp"]
        );

        // Where the walk lost track, it offers nothing rather than the wrong thing.
        assert!(complete(&schema, "{ me { nope {  } } }", 14).is_empty());
    }


    #[test]
    fn a_query_with_non_ascii_in_it_does_not_panic_and_still_lines_up() {
        let schema = schema();

        // The caret arrives in UTF-16 units. Treating it as a byte index used
        // to slice through the middle of a character and take the whole
        // command down with it, so every offset has to be safe.
        for at in 0..=14 {
            let _ = complete(&schema, "{ ıııı }", at);
            let _ = complete(&schema, "{ me { 😀 } }", at);
        }

        // A mark is reported where the editor will look for it. The Turkish
        // characters in the argument push the byte offset of `emial` two past
        // its UTF-16 offset, which is exactly the divergence that used to put
        // the underline in the wrong place.
        let query = "{ users(after: \"ıı\") { emial } }";
        let found = check(&schema, query);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_ne!(found[0].at, query.find("emial").unwrap(), "bytes and UTF-16 units differ here");

        let units: Vec<u16> = query.encode_utf16().collect();
        let marked = String::from_utf16(&units[found[0].at..found[0].at + found[0].len]).unwrap();
        assert_eq!(marked, "emial", "the mark covers the field the editor sees");

        // Completion still narrows on a prefix that sits after a wide character.
        let typed = "{ users(after: \"ıı\") { na } }";
        // The caret sits right after `na`, which is just before the trailing brace.
        let caret = typed.encode_utf16().count() - " } }".encode_utf16().count();
        let offered = complete(&schema, typed, caret);
        assert_eq!(offered.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["name"]);
        assert_eq!(offered[0].prefix, "na");
    }
}

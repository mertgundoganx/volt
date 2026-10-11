//! The `.bru` file format, read into blocks.
//!
//! A Bruno request is a text file of named blocks, and there are three kinds:
//!
//! ```text
//! headers {                 a dictionary: `key: value` a line, `~` for off
//!   accept: application/json
//!   ~x-debug: 1
//! }
//!
//! body:json {               text: whatever is inside, two spaces in
//!   { "name": "ada" }
//! }
//!
//! vars:secret [             a list of names
//!   apiKey,
//!   ~unused
//! ]
//! ```
//!
//! Which kind a block is follows from its name, as it does in Bruno's own
//! grammar, and a text block ends at a `}` in the first column — which is why
//! its content is indented: the braces inside it, balanced or not, never
//! reach the margin. This module only reads the shape. What a `post` block or
//! an `assert` block *means* is `bruno.rs`'s business.
//!
//! It is lenient on purpose and cannot fail: a line it does not understand is
//! passed over, because one odd file should not stop a collection of two
//! hundred from importing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub value: String,
    /// `~name: value` is an entry that is switched off.
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Pairs(Vec<Entry>),
    Text(String),
    /// Names only; `value` is empty.
    List(Vec<Entry>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub content: Content,
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    fn find(&self, name: &str) -> Option<&Content> {
        self.blocks.iter().find(|block| block.name == name).map(|block| &block.content)
    }

    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// A dictionary block's entries, or nothing when it is not there.
    pub fn pairs(&self, name: &str) -> &[Entry] {
        match self.find(name) {
            Some(Content::Pairs(entries)) => entries,
            _ => &[],
        }
    }

    pub fn list(&self, name: &str) -> &[Entry] {
        match self.find(name) {
            Some(Content::List(entries)) => entries,
            _ => &[],
        }
    }

    /// A text block's content, when it has any.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.find(name) {
            Some(Content::Text(text)) if !text.trim().is_empty() => Some(text),
            _ => None,
        }
    }

    /// One value out of a dictionary block. A switched-off entry is not a value.
    pub fn value(&self, block: &str, key: &str) -> Option<&str> {
        self.pairs(block).iter().find(|entry| entry.enabled && entry.name == key).map(|entry| entry.value.as_str())
    }
}

/// Blocks whose content is text rather than `key: value` lines.
fn is_text(name: &str) -> bool {
    match name {
        "tests" | "docs" | "body" => true,
        "body:form-urlencoded" | "body:multipart-form" | "body:file" => false,
        _ => name.starts_with("body:") || name.starts_with("script:"),
    }
}

pub fn parse(text: &str) -> Document {
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    let mut blocks = Vec::new();
    let mut at = 0;

    while at < lines.len() {
        let line = lines[at].trim();
        at += 1;

        // `name [ a, b ]` on one line.
        if let Some((name, rest)) = line.split_once('[') {
            if let Some(inside) = rest.trim_end().strip_suffix(']') {
                blocks.push(Block { name: name.trim().to_string(), content: Content::List(names(inside)) });
                continue;
            }
        }
        if let Some(name) = line.strip_suffix('[') {
            let mut inside = String::new();
            while at < lines.len() && !lines[at].trim_start().starts_with(']') {
                inside.push_str(lines[at]);
                inside.push('\n');
                at += 1;
            }
            at += 1;
            blocks.push(Block { name: name.trim().to_string(), content: Content::List(names(&inside)) });
            continue;
        }
        if let Some(name) = line.strip_suffix("{}") {
            blocks.push(Block { name: name.trim().to_string(), content: Content::Pairs(Vec::new()) });
            continue;
        }
        let Some(name) = line.strip_suffix('{') else { continue };
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }

        if is_text(&name) {
            let mut content: Vec<&str> = Vec::new();
            // Ends at a brace in the first column and nowhere else: the
            // content is indented, so its own braces never get there.
            while at < lines.len() && lines[at].trim_end() != "}" {
                content.push(dedent(lines[at]));
                at += 1;
            }
            at += 1;
            blocks.push(Block { name, content: Content::Text(content.join("\n")) });
            continue;
        }

        let mut entries = Vec::new();
        while at < lines.len() && lines[at].trim() != "}" {
            let line = lines[at].trim();
            at += 1;
            if line.is_empty() {
                continue;
            }
            let (enabled, line) = match line.strip_prefix('~') {
                Some(rest) => (false, rest.trim_start()),
                None => (true, line),
            };
            let (key, value) = key_and_value(line);
            if key.is_empty() {
                continue;
            }
            // `key: '''` opens a value that runs over several lines.
            let value = match value.strip_prefix("'''") {
                None => value.to_string(),
                Some(rest) => match rest.strip_suffix("'''") {
                    Some(inline) => inline.to_string(),
                    None => {
                        let mut long: Vec<&str> = Vec::new();
                        if !rest.trim().is_empty() {
                            long.push(rest);
                        }
                        while at < lines.len() {
                            let line = lines[at];
                            at += 1;
                            match line.trim_end().strip_suffix("'''") {
                                Some(last) => {
                                    if !last.trim().is_empty() {
                                        long.push(last);
                                    }
                                    break;
                                }
                                None => long.push(line),
                            }
                        }
                        outdent(&long)
                    }
                },
            };
            entries.push(Entry { name: key, value, enabled });
        }
        at += 1;
        blocks.push(Block { name, content: Content::Pairs(entries) });
    }
    Document { blocks }
}

/// `key: value`, where a key that has a colon of its own is in quotes.
fn key_and_value(line: &str) -> (String, &str) {
    if let Some(rest) = line.strip_prefix('"') {
        if let Some((key, after)) = rest.split_once('"') {
            let value = after.trim_start().strip_prefix(':').unwrap_or(after);
            return (key.to_string(), value.trim());
        }
    }
    match line.split_once(':') {
        Some((key, value)) => (key.trim().to_string(), value.trim()),
        None => (line.trim().to_string(), ""),
    }
}

/// The names in a list block: one a line, or several with commas between.
fn names(inside: &str) -> Vec<Entry> {
    inside
        .split([',', '\n'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| match name.strip_prefix('~') {
            Some(off) => Entry { name: off.trim().to_string(), value: String::new(), enabled: false },
            None => Entry { name: name.to_string(), value: String::new(), enabled: true },
        })
        .collect()
}

/// A text block's line without the two spaces Bruno puts in front of it.
fn dedent(line: &str) -> &str {
    line.strip_prefix("  ").or_else(|| line.strip_prefix('\t')).unwrap_or_else(|| line.trim_start_matches(' '))
}

/// Lines with the indentation they all share taken off.
fn outdent(lines: &[&str]) -> String {
    let shared = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    lines.iter().map(|line| line.get(shared..).unwrap_or("")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, value: &str, enabled: bool) -> Entry {
        Entry { name: name.into(), value: value.into(), enabled }
    }

    #[test]
    fn a_dictionary_block_is_keys_and_values_with_a_tilde_for_off() {
        let doc = parse("meta {\n  name: Create user\n  type: http\n  seq: 2\n}\n\nheaders {\n  accept: application/json\n  ~x-debug: 1\n  x-url: https://x.test:8080/a\n  empty:\n}\n");
        assert_eq!(doc.value("meta", "name"), Some("Create user"));
        assert_eq!(doc.value("meta", "seq"), Some("2"));
        assert_eq!(
            doc.pairs("headers"),
            [
                entry("accept", "application/json", true),
                entry("x-debug", "1", false),
                entry("x-url", "https://x.test:8080/a", true),
                entry("empty", "", true),
            ],
            "only the first colon divides, so a URL survives as a value"
        );
        assert_eq!(doc.value("headers", "x-debug"), None, "a switched-off entry is not a value");
        assert!(doc.pairs("nope").is_empty() && !doc.has("nope"));
    }

    #[test]
    fn a_text_block_keeps_its_own_braces_and_loses_only_the_indent() {
        let doc = parse(
            "post {\n  url: https://x.test\n}\n\nbody:json {\n  {\n    \"user\": { \"name\": \"ada\" },\n    \"tags\": [\"}\"]\n  }\n}\n\nscript:post-response {\n  if (res.status === 200) {\n    bru.setVar(\"ok\", true);\n  }\n}\n\ndocs {\n  # Title\n\n  A line.\n}\n",
        );
        assert_eq!(doc.text("body:json"), Some("{\n  \"user\": { \"name\": \"ada\" },\n  \"tags\": [\"}\"]\n}"));
        assert_eq!(doc.text("script:post-response"), Some("if (res.status === 200) {\n  bru.setVar(\"ok\", true);\n}"));
        assert_eq!(doc.text("docs"), Some("# Title\n\nA line."), "a blank line inside is part of the text");
        assert_eq!(doc.value("post", "url"), Some("https://x.test"), "and the block before is not swallowed");
    }

    #[test]
    fn a_list_block_reads_on_one_line_or_many() {
        let doc = parse("vars {\n  host: https://x.test\n}\nvars:secret [\n  apiKey,\n  ~old,\n  token\n]\n");
        assert_eq!(doc.list("vars:secret"), [entry("apiKey", "", true), entry("old", "", false), entry("token", "", true)]);
        assert_eq!(parse("vars:secret [ a, b ]").list("vars:secret").len(), 2);
        assert!(parse("vars:secret [\n]").list("vars:secret").is_empty());
        assert_eq!(doc.value("vars", "host"), Some("https://x.test"));
    }

    #[test]
    fn a_quoted_key_may_hold_a_colon_and_a_long_value_may_run_over_lines() {
        let doc = parse("headers {\n  \"x:odd\": 1\n  note: '''\n    first\n      second\n  '''\n  short: '''inline'''\n  after: 2\n}\n");
        assert_eq!(doc.value("headers", "x:odd"), Some("1"));
        assert_eq!(doc.value("headers", "note"), Some("first\n  second"));
        assert_eq!(doc.value("headers", "short"), Some("inline"));
        assert_eq!(doc.value("headers", "after"), Some("2"), "what follows a long value is still read");
    }

    #[test]
    fn windows_line_endings_a_byte_order_mark_and_stray_lines_are_all_survived() {
        let doc = parse("\u{feff}meta {\r\n  name: Ping\r\n}\r\nthis is not a block\r\nget {\r\n  url: https://x.test\r\n}\r\nbody:text {\r\n  hello\r\n}\r\n");
        assert_eq!(doc.value("meta", "name"), Some("Ping"));
        assert_eq!(doc.value("get", "url"), Some("https://x.test"));
        assert_eq!(doc.text("body:text"), Some("hello"));
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn a_file_that_ends_in_the_middle_of_a_block_is_read_as_far_as_it_goes() {
        assert_eq!(parse("headers {\n  a: 1").value("headers", "a"), Some("1"));
        assert_eq!(parse("body:json {\n  {").text("body:json"), Some("{"));
        assert_eq!(parse("vars:secret [\n  a").list("vars:secret").len(), 1);
        assert_eq!(parse("headers {\n  note: '''\n   never closed").value("headers", "note"), Some("never closed"));
        assert!(parse("").blocks.is_empty());
        assert!(parse("{\n}\n}\n][\n").blocks.iter().all(|block| !block.name.is_empty()));
        assert_eq!(parse("empty {}").pairs("empty").len(), 0);
    }
}

//! Secret values, kept in gitignored dotenv files next to `collection.yaml`.
//!
//! ```text
//! my-api/
//!   .env.local        # secrets for environments/local.yaml
//!   .env.prod         # secrets for environments/prod.yaml
//!   .env              # legacy: one shared value per name
//! ```
//!
//! Each environment owns the file named after its YAML file, so `token` can
//! differ between `local` and `prod`. Collections written before this format
//! have a single shared `.env`. It is still read as a fallback, and the first
//! save of an environment moves the values into per-environment files.
//!
//! Files are edited line by line rather than regenerated: comments and keys
//! someone added by hand are kept where they were.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const LEGACY_FILE: &str = ".env";
const GITIGNORE: &str = ".gitignore";
const HEADER_PREFIX: &str = "# Secret values for the";

/// The secrets file for the environment stored as `environments/<stem>.yaml`.
pub fn file_for(root: &Path, stem: &str) -> PathBuf {
    root.join(format!(".env.{stem}"))
}

/// Names that cannot round-trip through a dotenv line are refused up front,
/// rather than written and silently lost on the next read.
pub fn validate_name(name: &str) -> Result<()> {
    let problem = if name.is_empty() {
        Some("cannot be empty")
    } else if name.trim() != name {
        Some("cannot start or end with a space")
    } else if name.contains('=') || name.contains('\n') || name.contains('\r') {
        Some("cannot contain `=` or a line break")
    } else if name.starts_with('#') || name.starts_with("export ") {
        Some("cannot start with `#` or `export `")
    } else {
        None
    };
    match problem {
        Some(problem) => Err(Error::Invalid(format!("secret variable `{name}` {problem}"))),
        None => Ok(()),
    }
}

#[derive(Debug, Clone)]
enum Line {
    Entry {
        key: String,
        value: String,
        /// The original text, kept until the value changes, so an untouched
        /// line is written back exactly as it was read.
        raw: Option<String>,
    },
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct DotEnv {
    lines: Vec<Line>,
}

impl DotEnv {
    /// A new file for one environment, with a header saying what it is.
    pub fn for_environment(stem: &str) -> Self {
        DotEnv {
            lines: vec![Line::Other(format!("{HEADER_PREFIX} `{stem}` environment. Do not commit."))],
        }
    }

    /// `legacy` reads with the old rules: quotes trimmed, no escape sequences.
    /// Old files were written that way, so a backslash in them is literal.
    pub fn parse(text: &str, legacy: bool) -> Self {
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    return Line::Other(line.to_string());
                }
                let body = trimmed.strip_prefix("export ").map(str::trim_start).unwrap_or(trimmed);
                match body.split_once('=') {
                    Some((key, rest)) if !key.trim().is_empty() => Line::Entry {
                        key: key.trim().to_string(),
                        value: if legacy { legacy_value(rest) } else { parse_value(rest) },
                        raw: Some(line.to_string()),
                    },
                    _ => Line::Other(line.to_string()),
                }
            })
            .collect();
        DotEnv { lines }
    }

    /// The last definition wins, as in most dotenv loaders.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|line| match line {
            Line::Entry { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
    }

    /// Replace the value in place, or append it.
    pub fn set(&mut self, key: &str, value: &str) {
        let mut found = false;
        self.lines.retain_mut(|line| match line {
            Line::Entry { key: k, value: v, raw } if k == key => {
                if found {
                    return false; // a duplicate further down would override us
                }
                found = true;
                if v != value {
                    *v = value.to_string();
                    *raw = None;
                }
                true
            }
            _ => true,
        });
        if !found {
            self.lines.push(Line::Entry { key: key.to_string(), value: value.to_string(), raw: None });
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.lines.retain(|line| !matches!(line, Line::Entry { key: k, .. } if k == key));
    }

    pub fn keys(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Entry { key, .. } => Some(key.clone()),
                Line::Other(_) => None,
            })
            .collect()
    }

    /// Take every entry from `other`, which wins on conflicts.
    pub fn merge_from(&mut self, other: &DotEnv) {
        for line in &other.lines {
            if let Line::Entry { key, value, .. } = line {
                self.set(key, value);
            }
        }
    }

    /// Nothing worth keeping: no values and no comment someone wrote.
    pub fn is_disposable(&self) -> bool {
        self.lines.iter().all(|line| match line {
            Line::Entry { .. } => false,
            Line::Other(text) => text.trim().is_empty() || text.starts_with(HEADER_PREFIX),
        })
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Entry { raw: Some(raw), .. } | Line::Other(raw) => out.push_str(raw),
                Line::Entry { key, value, raw: None } => {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(&quote(value));
                }
            }
            out.push('\n');
        }
        out
    }
}

fn legacy_value(rest: &str) -> String {
    rest.trim().trim_matches('"').to_string()
}

fn parse_value(rest: &str) -> String {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('"') {
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return out,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    // Not an escape we write: keep both characters.
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                },
                other => out.push(other),
            }
        }
        out // unterminated: keep what there is rather than dropping the value
    } else if let Some(inner) = rest.strip_prefix('\'') {
        inner.split_once('\'').map_or(inner, |(value, _)| value).to_string()
    } else {
        // Unquoted, as someone might write by hand: `# ` after a space is a comment.
        rest.split_once(" #").map_or(rest, |(value, _)| value).trim_end().to_string()
    }
}

/// Always double-quoted and escaped, so a value with a quote, `#` or a line
/// break (a private key, say) survives the round trip.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `Ok(None)` when the file does not exist. Any other failure is an error: a
/// secrets file that silently read as empty would be overwritten with blanks
/// on the next save.
pub fn read(path: &Path, legacy: bool) -> Result<Option<DotEnv>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(DotEnv::parse(&text, legacy))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path.to_string_lossy(), e)),
    }
}

/// Write the file, or delete it once nothing in it is worth keeping. The
/// collection's `.gitignore` is checked first, every time.
pub fn write(root: &Path, path: &Path, env: &DotEnv) -> Result<()> {
    if env.is_disposable() {
        return match fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path.to_string_lossy(), e)),
            _ => Ok(()),
        };
    }
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    ensure_ignored(root, &file_name)?;
    // Atomic, like every other file volt writes: a half-written secrets file
    // is a lost credential the user has no copy of.
    crate::collection::write_atomic(path, &env.render())
}

// ---------------------------------------------------------------------------
// .gitignore
// ---------------------------------------------------------------------------

/// Make sure the collection's `.gitignore` covers `file_name`, appending what
/// is missing.
///
/// It appends *both* rules, not just the one that covers this file name.
/// Collections created before per-environment files only ignore `.env`, and a
/// file written by volt only ignored `.env.*` — so whichever one the collection
/// started with, the other kind of secrets file sat there uncovered. Writing
/// both is the only state in which neither can be committed by accident.
pub fn ensure_ignored(root: &Path, file_name: &str) -> Result<()> {
    let path = root.join(GITIGNORE);
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::io(path.to_string_lossy(), e)),
    };

    let mut rules: Vec<&str> = Vec::new();
    if !is_ignored(&existing, LEGACY_FILE) {
        rules.push(".env");
    }
    if !is_ignored(&existing, ".env.local") {
        rules.push(".env.*");
    }
    // The file the caller named can be excluded by a later negation even when
    // the general rule is there. Repeating that rule wins, because the last
    // match decides.
    let own = if file_name == LEGACY_FILE { ".env" } else { ".env.*" };
    if !is_ignored(&existing, file_name) && !rules.contains(&own) {
        rules.push(own);
    }
    if rules.is_empty() {
        return Ok(());
    }

    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("# Secret values, never commit these\n");
    for rule in rules {
        out.push_str(rule);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|e| Error::io(path.to_string_lossy(), e))
}
/// Whether a `.gitignore` in the collection root ignores a file in that same
/// root. Handles the rules that can target such a file — plain names, `*` and
/// `?`, a leading `/` or `**/`, and `!` negation with the last match winning.
/// Anything fancier counts as not matching, which errs towards adding a rule.
pub fn is_ignored(gitignore: &str, file_name: &str) -> bool {
    let mut ignored = false;
    for line in gitignore.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (negated, pattern) = match line.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        let pattern = pattern.strip_prefix("**/").or_else(|| pattern.strip_prefix('/')).unwrap_or(pattern);
        // Directory rules (`dir/`) and nested paths never name a root file.
        if pattern.contains('/') || pattern.contains('[') {
            continue;
        }
        if glob(pattern, file_name) {
            ignored = !negated;
        }
    }
    ignored
}

fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some('\\') if pattern.get(p + 1) == Some(&text[t]) => {
                p += 2;
                t += 1;
            }
            Some('?') => {
                p += 1;
                t += 1;
            }
            Some(&c) if c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awkward_values_survive_a_round_trip() {
        let values = [
            "plain",
            "with \"quotes\" and 'single'",
            "C:\\Users\\ada\\key",
            "-----BEGIN KEY-----\nline two\r\nline three\n-----END KEY-----",
            "a=b=c",
            "value # not a comment",
            "tab\there",
            "ğüşiöç 🔑",
            "",
            "trailing backslash\\",
        ];
        let mut env = DotEnv::for_environment("prod");
        for (i, value) in values.iter().enumerate() {
            env.set(&format!("K{i}"), value);
        }

        let text = env.render();
        assert_eq!(text.lines().count(), values.len() + 1, "one line per value, even multi-line ones:\n{text}");

        let reread = DotEnv::parse(&text, false);
        for (i, value) in values.iter().enumerate() {
            assert_eq!(reread.get(&format!("K{i}")), Some(*value), "K{i}");
        }
    }

    #[test]
    fn hand_written_lines_are_understood() {
        let env = DotEnv::parse(
            "# mine\nexport A=1\nB = 'single # quoted'\nC=bare value # trailing comment\nD=\"esc\\\"aped\"\nnot a pair\n=novalue\n",
            false,
        );
        assert_eq!(env.get("A"), Some("1"));
        assert_eq!(env.get("B"), Some("single # quoted"));
        assert_eq!(env.get("C"), Some("bare value"));
        assert_eq!(env.get("D"), Some("esc\"aped"));
        assert_eq!(env.keys(), ["A", "B", "C", "D"]);
    }

    #[test]
    fn legacy_files_keep_their_old_meaning() {
        // The old writer did not escape, so `\n` in an old file is two characters.
        let text = "# old\ntoken=\"ab\\ncd\"\npath=\"C:\\temp\"\n";
        let env = DotEnv::parse(text, true);
        assert_eq!(env.get("token"), Some("ab\\ncd"));
        assert_eq!(env.get("path"), Some("C:\\temp"));
        // Untouched lines are written back byte for byte.
        let mut pruned = env.clone();
        pruned.remove("token");
        assert_eq!(pruned.render(), "# old\npath=\"C:\\temp\"\n");
    }

    #[test]
    fn editing_keeps_comments_order_and_untouched_lines() {
        let mut env = DotEnv::parse("# header\nA=\"1\"\n# about B\nB='two'\nC=3\n", false);
        env.set("B", "changed");
        env.set("A", "1"); // same value: line stays exactly as written
        env.set("Z", "new");
        env.remove("C");
        assert_eq!(env.render(), "# header\nA=\"1\"\n# about B\nB=\"changed\"\nZ=\"new\"\n");
    }

    #[test]
    fn a_duplicate_key_is_collapsed_when_set() {
        let mut env = DotEnv::parse("A=1\nA=2\n", false);
        assert_eq!(env.get("A"), Some("2"), "last one wins on read");
        env.set("A", "3");
        assert_eq!(env.render(), "A=\"3\"\n");
    }

    #[test]
    fn only_the_header_and_blank_lines_are_disposable() {
        let mut env = DotEnv::for_environment("dev");
        assert!(env.is_disposable());
        env.set("A", "1");
        env.remove("A");
        assert!(env.is_disposable());
        assert!(!DotEnv::parse("# a note someone wrote\n", false).is_disposable());
    }

    #[test]
    fn names_that_cannot_round_trip_are_refused() {
        assert!(validate_name("token").is_ok());
        assert!(validate_name("aws.region").is_ok());
        for bad in ["", " token", "token ", "a=b", "a\nb", "#x", "export x"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn gitignore_matching() {
        let cases = [
            (".env\n", ".env.prod", false),
            (".env\n", ".env", true),
            (".env*\n", ".env.prod", true),
            (".env.*\n", ".env.prod", true),
            ("*.prod\n", ".env.prod", true),
            ("/.env.*\n", ".env.prod", true),
            ("**/.env.*\n", ".env.prod", true),
            (".env.?rod\n", ".env.prod", true),
            (".env.*\n!.env.prod\n", ".env.prod", false),
            ("!.env.prod\n.env.*\n", ".env.prod", true),
            ("# .env.*\n", ".env.prod", false),
            ("secrets/.env.*\n", ".env.prod", false),
            (".env.*/\n", ".env.prod", false),
            ("[.]env.*\n", ".env.prod", false),
            ("", ".env.prod", false),
        ];
        for (gitignore, file, expected) in cases {
            assert_eq!(is_ignored(gitignore, file), expected, "{gitignore:?} vs {file}");
        }
    }

    #[test]
    fn ensure_ignored_adds_a_rule_only_when_needed() {
        let root = std::env::temp_dir().join(format!("volt-gitignore-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(&root).unwrap();
        let gitignore = root.join(GITIGNORE);

        // A collection from before per-environment files.
        fs::write(&gitignore, "# Secret values, never commit these\n.env").unwrap();
        ensure_ignored(&root, ".env.prod").unwrap();
        let text = fs::read_to_string(&gitignore).unwrap();
        assert!(text.starts_with("# Secret values, never commit these\n.env\n"), "no missing newline: {text:?}");
        assert!(is_ignored(&text, ".env.prod"));

        // Both kinds are covered now, so the legacy file cannot be committed
        // either — that was the gap.
        assert!(is_ignored(&text, ".env"), "the legacy shared file too: {text:?}");

        // Already covered: untouched.
        ensure_ignored(&root, ".env.dev").unwrap();
        assert_eq!(fs::read_to_string(&gitignore).unwrap(), text);

        // A later negation is overridden by the rule appended after it.
        fs::write(&gitignore, ".env.*\n!.env.prod\n").unwrap();
        ensure_ignored(&root, ".env.prod").unwrap();
        assert!(is_ignored(&fs::read_to_string(&gitignore).unwrap(), ".env.prod"));

        // No .gitignore at all.
        fs::remove_file(&gitignore).unwrap();
        ensure_ignored(&root, ".env.prod").unwrap();
        assert!(is_ignored(&fs::read_to_string(&gitignore).unwrap(), ".env.prod"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn writing_a_disposable_file_deletes_it() {
        let root = std::env::temp_dir().join(format!("volt-dispose-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(&root).unwrap();
        let path = file_for(&root, "dev");

        let mut env = DotEnv::for_environment("dev");
        env.set("token", "x");
        write(&root, &path, &env).unwrap();
        assert!(path.is_file());

        env.remove("token");
        write(&root, &path, &env).unwrap();
        assert!(!path.exists());
        // And deleting something already gone is fine.
        write(&root, &path, &env).unwrap();

        fs::remove_dir_all(&root).ok();
    }
}

//! curl, both ways.
//!
//! `parse` turns a pasted command into a request: what browsers produce with
//! "Copy as cURL" (both the bash and the Windows cmd flavour) and what API docs
//! show. `export` prints a request as a command from the same `http::plan` that
//! Send uses, so the two cannot drift apart.
//!
//! Nothing is dropped quietly. An option volt cannot apply, a file the command
//! would read, or a header volt manages itself comes back as a note.

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use serde::Serialize;

use crate::collection;
use crate::error::{Error, Result};
use crate::http::{self, ExecOptions, Plan, PlanBody, PlanContext};
use crate::import::{extract_request_secrets, json_has_literal_secret, split_query};
use crate::model::{Auth, Body, EnvVar, FormField, KeyValue, Request, RequestOptions};

// ---------------------------------------------------------------------------
// Tokenising
// ---------------------------------------------------------------------------

fn unterminated() -> Error {
    Error::Invalid("the curl command has an unterminated quote".into())
}

/// Browsers' "Copy as cURL (cmd)" escapes with carets; that decides the rules.
fn looks_like_cmd(input: &str) -> bool {
    input.contains("^\"") || input.lines().any(|line| line.trim_end().ends_with('^'))
}

fn tokenize(input: &str) -> Result<Vec<String>> {
    let input = input.replace("\r\n", "\n");
    if looks_like_cmd(&input) {
        Ok(tokenize_cmd(&input))
    } else {
        tokenize_bash(&input)
    }
}

fn tokenize_bash(input: &str) -> Result<Vec<String>> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
                i += 1;
            }
            '\\' => {
                match chars.get(i + 1) {
                    // A backslash at the end of a line continues the command.
                    Some('\n') => {}
                    Some(&next) => {
                        current.push(next);
                        in_token = true;
                    }
                    None => {}
                }
                i += 2;
            }
            '\'' => {
                in_token = true;
                let end = chars[i + 1..].iter().position(|&c| c == '\'').ok_or_else(unterminated)? + i + 1;
                current.extend(&chars[i + 1..end]);
                i = end + 1;
            }
            '$' if chars.get(i + 1) == Some(&'\'') => {
                in_token = true;
                i = ansi_c_string(&chars, i + 2, &mut current)?;
            }
            '"' => {
                in_token = true;
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(unterminated()),
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') => match chars.get(i + 1) {
                            Some('\n') => i += 2,
                            Some(&next @ ('"' | '\\' | '$' | '`')) => {
                                current.push(next);
                                i += 2;
                            }
                            _ => {
                                current.push('\\');
                                i += 1;
                            }
                        },
                        Some(&c) => {
                            current.push(c);
                            i += 1;
                        }
                    }
                }
            }
            c => {
                current.push(c);
                in_token = true;
                i += 1;
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    Ok(tokens)
}

/// `$'...'`: the quoting Chrome uses for bodies with special characters.
fn ansi_c_string(chars: &[char], mut i: usize, out: &mut String) -> Result<usize> {
    fn hex(chars: &[char], i: &mut usize, max: usize) -> Option<char> {
        let digits: String = chars[*i..].iter().take(max).take_while(|c| c.is_ascii_hexdigit()).collect();
        if digits.is_empty() {
            return None;
        }
        *i += digits.len();
        u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32)
    }

    loop {
        match chars.get(i) {
            None => return Err(unterminated()),
            Some('\'') => return Ok(i + 1),
            Some('\\') => {
                let next = *chars.get(i + 1).ok_or_else(unterminated)?;
                i += 2;
                let decoded = match next {
                    'n' => Some('\n'),
                    't' => Some('\t'),
                    'r' => Some('\r'),
                    'a' => Some('\u{7}'),
                    'b' => Some('\u{8}'),
                    'e' | 'E' => Some('\u{1b}'),
                    'f' => Some('\u{c}'),
                    'v' => Some('\u{b}'),
                    '0' => Some('\0'),
                    '\\' | '\'' | '"' | '?' => Some(next),
                    'x' => hex(chars, &mut i, 2),
                    'u' => hex(chars, &mut i, 4),
                    'U' => hex(chars, &mut i, 8),
                    _ => None,
                };
                match decoded {
                    Some(c) => out.push(c),
                    None => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
            Some(&c) => {
                out.push(c);
                i += 1;
            }
        }
    }
}

/// Windows cmd: carets escape the next character (a caret before a line break
/// continues the command), then the result splits by the C runtime's rules.
fn tokenize_cmd(input: &str) -> Vec<String> {
    let mut unescaped = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' {
            match chars.next() {
                Some('\n') | None => {}
                Some(next) => unescaped.push(next),
            }
        } else {
            unescaped.push(c);
        }
    }

    let chars: Vec<char> = unescaped.chars().collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut in_quotes = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                let run = chars[i..].iter().take_while(|&&c| c == '\\').count();
                if chars.get(i + run) == Some(&'"') {
                    current.push_str(&"\\".repeat(run / 2));
                    if run % 2 == 1 {
                        current.push('"');
                        i += run + 1;
                    } else {
                        i += run;
                    }
                } else {
                    current.push_str(&"\\".repeat(run));
                    i += run;
                }
                in_token = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                in_token = true;
                i += 1;
            }
            ' ' | '\t' | '\n' if !in_quotes => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
                i += 1;
            }
            c => {
                current.push(c);
                in_token = true;
                i += 1;
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Short options that take a value, and the long name each stands for.
const SHORT_WITH_VALUE: &[(char, &str)] = &[
    ('X', "request"), ('H', "header"), ('d', "data"), ('F', "form"), ('u', "user"), ('A', "user-agent"),
    ('e', "referer"), ('b', "cookie"), ('o', "output"), ('w', "write-out"), ('m', "max-time"), ('x', "proxy"),
    ('T', "upload-file"), ('E', "cert"), ('K', "config"), ('r', "range"), ('U', "proxy-user"), ('C', "continue-at"),
    ('y', "speed-time"), ('Y', "speed-limit"), ('z', "time-cond"), ('c', "cookie-jar"), ('D', "dump-header"),
    ('P', "ftp-port"), ('Q', "quote"), ('t', "telnet-option"),
];

const SHORT_FLAGS: &[(char, &str)] = &[
    ('G', "get"), ('I', "head"), ('k', "insecure"), ('L', "location"), ('s', "silent"), ('S', "show-error"),
    ('v', "verbose"), ('i', "include"), ('f', "fail"), ('g', "globoff"), ('N', "no-buffer"), ('O', "remote-name"),
    ('J', "remote-header-name"), ('#', "progress-bar"), ('4', "ipv4"), ('6', "ipv6"), ('0', "http1.0"),
    ('j', "junk-session-cookies"), ('l', "list-only"), ('n', "netrc"), ('q', "disable"), ('R', "remote-time"),
    ('Z', "parallel"), ('V', "version"), ('h', "help"), ('M', "manual"), ('a', "append"), ('B', "use-ascii"),
    ('p', "proxytunnel"),
];

/// Long options volt understands that take a value.
const LONG_WITH_VALUE: &[&str] = &[
    "request", "header", "data", "data-ascii", "data-raw", "data-binary", "data-urlencode", "json", "form",
    "form-string", "user", "user-agent", "referer", "cookie", "url", "oauth2-bearer",
];

/// Output and diagnostics: irrelevant to the request, ignored without a note.
const IGNORED_WITH_VALUE: &[&str] = &["output", "write-out", "stderr", "trace", "trace-ascii", "dump-header"];
const IGNORED_FLAGS: &[&str] = &[
    "silent", "show-error", "verbose", "include", "fail", "fail-with-body", "compressed", "globoff", "no-buffer",
    "no-progress-meter", "progress-bar", "ipv4", "ipv6", "tcp-nodelay", "no-keepalive", "path-as-is", "raw",
    "ssl-no-revoke", "http1.0", "http1.1", "http2", "http2-prior-knowledge", "http3", "tlsv1", "tlsv1.0",
    "tlsv1.1", "tlsv1.2", "tlsv1.3", "remote-name", "remote-name-all", "remote-header-name", "create-dirs",
    "styled-output", "no-styled-output", "location", "location-trusted", "disable", "basic", "remote-time",
    "junk-session-cookies", "list-only", "parallel", "append", "use-ascii", "proxytunnel", "netrc", "version",
    "help", "manual",
];

/// Transport and TLS settings with a value that volt does not take per request.
const UNSUPPORTED_WITH_VALUE: &[&str] = &[
    "max-time", "connect-timeout", "proxy", "proxy-user", "preproxy", "cert", "cert-type", "key", "key-type",
    "pass", "cacert", "capath", "resolve", "connect-to", "upload-file", "range", "retry", "retry-delay",
    "retry-max-time", "limit-rate", "interface", "dns-servers", "doh-url", "keepalive-time", "max-redirs",
    "expect100-timeout", "unix-socket", "abstract-unix-socket", "aws-sigv4", "local-port", "noproxy", "socks4",
    "socks4a", "socks5", "socks5-hostname", "tls-max", "ciphers", "pinnedpubkey", "cookie-jar", "time-cond",
    "continue-at", "speed-time", "speed-limit", "config", "ftp-port", "quote", "telnet-option", "variable",
    "engine", "hostpubmd5", "krb", "login-options", "mail-from", "mail-rcpt", "proto", "proto-redir",
    "service-name", "sasl-authzid", "happy-eyeballs-timeout-ms", "header-file",
];

enum Data {
    Raw(String),
    Encoded { name: String, value: String },
}

#[derive(Default)]
struct Parts {
    url: Option<String>,
    extra_urls: usize,
    method: Option<String>,
    head: bool,
    get: bool,
    json: bool,
    insecure: bool,
    digest: bool,
    timeout_ms: Option<u64>,
    proxy: Option<String>,
    client_cert: Option<String>,
    headers: Vec<(String, String)>,
    data: Vec<Data>,
    /// name, value-or-path, and whether it is a file.
    form: Vec<(String, String, bool)>,
    user: Option<String>,
    bearer: Option<String>,
    cookies: Vec<String>,
    unsupported: Vec<String>,
    not_read: Vec<String>,
    other_auth: Vec<String>,
    unknown: Vec<String>,
}

/// The value for an option: inline (`--data=x`, `-XPOST`) or the next token.
fn value_for(name: &str, inline: Option<String>, tokens: &[String], i: &mut usize) -> Result<String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    let value = tokens
        .get(*i)
        .cloned()
        .ok_or_else(|| Error::Invalid(format!("the curl option `--{name}` is missing its value")))?;
    *i += 1;
    Ok(value)
}

fn read_options(tokens: &[String]) -> Result<Parts> {
    let mut parts = Parts::default();
    let mut i = 1;
    let mut positional_only = false;

    while i < tokens.len() {
        let token = tokens[i].clone();
        i += 1;

        if positional_only || !token.starts_with('-') || token == "-" {
            if parts.url.is_none() {
                parts.url = Some(token);
            } else {
                parts.extra_urls += 1;
            }
            continue;
        }
        if token == "--" {
            positional_only = true;
            continue;
        }

        if let Some(long) = token.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (long.to_string(), None),
            };
            apply(&mut parts, &name, inline, tokens, &mut i, &format!("--{name}"))?;
            continue;
        }

        // Short options, possibly combined (`-sSL`) or with the value attached (`-XPOST`).
        let flags: Vec<char> = token[1..].chars().collect();
        let mut k = 0;
        while k < flags.len() {
            let flag = flags[k];
            if let Some((_, long)) = SHORT_WITH_VALUE.iter().find(|(c, _)| *c == flag) {
                let rest: String = flags[k + 1..].iter().collect();
                let inline = (!rest.is_empty()).then_some(rest);
                apply(&mut parts, long, inline, tokens, &mut i, &format!("-{flag}"))?;
                break;
            }
            match SHORT_FLAGS.iter().find(|(c, _)| *c == flag) {
                Some((_, long)) => apply(&mut parts, long, None, tokens, &mut i, &format!("-{flag}"))?,
                None => parts.unknown.push(format!("-{flag}")),
            }
            k += 1;
        }
    }
    Ok(parts)
}

fn apply(parts: &mut Parts, name: &str, inline: Option<String>, tokens: &[String], i: &mut usize, shown: &str) -> Result<()> {
    if LONG_WITH_VALUE.contains(&name) {
        let value = value_for(name, inline, tokens, i)?;
        match name {
            "request" => parts.method = Some(value.to_uppercase()),
            "header" => match value.split_once(':') {
                Some((header, rest)) if !header.trim().is_empty() => {
                    // `Name:` with nothing after it tells curl to remove a header.
                    if !rest.trim().is_empty() {
                        parts.headers.push((header.trim().to_string(), rest.trim().to_string()));
                    }
                }
                _ => match value.strip_suffix(';') {
                    // `Name;` sends the header with an empty value.
                    Some(header) if !header.trim().is_empty() => parts.headers.push((header.trim().to_string(), String::new())),
                    _ => parts.unknown.push(format!("-H {value}")),
                },
            },
            "data" | "data-ascii" | "data-binary" if value.starts_with('@') => parts.not_read.push(format!("{shown} {value}")),
            "data" | "data-ascii" | "data-binary" | "data-raw" => parts.data.push(Data::Raw(value)),
            "json" if value.starts_with('@') => parts.not_read.push(format!("--json {value}")),
            "json" => {
                parts.json = true;
                parts.data.push(Data::Raw(value));
            }
            "data-urlencode" => {
                // name=content, =content, content, name@file, @file
                if let Some((field, content)) = value.split_once('=') {
                    parts.data.push(Data::Encoded { name: field.to_string(), value: content.to_string() });
                } else if value.contains('@') {
                    parts.not_read.push(format!("--data-urlencode {value}"));
                } else {
                    parts.data.push(Data::Encoded { name: String::new(), value });
                }
            }
            "form" | "form-string" => match value.split_once('=') {
                // `-F name=@path` uploads a file; `-F name=<path` reads the
                // value from one, which is a different thing and still not
                // something volt does.
                Some((field, content)) if name == "form" && content.starts_with('@') => {
                    parts.form.push((field.to_string(), content[1..].to_string(), true))
                }
                Some((field, content)) if name == "form" && content.starts_with('<') => {
                    parts.not_read.push(format!("-F {field}={content}"))
                }
                Some((field, content)) => parts.form.push((field.to_string(), content.to_string(), false)),
                None => parts.unknown.push(format!("-F {value}")),
            },
            "user" => parts.user = Some(value),
            "user-agent" => parts.headers.push(("User-Agent".into(), value)),
            "referer" => parts.headers.push(("Referer".into(), value)),
            "cookie" if value.contains('=') => parts.cookies.push(value),
            "cookie" => parts.not_read.push(format!("{shown} {value}")),
            "url" => {
                if parts.url.is_none() {
                    parts.url = Some(value);
                } else {
                    parts.extra_urls += 1;
                }
            }
            "oauth2-bearer" => parts.bearer = Some(value),
            _ => {}
        }
        return Ok(());
    }

    // Options volt can actually honour, now that a request carries its own
    // send settings. They are read before the tables below, which would
    // otherwise file them under "not applied".
    match name {
        "max-time" => {
            let value = value_for(name, inline, tokens, i)?;
            parts.timeout_ms = value.trim().parse::<f64>().ok().map(|s| (s * 1000.0).round() as u64);
            return Ok(());
        }
        "proxy" => {
            parts.proxy = Some(value_for(name, inline, tokens, i)?);
            return Ok(());
        }
        "cert" => {
            parts.client_cert = Some(value_for(name, inline, tokens, i)?);
            return Ok(());
        }
        _ => {}
    }

    if IGNORED_WITH_VALUE.contains(&name) {
        value_for(name, inline, tokens, i)?;
        return Ok(());
    }
    if UNSUPPORTED_WITH_VALUE.contains(&name) {
        value_for(name, inline, tokens, i)?;
        parts.unsupported.push(shown.to_string());
        return Ok(());
    }

    match name {
        "get" => parts.get = true,
        "head" => parts.head = true,
        "insecure" => parts.insecure = true,
        // `--digest` is a scheme volt speaks now, so it is applied rather than
        // reported. The rest are still notes.
        "digest" => parts.digest = true,
        "ntlm" | "negotiate" | "anyauth" | "ntlm-wb" => parts.other_auth.push(shown.to_string()),
        _ if IGNORED_FLAGS.contains(&name) => {}
        _ if name.starts_with("no-") => {}
        _ => parts.unknown.push(shown.to_string()),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Building the request
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parsed {
    pub request: Request,
    /// Secret variables holding the credentials the command had in plain text,
    /// for the caller to add to the active environment.
    pub new_secrets: Vec<EnvVar>,
    pub notes: Vec<String>,
}

/// Headers a browser adds on its own. Kept, so nothing is hidden, but off.
fn browser_only(lower: &str) -> bool {
    lower.starts_with("sec-ch-")
        || lower.starts_with("sec-fetch-")
        || matches!(lower, "priority" | "upgrade-insecure-requests" | "dnt" | "sec-gpc")
}

/// Is this pasted text a curl command at all?
pub fn is_curl(text: &str) -> bool {
    let trimmed = text.trim_start().trim_start_matches("$ ");
    let first = trimmed.split_whitespace().next().unwrap_or("");
    let program = first.rsplit(['/', '\\']).next().unwrap_or(first).to_ascii_lowercase();
    program == "curl" || program == "curl.exe"
}

pub fn parse(command: &str, existing: &[EnvVar]) -> Result<Parsed> {
    let mut tokens = tokenize(command.trim())?;
    if tokens.first().is_some_and(|t| t == "$") {
        tokens.remove(0);
    }
    if !tokens.first().is_some_and(|t| is_curl(t)) {
        return Err(Error::Invalid("this is not a curl command".into()));
    }

    let parts = read_options(&tokens)?;
    let mut notes: Vec<String> = Vec::new();

    let raw_url = parts.url.clone().ok_or_else(|| Error::Invalid("the curl command has no URL".into()))?;
    // curl assumes http:// when there is no scheme.
    let url = if raw_url.contains("://") || raw_url.starts_with("{{") { raw_url } else { format!("http://{raw_url}") };
    let (base_url, mut params) = split_query(&url);

    // --- Headers and auth ---------------------------------------------------
    let mut auth: Option<Auth> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut left_to_volt: Vec<String> = Vec::new();
    let mut switched_off: Vec<String> = Vec::new();

    let mut all_headers = parts.headers.clone();
    if !parts.cookies.is_empty() {
        all_headers.push(("Cookie".into(), parts.cookies.join("; ")));
    }

    for (name, value) in all_headers {
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "content-length" | "accept-encoding") {
            left_to_volt.push(name);
            continue;
        }
        if lower == "authorization" {
            if let Some(parsed) = auth_from_header(&value) {
                auth = Some(parsed);
                continue;
            }
        }
        let enabled = !browser_only(&lower);
        if !enabled {
            switched_off.push(name.clone());
        }
        // A later header of the same name replaces an earlier one, as in curl.
        headers.retain(|h| !h.name.eq_ignore_ascii_case(&name));
        headers.push(KeyValue { name, value, enabled, description: None });
    }

    // An Authorization header wins over -u, as it does in curl.
    if auth.is_none() {
        if let Some(user) = &parts.user {
            let (username, password) = match user.split_once(':') {
                Some((u, p)) => (u.to_string(), p.to_string()),
                None => {
                    notes.push("The command gives a user without a password, so curl would have asked for one; the password is empty.".into());
                    (user.clone(), String::new())
                }
            };
            auth = Some(if parts.digest {
                Auth::Digest { username, password }
            } else {
                Auth::Basic { username, password }
            });
        } else if let Some(token) = &parts.bearer {
            auth = Some(Auth::Bearer { token: token.clone() });
        }
    }

    if parts.json {
        for (name, value) in [("Content-Type", "application/json"), ("Accept", "application/json")] {
            if !headers.iter().any(|h| h.name.eq_ignore_ascii_case(name)) {
                headers.push(KeyValue { name: name.into(), value: value.into(), enabled: true, description: None });
            }
        }
    }

    // --- Body ---------------------------------------------------------------
    let content_type = headers
        .iter()
        .find(|h| h.enabled && h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default();
    let drop_content_type = |headers: &mut Vec<KeyValue>| headers.retain(|h| !h.name.eq_ignore_ascii_case("content-type"));

    let mut body = Body::None;
    if parts.get {
        // -G moves the data into the query string.
        for data in &parts.data {
            match data {
                Data::Raw(raw) => params.extend(url::form_urlencoded::parse(raw.as_bytes()).map(|(n, v)| field(&n, &v))),
                Data::Encoded { name, value } => params.push(field(name, value)),
            }
        }
    } else if !parts.form.is_empty() {
        if !parts.data.is_empty() {
            notes.push("curl cannot send -d and -F together; the -F fields were kept.".into());
        }
        body = Body::Form {
            fields: parts
                .form
                .iter()
                .map(|(name, value, file)| FormField {
                    name: name.clone(),
                    value: value.clone(),
                    enabled: true,
                    description: None,
                    file: *file,
                })
                .collect(),
        };
        // The multipart boundary is generated when sending; a fixed header would break it.
        drop_content_type(&mut headers);
    } else if !parts.data.is_empty() {
        let joined = join_data(&parts.data);
        let only_encoded = parts.data.iter().all(|d| matches!(d, Data::Encoded { .. }));
        let start = joined.trim_start();

        body = if parts.json || content_type.contains("json") {
            Body::Json { content: joined }
        } else if content_type.contains("xml") {
            Body::Xml { content: joined }
        } else if content_type.is_empty() || content_type.contains("x-www-form-urlencoded") {
            if only_encoded || looks_like_pairs(&joined) {
                drop_content_type(&mut headers);
                Body::UrlEncoded { fields: form_fields(&parts.data) }
            } else if content_type.is_empty() && (start.starts_with('{') || start.starts_with('[')) {
                Body::Json { content: joined }
            } else {
                if content_type.is_empty() {
                    // curl labels -d data as a form unless told otherwise; keep that.
                    headers.push(KeyValue {
                        name: "Content-Type".into(),
                        value: "application/x-www-form-urlencoded".into(),
                        enabled: true,
                        description: None,
                    });
                }
                Body::Text { content: joined }
            }
        } else {
            Body::Text { content: joined }
        };
    }

    let method = match &parts.method {
        Some(method) => method.clone(),
        None if parts.head => "HEAD".into(),
        None if !matches!(body, Body::None) => "POST".into(),
        None => "GET".into(),
    };

    let mut request = Request {
        name: name_from_url(&base_url),
        kind: crate::model::Kind::Http,
        seq: 1,
        method,
        url: base_url,
        headers,
        params,
        body,
        auth: auth.unwrap_or(Auth::None),
        captures: Vec::new(),
        checks: Vec::new(),
        options: None,
        docs: None,
    };

    // What the command asked for about the sending itself, which a request can
    // now carry instead of it only becoming a note.
    let options = RequestOptions {
        timeout_ms: parts.timeout_ms,
        verify_tls: parts.insecure.then_some(false),
        proxy: parts.proxy.clone(),
        client_cert: parts.client_cert.clone(),
        follow_redirects: None,
    };
    if !options.is_empty() {
        request.options = Some(options);
    }

    // --- Notes --------------------------------------------------------------
    if parts.insecure {
        notes.push("TLS verification is off for this request (-k). Turn it back on in its Options tab.".into());
    }
    if let Some(proxy) = &parts.proxy {
        notes.push(format!("This request will go through the proxy {proxy}, from the command."));
    }
    if !parts.other_auth.is_empty() {
        notes.push(format!("{} is not supported; only basic and bearer auth are.", parts.other_auth.join(", ")));
    }
    if !parts.not_read.is_empty() {
        notes.push(format!("Files are not read from disk, so these were left out: {}.", parts.not_read.join(", ")));
    }
    if !parts.unsupported.is_empty() {
        notes.push(format!("Not applied, since volt does not set them per request: {}.", dedupe(parts.unsupported).join(", ")));
    }
    if !parts.unknown.is_empty() {
        notes.push(format!("Not understood and ignored: {}.", dedupe(parts.unknown).join(", ")));
    }
    if !switched_off.is_empty() {
        notes.push(format!("Headers a browser adds itself were kept but switched off: {}.", switched_off.join(", ")));
    }
    if !left_to_volt.is_empty() {
        notes.push(format!("Left for volt to set when sending: {}.", dedupe(left_to_volt).join(", ")));
    }
    if parts.extra_urls > 0 {
        notes.push("The command has more than one URL; only the first was used.".into());
    }
    if let Body::Json { content } = &request.body {
        if json_has_literal_secret(content) {
            notes.push("The JSON body appears to contain a plain-text credential; move it into a secret variable before saving.".into());
        }
    }

    let new_secrets = extract_request_secrets(&mut request, existing);
    Ok(Parsed { request, new_secrets, notes })
}

fn field(name: &str, value: &str) -> KeyValue {
    KeyValue { name: name.to_string(), value: value.to_string(), enabled: true, description: None }
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|item| seen.insert(item.to_ascii_lowercase())).collect()
}

/// `Authorization: Bearer x` and `Basic <base64>` become the auth they are.
fn auth_from_header(value: &str) -> Option<Auth> {
    let (scheme, credential) = value.trim().split_once(' ')?;
    let credential = credential.trim();
    if scheme.eq_ignore_ascii_case("bearer") && !credential.is_empty() {
        return Some(Auth::Bearer { token: credential.to_string() });
    }
    if scheme.eq_ignore_ascii_case("basic") {
        let decoded = base64::engine::general_purpose::STANDARD.decode(credential).ok()?;
        let text = String::from_utf8(decoded).ok()?;
        let (username, password) = text.split_once(':')?;
        return Some(Auth::Basic { username: username.to_string(), password: password.to_string() });
    }
    None
}

/// The body curl would send for all the -d options, in order, joined by `&`.
fn join_data(data: &[Data]) -> String {
    data.iter()
        .map(|d| match d {
            Data::Raw(raw) => raw.clone(),
            Data::Encoded { name, value } => {
                let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
                if name.is_empty() { encoded } else { format!("{name}={encoded}") }
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn looks_like_pairs(body: &str) -> bool {
    !body.is_empty() && body.split('&').all(|pair| pair.contains('=') && !pair.chars().any(char::is_whitespace))
}

fn form_fields(data: &[Data]) -> Vec<KeyValue> {
    let mut fields = Vec::new();
    for d in data {
        match d {
            Data::Raw(raw) => fields.extend(url::form_urlencoded::parse(raw.as_bytes()).map(|(n, v)| field(&n, &v))),
            Data::Encoded { name, value } => fields.push(field(name, value)),
        }
    }
    fields
}

/// A readable name from the URL's path, or its host when there is no path.
fn name_from_url(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (host, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, ""),
    };
    let path = path.trim_end_matches('/');
    let name = if path.is_empty() { host } else { path };
    if name.chars().count() > 60 {
        format!("{}…", name.chars().take(59).collect::<String>())
    } else {
        name.to_string()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub command: String,
    /// Secret variables left as `{{name}}` instead of their values.
    pub hidden: Vec<String>,
    /// Variables with no value anywhere, also left as `{{name}}`.
    pub undefined: Vec<String>,
}

/// Print a request as a curl command, resolved exactly as Send would resolve
/// it. Unless `include_secrets`, secret variables stay as `{{name}}` so the
/// command is safe to paste into a chat or a ticket.
pub fn export(
    root: &std::path::Path,
    request: &Request,
    env_vars: &[EnvVar],
    scopes: &collection::Scopes,
    options: &ExecOptions,
    include_secrets: bool,
) -> Result<Rendered> {
    let secret_names: HashSet<&str> = env_vars.iter().filter(|v| v.secret).map(|v| v.name.as_str()).collect();
    let visible: Vec<EnvVar> = env_vars.iter().filter(|v| include_secrets || !v.secret).cloned().collect();
    let vars: HashMap<String, String> = collection::scope_context(scopes, &visible);

    let plan = http::plan(
        request,
        &PlanContext { root, vars: &vars, scopes, lenient_url: true },
    )?;
    let (hidden, undefined): (Vec<String>, Vec<String>) =
        plan.missing.iter().cloned().partition(|name| secret_names.contains(name.as_str()));

    // Same merge `execute` does, so what is printed is what would be sent.
    Ok(Rendered { command: render(&plan, &options.for_request(request)), hidden, undefined })
}

pub fn render(plan: &Plan, options: &ExecOptions) -> String {
    // A `{{name}}` left in the URL gets percent-encoded by URL parsing; put it back.
    let url = plan.url.replace("%7B%7B", "{{").replace("%7D%7D", "}}");
    let has_type = plan.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-type"));

    let mut first = String::from("curl");
    match (plan.method.as_str(), &plan.body) {
        ("HEAD", _) => first.push_str(" --head"),
        ("GET", PlanBody::None) => {}
        (method, _) => {
            first.push_str(" -X ");
            first.push_str(&quote(method));
        }
    }
    first.push(' ');
    first.push_str(&quote(&url));

    let mut lines = vec![first];
    for (name, value) in &plan.headers {
        lines.push(format!("-H {}", quote(&format!("{name}: {value}"))));
    }
    if let Some((username, password)) = &plan.basic {
        lines.push(format!("-u {}", quote(&format!("{username}:{password}"))));
    }
    if let Some((username, password)) = &plan.digest {
        // curl does the challenge round trip itself with --digest.
        lines.push("--digest".into());
        lines.push(format!("-u {}", quote(&format!("{username}:{password}"))));
    }
    if let Some(credentials) = &plan.aws {
        // Signed here rather than left to curl, so the command runs as it is.
        // A SigV4 signature is good for fifteen minutes, which the note says.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let signed = crate::aws::sign(
            plan,
            &crate::aws::Credentials {
                key_id: &credentials.key_id,
                secret: &credentials.secret,
                region: &credentials.region,
                service: &credentials.service,
                session_token: credentials.session_token.as_deref(),
            },
            now,
        );
        for (name, value) in signed {
            lines.push(format!("-H {}", quote(&format!("{name}: {value}"))));
        }
    }

    match &plan.body {
        PlanBody::None => {}
        PlanBody::Text { content, default_type } => {
            if !has_type {
                lines.push(format!("-H {}", quote(&format!("Content-Type: {default_type}"))));
            }
            lines.push(format!("--data-raw {}", quote(content)));
        }
        PlanBody::UrlEncoded(pairs) => {
            for (name, value) in pairs {
                lines.push(format!("--data-urlencode {}", quote(&format!("{name}={value}"))));
            }
        }
        PlanBody::Multipart(parts) => {
            for part in parts {
                if part.file {
                    lines.push(format!("-F {}", quote(&format!("{}=@{}", part.name, part.value))));
                } else {
                    // --form-string, so a value starting with @ or < is not read as a file.
                    lines.push(format!("--form-string {}", quote(&format!("{}={}", part.name, part.value))));
                }
            }
        }
        PlanBody::File { path } => {
            if !has_type {
                lines.push(format!("-H {}", quote("Content-Type: application/octet-stream")));
            }
            lines.push(format!("--data-binary {}", quote(&format!("@{}", path.to_string_lossy()))));
        }
    }

    if let Some(proxy) = options.proxy_url() {
        lines.push(format!("-x {}", quote(proxy)));
    }
    if let Some(cert) = options.client_cert.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        lines.push(format!("--cert {}", quote(cert)));
    }
    if options.follow_redirects {
        lines.push("-L".into());
    }
    if !options.verify_tls {
        lines.push("-k".into());
    }
    if options.timeout_ms != ExecOptions::default().timeout_ms {
        let seconds = options.timeout_ms as f64 / 1000.0;
        lines.push(format!("--max-time {}", format!("{seconds:.3}").trim_end_matches('0').trim_end_matches('.')));
    }

    lines.join(" \\\n  ")
}

/// Single quotes for bash, unless the word is plainly safe without them.
fn quote(text: &str) -> String {
    let safe = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c));
    if safe {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;

    fn parse_ok(command: &str) -> Parsed {
        parse(command, &[]).unwrap_or_else(|e| panic!("{e}: {command}"))
    }

    fn header<'a>(request: &'a Request, name: &str) -> Option<&'a KeyValue> {
        request.headers.iter().find(|h| h.name.eq_ignore_ascii_case(name))
    }

    fn env(name: &str, value: &str, secret: bool) -> EnvVar {
        EnvVar { name: name.into(), value: value.into(), secret }
    }

    // --- tokenising -----------------------------------------------------------

    #[test]
    fn bash_quoting_and_line_continuations() {
        let tokens = tokenize(
            "curl 'https://x.test/a b' \\\n  -H \"X-Quote: say \\\"hi\\\" \\$HOME\" \\\r\n  --data-raw $'line1\\nline2 \\'q\\' \\u00e9' plain\\ word",
        )
        .unwrap();
        assert_eq!(
            tokens,
            ["curl", "https://x.test/a b", "-H", "X-Quote: say \"hi\" $HOME", "--data-raw", "line1\nline2 'q' é", "plain word"]
        );
        assert!(tokenize("curl 'oops").is_err());
        assert!(tokenize("curl \"oops").is_err());
    }

    #[test]
    fn windows_cmd_copy_splits_the_same_way() {
        // What Chrome's "Copy as cURL (cmd)" produces.
        let cmd = "curl ^\"https://api.shop.test/v2/users?_limit=3^\" ^\n  -H ^\"accept: application/json^\" ^\n  --data-raw ^\"^{^\\^\"name^\\^\":^\\^\"Ada^\\^\"^}^\"";
        let tokens = tokenize(cmd).unwrap();
        assert_eq!(
            tokens,
            ["curl", "https://api.shop.test/v2/users?_limit=3", "-H", "accept: application/json", "--data-raw", "{\"name\":\"Ada\"}"]
        );
    }

    // --- parsing --------------------------------------------------------------

    #[test]
    fn a_plain_get_with_a_query_string() {
        let p = parse_ok("curl https://api.shop.test/v2/users?_limit=3&sort=-created_at -H 'Accept: application/json'");
        let r = &p.request;
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://api.shop.test/v2/users");
        assert_eq!(r.params.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect::<Vec<_>>(), [("_limit", "3"), ("sort", "-created_at")]);
        assert_eq!(header(r, "accept").unwrap().value, "application/json");
        assert!(matches!(r.body, Body::None));
        assert!(matches!(r.auth, Auth::None));
        assert_eq!(r.name, "/v2/users");
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn data_makes_a_post_and_the_content_type_picks_the_body() {
        let json = parse_ok(r#"curl -X POST https://x.test/users -H 'Content-Type: application/json' -d '{"name":"Ada"}'"#);
        assert_eq!(json.request.method, "POST");
        assert!(matches!(&json.request.body, Body::Json { content } if content == r#"{"name":"Ada"}"#));

        let implied = parse_ok(r#"curl https://x.test/users --data '{"name":"Ada"}'"#);
        assert_eq!(implied.request.method, "POST", "data without -X means POST");
        assert!(matches!(implied.request.body, Body::Json { .. }), "JSON-looking data without a type");

        let form = parse_ok("curl https://x.test/login -d 'user=ada&pass=x%20y' -d remember=1");
        let Body::UrlEncoded { fields } = &form.request.body else { panic!("{:?}", form.request.body) };
        assert_eq!(fields.iter().map(|f| (f.name.as_str(), f.value.as_str())).collect::<Vec<_>>(), [("user", "ada"), ("pass", "x y"), ("remember", "1")]);

        let xml = parse_ok("curl https://x.test -H 'content-type: text/xml' --data-binary '<a/>'");
        assert!(matches!(xml.request.body, Body::Xml { .. }));

        let json_flag = parse_ok(r#"curl --json '{"a":1}' https://x.test"#);
        assert!(matches!(json_flag.request.body, Body::Json { .. }));
        assert_eq!(header(&json_flag.request, "accept").unwrap().value, "application/json");
    }

    #[test]
    fn combined_short_flags_attached_values_and_long_equals() {
        let p = parse_ok("curl -sSL -XPUT -H'X-A: 1' --header=X-B:2 --url=https://x.test/items/7 --data-raw=hello");
        assert_eq!(p.request.method, "PUT");
        assert_eq!(p.request.url, "https://x.test/items/7");
        assert_eq!(header(&p.request, "x-a").unwrap().value, "1");
        assert_eq!(header(&p.request, "x-b").unwrap().value, "2");
        assert!(matches!(&p.request.body, Body::Text { content } if content == "hello"));
        // Plain text under curl's default form type keeps that type.
        assert_eq!(header(&p.request, "content-type").unwrap().value, "application/x-www-form-urlencoded");
    }

    #[test]
    fn get_flag_moves_data_into_the_query() {
        let p = parse_ok("curl -G https://x.test/search -d q=volt --data-urlencode 'tag=a b'");
        assert_eq!(p.request.method, "GET");
        assert!(matches!(p.request.body, Body::None));
        let pairs: Vec<_> = p.request.params.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect();
        assert_eq!(pairs, [("q", "volt"), ("tag", "a b")]);
    }

    #[test]
    fn multipart_keeps_text_fields_and_files() {
        let p = parse_ok("curl https://x.test/avatar -H 'Content-Type: multipart/form-data' -F caption=me -F file=@me.png -F bio=<bio.txt");
        let Body::Form { fields } = &p.request.body else { panic!() };
        assert_eq!(fields.len(), 2);
        assert_eq!((fields[0].name.as_str(), fields[0].value.as_str(), fields[0].file), ("caption", "me", false));
        assert_eq!((fields[1].name.as_str(), fields[1].value.as_str(), fields[1].file), ("file", "me.png", true));
        assert!(header(&p.request, "content-type").is_none(), "the boundary is generated when sending");
        // `<` reads the *value* from a file, which is a different thing.
        assert!(p.notes.iter().any(|n| n.contains("-F bio=<bio.txt")), "{:?}", p.notes);
    }

    #[test]
    fn auth_in_every_form_becomes_auth_and_its_secret_moves_out() {
        let basic = parse_ok("curl -u ada:hunter2 https://x.test");
        assert!(matches!(&basic.request.auth, Auth::Basic { username, password } if username == "ada" && password == "{{password}}"));
        assert_eq!(basic.new_secrets, vec![env("password", "hunter2", true)]);

        let bearer = parse_ok("curl https://x.test -H 'Authorization: Bearer eyJ.abc'");
        assert!(matches!(&bearer.request.auth, Auth::Bearer { token } if token == "{{token}}"));
        assert!(header(&bearer.request, "authorization").is_none());
        assert_eq!(bearer.new_secrets[0].value, "eyJ.abc");

        // Basic in a header is decoded, not left as base64.
        let encoded = base64::engine::general_purpose::STANDARD.encode("grace:cobol");
        let header_basic = parse_ok(&format!("curl https://x.test -H 'authorization: Basic {encoded}'"));
        assert!(matches!(&header_basic.request.auth, Auth::Basic { username, .. } if username == "grace"));

        let api_key = parse_ok("curl https://x.test -H 'X-API-Key: sk_live_1'");
        assert_eq!(header(&api_key.request, "x-api-key").unwrap().value, "{{apiKey}}");
    }

    #[test]
    fn new_secret_names_never_clash_and_existing_values_are_reused() {
        let existing = [env("token", "old-token", true), env("baseUrl", "https://x.test", false)];

        let fresh = parse("curl https://x.test -H 'Authorization: Bearer new-token'", &existing).unwrap();
        assert!(matches!(&fresh.request.auth, Auth::Bearer { token } if token == "{{token2}}"));
        assert_eq!(fresh.new_secrets, vec![env("token2", "new-token", true)]);

        let same = parse("curl https://x.test -H 'Authorization: Bearer old-token'", &existing).unwrap();
        assert!(matches!(&same.request.auth, Auth::Bearer { token } if token == "{{token}}"));
        assert!(same.new_secrets.is_empty(), "a value already held is reused, not duplicated");
    }

    #[test]
    fn a_browser_copy_is_cleaned_up_without_losing_anything() {
        let command = r#"curl 'https://shop.test/api/cart' \
  -H 'accept: */*' \
  -H 'accept-language: tr-TR,tr;q=0.9' \
  -H 'content-type: application/json' \
  -b 'session=s3ss10n; theme=dark' \
  -H 'origin: https://shop.test' \
  -H 'priority: u=1, i' \
  -H 'sec-ch-ua-platform: "Windows"' \
  -H 'sec-fetch-mode: cors' \
  -H 'accept-encoding: gzip, deflate, br, zstd' \
  -H 'content-length: 18' \
  -H 'user-agent: Mozilla/5.0' \
  --data-raw $'{"sku":"A-1","qty":2}' \
  --compressed"#;
        let p = parse_ok(command);
        let r = &p.request;
        assert_eq!(r.method, "POST");
        assert!(matches!(r.body, Body::Json { .. }));

        assert!(!header(r, "sec-fetch-mode").unwrap().enabled, "browser-only headers are off");
        assert!(!header(r, "priority").unwrap().enabled);
        assert!(header(r, "origin").unwrap().enabled);
        assert!(header(r, "content-length").is_none() && header(r, "accept-encoding").is_none());

        assert_eq!(header(r, "cookie").unwrap().value, "{{cookie}}", "a session cookie is a credential");
        assert_eq!(p.new_secrets, vec![env("cookie", "session=s3ss10n; theme=dark", true)]);

        assert!(p.notes.iter().any(|n| n.contains("switched off: priority, sec-ch-ua-platform, sec-fetch-mode")), "{:?}", p.notes);
        assert!(p.notes.iter().any(|n| n.contains("Accept-Encoding") || n.contains("accept-encoding")));
        assert!(!p.notes.iter().any(|n| n.contains("compressed")), "output-only flags are not noise");
    }

    #[test]
    fn what_cannot_be_applied_is_said_and_values_are_not_mistaken_for_the_url() {
        let p = parse_ok("curl --proxy http://proxy:8080 -k --max-time 5 --digest -o out.json -w '%{http_code}' https://x.test/a -d @body.json --frobnicate");
        assert_eq!(p.request.url, "https://x.test/a", "option values were consumed");

        // These three are no longer notes-only: a request carries them now.
        let options = p.request.options.clone().expect("send options were taken from the command");
        assert_eq!(options.timeout_ms, Some(5_000));
        assert_eq!(options.verify_tls, Some(false));
        assert_eq!(options.proxy.as_deref(), Some("http://proxy:8080"));

        let notes = p.notes.join("\n");
        assert!(notes.contains("-k") && notes.contains("Options tab"), "{notes}");
        assert!(notes.contains("http://proxy:8080"), "{notes}");
        assert!(!notes.contains("--digest"), "digest is applied now, not reported: {notes}");
        assert!(notes.contains("-d @body.json"), "{notes}");
        assert!(notes.contains("--frobnicate"), "{notes}");
        assert!(!notes.contains("-o"), "output options are ignored quietly: {notes}");
        assert!(!notes.contains("Not applied"), "nothing is filed as unapplied any more: {notes}");
    }

    #[test]
    fn digest_and_aws_auth_come_out_the_way_each_one_travels() {
        let mut r = request();
        r.url = "https://api.test/orders".into();
        r.body = Body::None;
        r.auth = Auth::Digest { username: "ada".into(), password: "pw".into() };
        let out = export_with(&r, true, &ExecOptions::default()).command;
        assert!(out.contains("--digest") && out.contains("-u ada:pw"), "{out}");

        r.auth = Auth::AwsSigV4 {
            key_id: "AKIDEXAMPLE".into(),
            secret: "secret".into(),
            region: "eu-west-1".into(),
            service: "execute-api".into(),
            session_token: String::new(),
        };
        let out = export_with(&r, true, &ExecOptions::default()).command;
        assert!(out.contains("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"), "{out}");
        assert!(out.contains("eu-west-1/execute-api/aws4_request"), "{out}");
        assert!(out.contains("x-amz-date:"), "{out}");
    }

    #[test]
    fn send_options_survive_export_then_parse() {
        let mut r = request();
        r.options = Some(RequestOptions {
            timeout_ms: Some(2_500),
            verify_tls: Some(false),
            proxy: Some("http://proxy:8080".into()),
            client_cert: Some("certs/client.pem".into()),
            follow_redirects: None,
        });
        let command = export_with(&r, true, &ExecOptions::default()).command;
        assert!(command.contains("-x http://proxy:8080") && command.contains("--cert certs/client.pem"), "{command}");
        assert!(command.contains("-k") && command.contains("--max-time 2.5"), "{command}");

        let back = parse_ok(&command).request.options.expect("they come back");
        assert_eq!(back, r.options.unwrap());
    }

    #[test]
    fn a_json_body_with_a_password_is_flagged() {
        let p = parse_ok(r#"curl https://x.test/login -H 'content-type: application/json' -d '{"email":"a@b.c","password":"hunter2"}'"#);
        assert!(p.notes.iter().any(|n| n.contains("plain-text credential")), "{:?}", p.notes);
    }

    #[test]
    fn edge_inputs() {
        assert_eq!(parse_ok("$ curl example.com/x").request.url, "http://example.com/x", "prompt and scheme");
        assert_eq!(parse_ok("curl.exe https://x.test").request.url, "https://x.test");
        assert_eq!(parse_ok("curl -I https://x.test").request.method, "HEAD");
        assert_eq!(parse_ok("curl {{baseUrl}}/users").request.url, "{{baseUrl}}/users");
        assert!(parse("wget https://x.test", &[]).unwrap_err().to_string().contains("not a curl"));
        assert!(parse("curl -s", &[]).unwrap_err().to_string().contains("no URL"));
        assert!(parse("curl https://x.test -H", &[]).unwrap_err().to_string().contains("missing its value"));
        assert!(is_curl("  curl -X GET x") && is_curl("/usr/bin/curl x") && !is_curl("curling"));
    }

    // --- rendering --------------------------------------------------------------
    fn text_part(name: &str, value: &str) -> FormField {
        FormField { name: name.into(), value: value.into(), enabled: true, description: None, file: false }
    }


    fn request() -> Request {
        Request {
            name: "Create user".into(),
            kind: crate::model::Kind::Http,
            seq: 1,
            method: "POST".into(),
            url: "{{baseUrl}}/users".into(),
            params: vec![field("dry_run", "1")],
            headers: vec![field("Accept", "application/json"), KeyValue { enabled: false, ..field("X-Off", "1") }],
            body: Body::Json { content: "{\"name\": \"O'Brien\"}".into() },
            auth: Auth::Bearer { token: "{{token}}".into() },
            captures: Vec::new(),
            checks: Vec::new(),
            options: None,
            docs: None,
        }
    }

    fn vars() -> Vec<EnvVar> {
        vec![env("baseUrl", "https://api.shop.test/v2", false), env("token", "eyJ.secret", true)]
    }

    fn export_with(request: &Request, include_secrets: bool, options: &ExecOptions) -> Rendered {
        export(std::path::Path::new("."), request, &vars(), &Default::default(), options, include_secrets).unwrap()
    }

    #[test]
    fn export_matches_what_send_resolves_and_hides_secrets_by_default() {
        let out = export_with(&request(), false, &ExecOptions::default());
        assert_eq!(
            out.command,
            "curl -X POST 'https://api.shop.test/v2/users?dry_run=1' \\\n  \
             -H 'Accept: application/json' \\\n  \
             -H 'authorization: Bearer {{token}}' \\\n  \
             -H 'Content-Type: application/json' \\\n  \
             --data-raw '{\"name\": \"O'\\''Brien\"}' \\\n  \
             -L"
        );
        assert_eq!(out.hidden, ["token"]);
        assert!(out.undefined.is_empty());

        let with = export_with(&request(), true, &ExecOptions::default());
        assert!(with.command.contains("Bearer eyJ.secret") && with.hidden.is_empty());
    }

    #[test]
    fn export_keeps_placeholders_readable_even_inside_a_parsed_url() {
        let mut r = request();
        r.url = "https://api.shop.test/users/{{userId}}".into();
        r.params = vec![field("key", "{{token}}")];
        let out = export_with(&r, false, &ExecOptions::default());
        assert!(out.command.contains("'https://api.shop.test/users/{{userId}}?key={{token}}'"), "{}", out.command);
        assert_eq!(out.undefined, ["userId"]);
    }

    #[test]
    fn export_follows_the_app_settings_and_the_body_kind() {
        let options =
            ExecOptions { timeout_ms: 2500, follow_redirects: false, verify_tls: false, ..Default::default() };
        let mut r = request();
        r.method = "GET".into();
        r.body = Body::None;
        r.auth = Auth::Basic { username: "ada".into(), password: "pw".into() };
        let out = export_with(&r, true, &options).command;
        assert!(out.starts_with("curl 'https://"), "GET without a body needs no -X: {out}");
        assert!(out.contains("-u ada:pw") && out.contains("-k") && out.contains("--max-time 2.5") && !out.contains("-L"), "{out}");

        r.method = "HEAD".into();
        assert!(export_with(&r, true, &options).command.starts_with("curl --head "));

        r.method = "POST".into();
        r.body = Body::Form { fields: vec![text_part("avatar", "@not-a-file")] };
        assert!(export_with(&r, true, &options).command.contains("--form-string avatar=@not-a-file"));

        // A real file part is the one case that does use -F name=@path.
        r.body = Body::Form {
            fields: vec![FormField { name: "avatar".into(), value: "pics/a.png".into(), enabled: true, file: true, description: None }],
        };
        let out = export_with(&r, true, &options).command;
        assert!(out.contains("-F avatar=@pics/a.png") && !out.contains("--form-string"), "{out}");

        let back = parse_ok(&out).request.body;
        match back {
            Body::Form { fields } => {
                assert_eq!(fields.len(), 1);
                assert!(fields[0].file && fields[0].value == "pics/a.png", "{:?}", fields[0]);
            }
            other => panic!("a file field should come back as one: {other:?}"),
        }

        r.body = Body::UrlEncoded { fields: vec![field("q", "a b&c")] };
        assert!(export_with(&r, true, &options).command.contains("--data-urlencode 'q=a b&c'"));
    }

    #[test]
    fn a_request_survives_export_then_parse() {
        // Bearer: exported with its value, parsed back into auth and a secret.
        let rendered = export_with(&request(), true, &ExecOptions::default());
        let back = parse(&rendered.command, &[]).unwrap();
        let r = back.request;
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.shop.test/v2/users");
        assert_eq!(r.params[0].value, "1");
        assert!(matches!(&r.body, Body::Json { content } if content == "{\"name\": \"O'Brien\"}"));
        assert_eq!(header(&r, "accept").unwrap().value, "application/json");
        assert!(header(&r, "x-off").is_none(), "disabled headers are not exported");
        assert!(matches!(&r.auth, Auth::Bearer { token } if token == "{{token}}"), "{:?}", r.auth);
        assert_eq!(back.new_secrets, vec![env("token", "eyJ.secret", true)]);

        // An API key in a header comes back as that header, its value a secret.
        // (curl has no notion of "API key auth", so it returns as a plain header;
        // the secret is recognised by the header's name.)
        let mut with_key = request();
        with_key.auth = Auth::ApiKey { key: "X-API-Key".into(), value: "k1".into(), location: ApiKeyLocation::Header };
        let back = parse(&export_with(&with_key, true, &ExecOptions::default()).command, &[]).unwrap();
        assert!(matches!(back.request.auth, Auth::None));
        assert_eq!(header(&back.request, "x-api-key").unwrap().value, "{{apiKey}}");
    }
}

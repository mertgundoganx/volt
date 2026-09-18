//! Documentation, as one file.
//!
//! Postman publishes docs to a page it hosts. volt writes an HTML file: it
//! opens in any browser, goes in the repository next to the collection, and is
//! reviewed in a pull request like everything else here. Nothing to sign in
//! to, nothing that goes stale because someone forgot to press publish.
//!
//! Saved examples carry it: a documented endpoint is one with a real response
//! beside it.

use std::fmt::Write as _;
use std::path::Path;

use crate::collection::{self, Collection, Node};
use crate::error::Result;
use crate::examples;
use crate::model::{ApiKeyLocation, Auth, Body, KeyValue, Request};

/// Escape for HTML text and attribute values. Everything written into the page
/// goes through this: a collection is the user's own, but a response body kept
/// as an example is whatever a server sent.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

pub fn html(root: &Path, collection: &Collection) -> Result<String> {
    let mut body = String::new();
    let mut contents = String::new();

    section(root, &collection.tree, 0, &mut body, &mut contents)?;

    let meta = &collection.meta;
    let mut page = String::new();
    let _ = write!(
        page,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{name}</title>\n<style>{css}</style>\n</head>\n<body>\n\
         <header><p class=\"eyebrow\">API documentation</p><h1>{name}</h1></header>\n",
        name = escape(&meta.name),
        css = CSS,
    );

    if !meta.vars.is_empty() {
        let _ = writeln!(page, "<section class=\"vars\"><h2>Variables</h2>{}</section>", table(&meta.vars));
    }
    if !meta.headers.is_empty() {
        let _ = writeln!(
            page,
            "<section class=\"vars\"><h2>Sent with every request</h2>{}</section>",
            table(&meta.headers)
        );
    }

    let _ = writeln!(page, "<nav aria-label=\"Contents\"><h2>Contents</h2><ul>{contents}</ul></nav>");
    page.push_str(&body);
    let _ = write!(
        page,
        "<footer>Written by volt from the collection's own files. Secret values are never included.</footer>\n</body>\n</html>\n"
    );
    Ok(page)
}

fn section(
    root: &Path,
    nodes: &[Node],
    depth: usize,
    body: &mut String,
    contents: &mut String,
) -> Result<()> {
    for node in nodes {
        match node {
            Node::Folder { id, name, children, .. } => {
                let level = (depth + 2).min(4);
                let _ = writeln!(
                    body,
                    "<h{level} id=\"{id}\" class=\"folder\">{name}</h{level}>",
                    id = escape(id),
                    name = escape(name)
                );
                let _ = write!(
                    contents,
                    "<li class=\"folder\"><a href=\"#{id}\">{name}</a></li>",
                    id = escape(id),
                    name = escape(name)
                );
                section(root, children, depth + 1, body, contents)?;
            }
            Node::Request { id, name, method, .. } => {
                let request = collection::read_request(root, id)?;
                let _ = write!(
                    contents,
                    "<li><a href=\"#{id}\"><span class=\"method {lower}\">{method}</span>{name}</a></li>",
                    id = escape(id),
                    lower = escape(&method.to_lowercase()),
                    method = escape(method),
                    name = escape(name)
                );
                body.push_str(&request_section(root, id, name, &request)?);
            }
        }
    }
    Ok(())
}

fn request_section(root: &Path, id: &str, name: &str, request: &Request) -> Result<String> {
    let mut out = String::new();
    let method = request.method.to_uppercase();

    let _ = write!(
        out,
        "<article id=\"{id}\" class=\"request\">\n<h3><span class=\"method {lower}\">{method}</span>{name}</h3>\n\
         <p class=\"url\"><code>{url}</code></p>\n<p class=\"file\">{id}</p>\n",
        id = escape(id),
        lower = escape(&method.to_lowercase()),
        method = escape(&method),
        name = escape(name),
        url = escape(request.url.trim()),
    );

    if let Some(docs) = request.docs.as_deref().filter(|d| !d.trim().is_empty()) {
        let _ = writeln!(out, "<div class=\"notes\">{}</div>", paragraphs(docs));
    }

    let enabled: Vec<KeyValue> = request.params.iter().filter(|p| p.enabled).cloned().collect();
    if !enabled.is_empty() {
        let _ = write!(out, "<h4>Query</h4>{}", table(&enabled));
    }
    // A header that carries a credential is masked the same way the auth block
    // is: this file is meant to be shared.
    let headers: Vec<KeyValue> = request
        .headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| {
            let credential = ["authorization", "proxy-authorization", "cookie"]
                .iter()
                .any(|name| h.name.eq_ignore_ascii_case(name));
            match credential {
                true => KeyValue { value: masked(&h.value), ..h.clone() },
                false => h.clone(),
            }
        })
        .collect();
    if !headers.is_empty() {
        let _ = write!(out, "<h4>Headers</h4>{}", table(&headers));
    }

    if let Some(auth) = describe_auth(&request.auth) {
        let _ = writeln!(out, "<h4>Auth</h4><p>{auth}</p>");
    }
    if let Some(body) = describe_body(&request.body) {
        let _ = write!(out, "<h4>Body</h4>{body}");
    }

    let kept = examples::list(root, id)?;
    for example in kept {
        let tone = match example.status {
            200..=299 => "ok",
            300..=399 => "warn",
            _ => "bad",
        };
        let _ = writeln!(
            out,
            "<h4 class=\"example\"><span class=\"status {tone}\">{status}</span>{name}</h4>",
            status = example.status,
            name = escape(&example.name),
        );
        if example.body_is_base64 {
            let _ = writeln!(out, "<p class=\"binary\">A binary response, not shown.</p>");
        } else if !example.body.trim().is_empty() {
            let _ = writeln!(out, "<pre><code>{}</code></pre>", escape(&example.body));
        }
    }

    out.push_str("</article>\n");
    Ok(out)
}

fn table(rows: &[KeyValue]) -> String {
    let mut out = String::from("<table><tbody>");
    for row in rows {
        let _ = write!(
            out,
            "<tr><th scope=\"row\"><code>{name}</code></th><td><code>{value}</code></td><td class=\"about\">{about}</td></tr>",
            name = escape(&row.name),
            value = escape(&row.value),
            about = escape(row.description.as_deref().unwrap_or("")),
        );
    }
    out.push_str("</tbody></table>\n");
    out
}

/// Blank-line separated text into paragraphs. Not Markdown: guessing at
/// formatting is how a doc generator starts lying about what was written.
fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| format!("<p>{}</p>", escape(part).replace('\n', "<br>")))
        .collect()
}

/// A credential as the page may show it.
///
/// A `{{variable}}` documents itself and is safe: the value lives in a
/// gitignored file, not here. Anything else was typed straight into the field,
/// and this page is written to be emailed around — the same rule "Copy as
/// cURL" follows.
fn masked(value: &str) -> String {
    let placeholder_only = value.contains("{{")
        && value
            .split('{')
            .flat_map(|part| part.split('}'))
            .all(|part| part.chars().all(|c| c.is_alphanumeric() || " _-.:/".contains(c)));
    if placeholder_only { value.to_string() } else { "…".to_string() }
}

fn describe_auth(auth: &Auth) -> Option<String> {
    match auth {
        Auth::Inherit => Some("Whatever the folder or collection says.".into()),
        Auth::None => None,
        Auth::Bearer { token } => Some(format!("Bearer token: <code>{}</code>", escape(&masked(token)))),
        Auth::Basic { username, .. } => Some(format!("Basic, as <code>{}</code>.", escape(username))),
        Auth::Digest { username, .. } => Some(format!("Digest, as <code>{}</code>.", escape(username))),
        Auth::Ntlm { username, domain, .. } => Some(format!(
            "NTLM, as <code>{}{}</code>.",
            if domain.is_empty() { String::new() } else { format!("{}\\", escape(domain)) },
            escape(username)
        )),
        Auth::AwsSigV4 { region, service, .. } => Some(format!(
            "AWS SigV4, signed for <code>{}</code> in <code>{}</code>.",
            escape(service),
            escape(region)
        )),
        Auth::ApiKey { key, location, .. } => Some(format!(
            "API key <code>{}</code>, sent in the {}.",
            escape(key),
            match location {
                ApiKeyLocation::Header => "header",
                ApiKeyLocation::Query => "query string",
            }
        )),
    }
}

fn describe_body(body: &Body) -> Option<String> {
    match body {
        Body::None => None,
        Body::Text { content } | Body::Json { content } | Body::Xml { content } => {
            Some(format!("<pre><code>{}</code></pre>\n", escape(content)))
        }
        Body::UrlEncoded { fields } => {
            let rows: Vec<KeyValue> = fields.iter().filter(|f| f.enabled).cloned().collect();
            Some(format!("<p>Form encoded:</p>{}", table(&rows)))
        }
        Body::Form { fields } => {
            let rows: Vec<KeyValue> = fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| KeyValue {
                    name: f.name.clone(),
                    value: if f.file { format!("{} (a file)", f.value) } else { f.value.clone() },
                    enabled: true,
                    description: f.description.clone(),
                })
                .collect();
            Some(format!("<p>Multipart:</p>{}", table(&rows)))
        }
        Body::Grpc { proto, method, message } => Some(format!(
            "<p>gRPC <code>{}</code>, from <code>{}</code>.</p><pre><code>{}</code></pre>\n",
            escape(method),
            escape(proto),
            escape(message)
        )),
        Body::GraphQl { query, variables } => {
            let mut out = format!("<pre><code>{}</code></pre>\n", escape(query));
            if !variables.trim().is_empty() {
                out.push_str(&format!("<p>Variables:</p><pre><code>{}</code></pre>\n", escape(variables)));
            }
            Some(out)
        }
        Body::Binary { path } => Some(format!("<p>The file <code>{}</code>.</p>\n", escape(path))),
    }
}

/// Deliberately plain and self-contained: the file has to open from a network
/// share or an email attachment, offline, with no fonts to fetch.
const CSS: &str = "
:root {
  --ink: #16181d; --silk: #6b7280; --rule: #e2e4e9; --well: #f6f7f9;
  --ok: #1f7a4d; --warn: #9a6b00; --bad: #b4232a;
  color-scheme: light dark;
}
@media (prefers-color-scheme: dark) {
  :root { --ink: #e8eaee; --silk: #9aa1ad; --rule: #2a2e37; --well: #1a1d23; }
  body { background: #101216; }
}
* { box-sizing: border-box; }
body {
  margin: 0 auto; max-width: 62rem; padding: 3rem 1.25rem 5rem;
  font: 15px/1.6 ui-sans-serif, system-ui, sans-serif; color: var(--ink);
}
code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
h1 { font-size: 1.9rem; margin: 0 0 2rem; }
h2 { font-size: 1.15rem; margin: 2.5rem 0 0.75rem; }
h3 { display: flex; align-items: center; gap: 0.6rem; font-size: 1.05rem; margin: 0 0 0.35rem; }
h4 { font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--silk); margin: 1.25rem 0 0.4rem; }
.eyebrow { text-transform: uppercase; letter-spacing: 0.08em; font-size: 0.72rem; color: var(--silk); margin: 0 0 0.35rem; }
nav ul { list-style: none; padding: 0; margin: 0; columns: 2; }
nav li { margin: 0 0 0.3rem; break-inside: avoid; }
nav li.folder { margin-top: 0.8rem; font-weight: 600; }
nav a { display: flex; gap: 0.5rem; align-items: center; color: inherit; text-decoration: none; }
nav a:hover { text-decoration: underline; }
.method {
  display: inline-block; min-width: 3.4rem; padding: 0.1rem 0.35rem; border-radius: 4px;
  background: var(--well); font-family: ui-monospace, monospace; font-size: 0.7rem;
  letter-spacing: 0.04em; text-align: center; color: var(--silk);
}
.method.get { color: #1f6feb; } .method.post { color: var(--ok); }
.method.put, .method.patch { color: var(--warn); } .method.delete { color: var(--bad); }
.request { padding: 1.5rem 0; border-top: 1px solid var(--rule); }
.folder { margin-top: 2.5rem; }
.url code { font-size: 14px; }
.url { margin: 0 0 0.2rem; }
.file { margin: 0; color: var(--silk); font-size: 0.78rem; font-family: ui-monospace, monospace; }
.notes p { margin: 0.6rem 0; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: 0.3rem 0.6rem 0.3rem 0; border-bottom: 1px solid var(--rule); font-weight: 400; vertical-align: top; }
th { width: 12rem; }
.about { color: var(--silk); }
pre { padding: 0.75rem 0.9rem; border-radius: 6px; background: var(--well); overflow-x: auto; }
.status { display: inline-block; margin-right: 0.5rem; font-family: ui-monospace, monospace; }
.status.ok { color: var(--ok); } .status.warn { color: var(--warn); } .status.bad { color: var(--bad); }
.binary { color: var(--silk); }
footer { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--rule); color: var(--silk); font-size: 0.8rem; }
";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sample-collection")
    }

    #[test]
    fn the_page_documents_every_request_and_stands_on_its_own() {
        let root = sample();
        let page = html(&root, &collection::load(&root).unwrap()).unwrap();

        assert!(page.starts_with("<!doctype html>"));
        assert!(page.contains("<title>Sample API</title>"));
        assert!(page.contains("List users"), "every request is in it");
        assert!(page.contains("{{baseUrl}}/users"), "with the URL as it is written");
        assert!(page.contains("id=\"users/list-users.yaml\""), "anchored by its file");
        assert!(!page.contains("<link"), "nothing to fetch: it has to open offline");
        assert!(!page.contains("<script"), "and nothing to run");
    }

    #[test]
    fn a_response_body_cannot_write_html_into_the_page() {
        let root = std::env::temp_dir().join(format!("volt-docs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("collection.yaml"), "name: Escapes\nversion: 1\n").unwrap();
        std::fs::write(
            root.join("x.yaml"),
            "name: Nasty\nseq: 1\nmethod: GET\nurl: 'https://x.test/<img src=x>'\ndocs: \"a & b < c\"\n",
        )
        .unwrap();

        let page = html(&root, &collection::load(&root).unwrap()).unwrap();
        assert!(!page.contains("<img src=x>"), "the URL was escaped");
        assert!(page.contains("&lt;img src=x&gt;"));
        assert!(page.contains("a &amp; b &lt; c"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn notes_keep_their_paragraphs_without_pretending_to_be_markdown() {
        let out = paragraphs("First line.\nSame paragraph.\n\nSecond one.");
        assert_eq!(out, "<p>First line.<br>Same paragraph.</p><p>Second one.</p>");
    }

    /// The page promises at the bottom that it carries no secret values, so
    /// that has to hold for a credential typed straight into the field too:
    /// that one is in no environment, so nothing else would catch it.
    #[test]
    fn a_literal_credential_never_reaches_the_page() {
        let root = std::env::temp_dir().join(format!("volt-docs-secret-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("collection.yaml"), "name: Shop\nversion: 1\n").unwrap();
        std::fs::write(
            root.join("me.yaml"),
            concat!(
                "name: Me\nseq: 1\nmethod: GET\nurl: '{{baseUrl}}/me'\n",
                "headers:\n",
                "  - name: Authorization\n    value: Bearer abc123liveheader\n    enabled: true\n",
                "  - name: Accept\n    value: application/json\n    enabled: true\n",
                "auth:\n  type: bearer\n  token: ghp_liveTokenPastedIn\n",
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("named.yaml"),
            "name: Named\nseq: 2\nmethod: GET\nurl: '{{baseUrl}}/me'\nauth:\n  type: bearer\n  token: '{{token}}'\n",
        )
        .unwrap();

        let page = html(&root, &collection::load(&root).unwrap()).unwrap();
        assert!(!page.contains("ghp_liveTokenPastedIn"), "the pasted bearer token is gone:\n{page}");
        assert!(!page.contains("abc123liveheader"), "and so is the one in a header");
        assert!(page.contains("application/json"), "an ordinary header is untouched");
        assert!(page.contains("{{token}}"), "a variable reference documents itself and stays");

        std::fs::remove_dir_all(&root).ok();
    }
}

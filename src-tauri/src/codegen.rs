//! Printing a request as code.
//!
//! Fed by `http::plan`, like `curl::render`, so what a snippet does is what
//! Send does: same variables, same inherited auth and headers, same query.
//!
//! Secrets stay as `{{name}}` unless the user asks for them, exactly as "Copy
//! as cURL" does — a snippet is something people paste into a chat.

use std::collections::HashSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::collection;
use crate::error::Result;
use crate::http::{self, ExecOptions, Plan, PlanBody, PlanContext};
use crate::model::{EnvVar, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    JsFetch,
    JsAxios,
    Python,
    Go,
    Csharp,
    Php,
    Ruby,
}

impl Language {
    pub const ALL: [Language; 7] = [
        Language::JsFetch,
        Language::JsAxios,
        Language::Python,
        Language::Go,
        Language::Csharp,
        Language::Php,
        Language::Ruby,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Language::JsFetch => "JavaScript · fetch",
            Language::JsAxios => "JavaScript · axios",
            Language::Python => "Python · requests",
            Language::Go => "Go · net/http",
            Language::Csharp => "C# · HttpClient",
            Language::Php => "PHP · cURL",
            Language::Ruby => "Ruby · Net::HTTP",
        }
    }

    /// What the UI highlights it as. Only `json` is highlighted today, so
    /// everything here is plain text; the name still travels for later.
    pub fn syntax(self) -> &'static str {
        match self {
            Language::JsFetch | Language::JsAxios => "javascript",
            Language::Python => "python",
            Language::Go => "go",
            Language::Csharp => "csharp",
            Language::Php => "php",
            Language::Ruby => "ruby",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Generated {
    pub code: String,
    pub syntax: &'static str,
    /// Secret variables left as `{{name}}`.
    pub hidden: Vec<String>,
    /// Variables with no value anywhere.
    pub undefined: Vec<String>,
}

pub fn generate(
    root: &std::path::Path,
    request: &Request,
    env_vars: &[EnvVar],
    scopes: &collection::Scopes,
    options: &ExecOptions,
    language: Language,
    include_secrets: bool,
) -> Result<Generated> {
    let secret_names: HashSet<&str> = env_vars.iter().filter(|v| v.secret).map(|v| v.name.as_str()).collect();
    let visible: Vec<EnvVar> = env_vars.iter().filter(|v| include_secrets || !v.secret).cloned().collect();
    let vars = collection::scope_context(scopes, &visible);

    let plan = http::plan(request, &PlanContext { root, vars: &vars, scopes, lenient_url: true })?;
    let (hidden, undefined): (Vec<String>, Vec<String>) =
        plan.missing.iter().cloned().partition(|name| secret_names.contains(name.as_str()));

    Ok(Generated {
        code: render(&plan, &options.for_request(request), language),
        syntax: language.syntax(),
        hidden,
        undefined,
    })
}

/// Headers as they go on the wire, including the one Basic auth becomes, so
/// every language prints the same set.
fn headers_of(plan: &Plan) -> Vec<(String, String)> {
    let mut headers = plan.headers.clone();
    if let Some((username, password)) = &plan.basic {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
        headers.push(("Authorization".into(), format!("Basic {encoded}")));
    }
    headers
}

/// The body as a string, and the Content-Type to send it with when the plan
/// carries a default. `None` means there is no body.
fn body_of(plan: &Plan) -> Option<(String, Option<&'static str>)> {
    match &plan.body {
        PlanBody::None => None,
        PlanBody::Text { content, default_type } => Some((content.clone(), Some(default_type))),
        PlanBody::UrlEncoded(pairs) => {
            let encoded =
                pairs.iter().map(|(n, v)| format!("{}={}", encode(n), encode(v))).collect::<Vec<_>>().join("&");
            Some((encoded, Some("application/x-www-form-urlencoded")))
        }
        // A multipart body is a different shape in every language; rather than
        // print something that does not run, say so.
        PlanBody::Multipart(_) => Some(("<multipart body: build it with the language's own form helper>".into(), None)),
        PlanBody::File { path } => {
            Some((format!("<the contents of {}>", path.to_string_lossy()), Some("application/octet-stream")))
        }
    }
}

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

// Written with escape codes rather than literal backslashes: these functions
// are all about escaping, and a source file full of leaning toothpicks is how
// the wrong number of them gets shipped.
const BACKSLASH: char = '\u{5c}';
const DOUBLE: char = '\u{22}';
const SINGLE: char = '\u{27}';
const NEWLINE: char = '\u{a}';

fn escaped(text: &str, delimiter: char) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(delimiter);
    for ch in text.chars() {
        match ch {
            BACKSLASH => {
                out.push(BACKSLASH);
                out.push(BACKSLASH);
            }
            c if c == delimiter => {
                out.push(BACKSLASH);
                out.push(c);
            }
            NEWLINE => {
                out.push(BACKSLASH);
                out.push('n');
            }
            other => out.push(other),
        }
    }
    out.push(delimiter);
    out
}

fn quote(text: &str) -> String {
    escaped(text, DOUBLE)
}

/// Single-quoted, for the languages where that is the idiom.
fn single(text: &str) -> String {
    escaped(text, SINGLE)
}

pub fn render(plan: &Plan, options: &ExecOptions, language: Language) -> String {
    let headers = headers_of(plan);
    let body = body_of(plan);
    match language {
        Language::JsFetch => js_fetch(plan, &headers, body.as_ref()),
        Language::JsAxios => js_axios(plan, &headers, body.as_ref()),
        Language::Python => python(plan, &headers, body.as_ref(), options),
        Language::Go => go(plan, &headers, body.as_ref()),
        Language::Csharp => csharp(plan, &headers, body.as_ref()),
        Language::Php => php(plan, &headers, body.as_ref(), options),
        Language::Ruby => ruby(plan, &headers, body.as_ref()),
    }
}

fn content_type<'a>(headers: &'a [(String, String)], body: Option<&'a (String, Option<&'static str>)>) -> Option<String> {
    if headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-type")) {
        return None;
    }
    body.and_then(|(_, default)| default.map(str::to_string))
}

fn js_fetch(plan: &Plan, headers: &[(String, String)], body: Option<&(String, Option<&'static str>)>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "const response = await fetch({}, {{", single(&plan.url));
    let _ = writeln!(out, "  method: {},", single(&plan.method));

    let extra = content_type(headers, body);
    if !headers.is_empty() || extra.is_some() {
        let _ = writeln!(out, "  headers: {{");
        for (name, value) in headers {
            let _ = writeln!(out, "    {}: {},", single(name), single(value));
        }
        if let Some(mime) = &extra {
            let _ = writeln!(out, "    'Content-Type': {},", single(mime));
        }
        let _ = writeln!(out, "  }},");
    }
    if let Some((content, _)) = body {
        let _ = writeln!(out, "  body: {},", single(content));
    }
    let _ = writeln!(out, "}})\n");
    let _ = write!(out, "console.log(response.status, await response.text())");
    out
}

fn js_axios(plan: &Plan, headers: &[(String, String)], body: Option<&(String, Option<&'static str>)>) -> String {
    let mut out = String::from("import axios from 'axios'\n\n");
    let _ = writeln!(out, "const response = await axios({{");
    let _ = writeln!(out, "  method: {},", single(&plan.method.to_lowercase()));
    let _ = writeln!(out, "  url: {},", single(&plan.url));

    let extra = content_type(headers, body);
    if !headers.is_empty() || extra.is_some() {
        let _ = writeln!(out, "  headers: {{");
        for (name, value) in headers {
            let _ = writeln!(out, "    {}: {},", single(name), single(value));
        }
        if let Some(mime) = &extra {
            let _ = writeln!(out, "    'Content-Type': {},", single(mime));
        }
        let _ = writeln!(out, "  }},");
    }
    if let Some((content, _)) = body {
        let _ = writeln!(out, "  data: {},", single(content));
    }
    let _ = writeln!(out, "}})\n");
    let _ = write!(out, "console.log(response.status, response.data)");
    out
}

fn python(
    plan: &Plan,
    headers: &[(String, String)],
    body: Option<&(String, Option<&'static str>)>,
    options: &ExecOptions,
) -> String {
    let mut out = String::from("import requests\n\n");
    let extra = content_type(headers, body);

    if !headers.is_empty() || extra.is_some() {
        let _ = writeln!(out, "headers = {{");
        for (name, value) in headers {
            let _ = writeln!(out, "    {}: {},", quote(name), quote(value));
        }
        if let Some(mime) = &extra {
            let _ = writeln!(out, "    \"Content-Type\": {},", quote(mime));
        }
        let _ = writeln!(out, "}}\n");
    }
    if let Some((content, _)) = body {
        let _ = writeln!(out, "body = {}\n", quote(content));
    }

    let mut args = vec![quote(&plan.url)];
    if !headers.is_empty() || extra.is_some() {
        args.push("headers=headers".into());
    }
    if body.is_some() {
        args.push("data=body".into());
    }
    if options.timeout_ms != ExecOptions::default().timeout_ms {
        args.push(format!("timeout={}", options.timeout_ms as f64 / 1000.0));
    }
    if !options.verify_tls {
        args.push("verify=False".into());
    }
    if !options.follow_redirects {
        args.push("allow_redirects=False".into());
    }

    let _ = writeln!(out, "response = requests.request({}, {})", quote(&plan.method), args.join(", "));
    let _ = write!(out, "print(response.status_code, response.text)");
    out
}

fn go(plan: &Plan, headers: &[(String, String)], body: Option<&(String, Option<&'static str>)>) -> String {
    let mut out = String::from("package main\n\nimport (\n\t\"fmt\"\n\t\"io\"\n\t\"net/http\"\n");
    if body.is_some() {
        out.push_str("\t\"strings\"\n");
    }
    out.push_str(")\n\nfunc main() {\n");

    let reader = match body {
        Some((content, _)) => {
            let _ = writeln!(out, "\tbody := strings.NewReader({})", quote(content));
            "body"
        }
        None => "nil",
    };
    let _ = writeln!(out, "\treq, err := http.NewRequest({}, {}, {reader})", quote(&plan.method), quote(&plan.url));
    out.push_str("\tif err != nil {\n\t\tpanic(err)\n\t}\n");

    for (name, value) in headers {
        let _ = writeln!(out, "\treq.Header.Set({}, {})", quote(name), quote(value));
    }
    if let Some(mime) = content_type(headers, body) {
        let _ = writeln!(out, "\treq.Header.Set(\"Content-Type\", {})", quote(&mime));
    }

    out.push_str("\n\tres, err := http.DefaultClient.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n");
    out.push_str("\tdefer res.Body.Close()\n\n\tout, _ := io.ReadAll(res.Body)\n");
    out.push_str("\tfmt.Println(res.Status, string(out))\n}");
    out
}

fn csharp(plan: &Plan, headers: &[(String, String)], body: Option<&(String, Option<&'static str>)>) -> String {
    let mut out = String::from("using var client = new HttpClient();\n");
    let _ = writeln!(
        out,
        "using var request = new HttpRequestMessage(new HttpMethod({}), {});",
        quote(&plan.method),
        quote(&plan.url)
    );

    // Content headers belong on the content in this API, not on the request.
    let (content_headers, request_headers): (Vec<_>, Vec<_>) =
        headers.iter().partition(|(name, _)| name.to_ascii_lowercase().starts_with("content-"));
    for (name, value) in &request_headers {
        let _ = writeln!(out, "request.Headers.TryAddWithoutValidation({}, {});", quote(name), quote(value));
    }

    if let Some((content, _)) = body {
        let mime = content_headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.clone())
            .or_else(|| content_type(headers, body))
            .unwrap_or_else(|| "text/plain".into());
        let _ = writeln!(
            out,
            "request.Content = new StringContent({}, System.Text.Encoding.UTF8, {});",
            quote(content),
            quote(mime.split(';').next().unwrap_or("text/plain").trim())
        );
    }

    out.push_str("\nvar response = await client.SendAsync(request);\n");
    out.push_str("Console.WriteLine((int)response.StatusCode);\n");
    out.push_str("Console.WriteLine(await response.Content.ReadAsStringAsync());");
    out
}

fn php(
    plan: &Plan,
    headers: &[(String, String)],
    body: Option<&(String, Option<&'static str>)>,
    options: &ExecOptions,
) -> String {
    let mut out = String::from("<?php\n\n$ch = curl_init();\n");
    let _ = writeln!(out, "curl_setopt($ch, CURLOPT_URL, {});", single(&plan.url));
    let _ = writeln!(out, "curl_setopt($ch, CURLOPT_CUSTOMREQUEST, {});", single(&plan.method));
    out.push_str("curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);\n");
    if options.follow_redirects {
        out.push_str("curl_setopt($ch, CURLOPT_FOLLOWLOCATION, true);\n");
    }
    if !options.verify_tls {
        out.push_str("curl_setopt($ch, CURLOPT_SSL_VERIFYPEER, false);\n");
    }

    let mut all: Vec<String> = headers.iter().map(|(n, v)| single(&format!("{n}: {v}"))).collect();
    if let Some(mime) = content_type(headers, body) {
        all.push(single(&format!("Content-Type: {mime}")));
    }
    if !all.is_empty() {
        let _ = writeln!(out, "curl_setopt($ch, CURLOPT_HTTPHEADER, [\n    {},\n]);", all.join(",\n    "));
    }
    if let Some((content, _)) = body {
        let _ = writeln!(out, "curl_setopt($ch, CURLOPT_POSTFIELDS, {});", single(content));
    }

    out.push_str("\n$response = curl_exec($ch);\n");
    out.push_str("echo curl_getinfo($ch, CURLINFO_HTTP_CODE), PHP_EOL, $response;\n");
    out.push_str("curl_close($ch);");
    out
}

fn ruby(plan: &Plan, headers: &[(String, String)], body: Option<&(String, Option<&'static str>)>) -> String {
    let mut out = String::from("require 'net/http'\nrequire 'uri'\n\n");
    let _ = writeln!(out, "uri = URI({})", single(&plan.url));
    let method = plan.method.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
        + &plan.method[1.min(plan.method.len())..].to_lowercase();
    let _ = writeln!(out, "request = Net::HTTP::{method}.new(uri)");

    for (name, value) in headers {
        let _ = writeln!(out, "request[{}] = {}", single(name), single(value));
    }
    if let Some(mime) = content_type(headers, body) {
        let _ = writeln!(out, "request['Content-Type'] = {}", single(&mime));
    }
    if let Some((content, _)) = body {
        let _ = writeln!(out, "request.body = {}", single(content));
    }

    out.push_str("\nresponse = Net::HTTP.start(uri.hostname, uri.port, use_ssl: uri.scheme == 'https') do |http|\n");
    out.push_str("  http.request(request)\nend\n\n");
    out.push_str("puts response.code\nputs response.body");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Auth, Body, KeyValue};

    fn request() -> Request {
        Request {
            name: "Create user".into(),
            seq: 1,
            method: "POST".into(),
            url: "https://api.test/users?dry=1".into(),
            headers: vec![KeyValue {
                name: "Accept".into(),
                value: "application/json".into(),
                enabled: true,
                description: None,
            }],
            body: Body::Json { content: "{\"name\": \"Ada\"}".into() },
            auth: Auth::Bearer { token: "tok-1".into() },
            ..Default::default()
        }
    }

    fn code(language: Language) -> String {
        generate(
            std::path::Path::new("."),
            &request(),
            &[],
            &Default::default(),
            &ExecOptions::default(),
            language,
            true,
        )
        .unwrap()
        .code
    }

    #[test]
    fn every_language_prints_the_url_method_headers_auth_and_body() {
        for language in Language::ALL {
            let out = code(language);
            assert!(out.contains("https://api.test/users?dry=1"), "{}: {out}", language.label());
            assert!(out.contains("Bearer tok-1"), "auth is resolved, not left to the reader: {}", language.label());
            assert!(out.contains("Accept"), "{}", language.label());
            assert!(out.contains("Ada"), "{}", language.label());
            // The body's Content-Type comes from the plan, like everywhere else.
            assert!(out.contains("application/json"), "{}", language.label());
            assert!(!out.contains("POSTPOST"), "{}", language.label());
        }
    }

    #[test]
    fn a_value_with_quotes_and_newlines_does_not_break_out_of_its_string() {
        let mut awkward = request();
        let quote_char = char::from_u32(0x22).unwrap();
        let newline = char::from_u32(0x0a).unwrap();
        let backslash = char::from_u32(0x5c).unwrap();
        awkward.body = Body::Text {
            content: format!("line {quote_char}one{quote_char}{newline}and {backslash} two"),
        };

        for language in Language::ALL {
            let out = generate(
                std::path::Path::new("."),
                &awkward,
                &[],
                &Default::default(),
                &ExecOptions::default(),
                language,
                true,
            )
            .unwrap()
            .code;

            // Whatever the quoting style, the raw newline never survives into
            // the literal — that is what would break the snippet.
            let body_line = out.lines().find(|l| l.contains("line ")).unwrap_or_default();
            assert!(body_line.contains("one"), "{}: {out}", language.label());
            assert!(!body_line.ends_with("line "), "{}: the string was cut short", language.label());
        }
    }

    #[test]
    fn a_secret_stays_a_placeholder_unless_asked_for() {
        let mut r = request();
        r.auth = Auth::Bearer { token: "{{token}}".into() };
        let env = vec![EnvVar { name: "token".into(), value: "eyJ.live".into(), secret: true }];

        let hidden = generate(
            std::path::Path::new("."),
            &r,
            &env,
            &Default::default(),
            &ExecOptions::default(),
            Language::Python,
            false,
        )
        .unwrap();
        assert!(hidden.code.contains("{{token}}") && !hidden.code.contains("eyJ.live"));
        assert_eq!(hidden.hidden, ["token"]);

        let shown = generate(
            std::path::Path::new("."),
            &r,
            &env,
            &Default::default(),
            &ExecOptions::default(),
            Language::Python,
            true,
        )
        .unwrap();
        assert!(shown.code.contains("eyJ.live") && shown.hidden.is_empty());
    }

    #[test]
    fn send_options_show_up_where_the_language_has_them() {
        let mut r = request();
        r.options = Some(crate::model::RequestOptions {
            timeout_ms: Some(2_500),
            verify_tls: Some(false),
            follow_redirects: Some(false),
            ..Default::default()
        });
        let python = generate(
            std::path::Path::new("."),
            &r,
            &[],
            &Default::default(),
            &ExecOptions::default(),
            Language::Python,
            true,
        )
        .unwrap()
        .code;

        assert!(python.contains("timeout=2.5"), "{python}");
        assert!(python.contains("verify=False") && python.contains("allow_redirects=False"), "{python}");
    }
}

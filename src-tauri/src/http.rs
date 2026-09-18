//! Request execution.
//!
//! The HTTP call happens in Rust rather than the webview on purpose: no CORS
//! restrictions, real redirect/TLS control, and honest timing numbers.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{ApiKeyLocation, Auth, Body, KeyValue, Request};
use crate::vars;

fn default_timeout() -> u64 {
    30_000
}

fn default_true() -> bool {
    true
}

/// How a request goes out, after the app's settings and the request's own
/// overrides have been put together. IPC only, hence camelCase.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecOptions {
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    /// `None` or empty leaves reqwest to read the system proxy settings.
    #[serde(default)]
    pub proxy: Option<String>,
    /// Keep cookies between sends, per collection.
    #[serde(default = "default_true")]
    pub send_cookies: bool,
    /// Path to a PEM holding a client certificate and its key, for servers
    /// that ask for one. Relative paths are taken from the collection root.
    #[serde(default)]
    pub client_cert: Option<String>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            timeout_ms: default_timeout(),
            follow_redirects: true,
            verify_tls: true,
            proxy: None,
            send_cookies: true,
            client_cert: None,
        }
    }
}

impl ExecOptions {
    /// The settings with a request's own overrides laid on top. Both `execute`
    /// and `curl::export` call this, so a per-request timeout shows up as
    /// `--max-time` in the printed command as well as on the wire.
    pub fn for_request(&self, request: &Request) -> ExecOptions {
        let Some(over) = request.options.as_ref() else { return self.clone() };
        ExecOptions {
            timeout_ms: over.timeout_ms.unwrap_or(self.timeout_ms),
            follow_redirects: over.follow_redirects.unwrap_or(self.follow_redirects),
            verify_tls: over.verify_tls.unwrap_or(self.verify_tls),
            proxy: over.proxy.clone().or_else(|| self.proxy.clone()),
            send_cookies: self.send_cookies,
            client_cert: over.client_cert.clone().or_else(|| self.client_cert.clone()),
        }
    }

    pub(crate) fn proxy_url(&self) -> Option<&str> {
        self.proxy.as_deref().map(str::trim).filter(|url| !url.is_empty())
    }
}

/// Also stored in request history, hence `Deserialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    /// UTF-8 text when the body decodes cleanly, base64 otherwise.
    pub body: String,
    pub body_is_base64: bool,
    pub size_bytes: usize,
    pub duration_ms: u64,
    pub time_to_first_byte_ms: u64,
    pub final_url: String,
    /// The request as actually sent, for the "raw" tab in the UI.
    pub sent_url: String,
    /// The method and headers that went out, for the timeline. Taken from the
    /// plan, so it is what was sent rather than a guess at it.
    ///
    /// These carry `#[serde(default)]` because history entries written before
    /// they existed have to keep loading — a stricter struct would make old
    /// history silently disappear, since a line that fails to parse is skipped.
    #[serde(default)]
    pub sent_method: String,
    #[serde(default)]
    pub sent_headers: Vec<KeyValue>,
    #[serde(default)]
    pub sent_body_bytes: usize,
    /// Every URL a redirect led to, in order.
    #[serde(default)]
    pub redirects: Vec<String>,
    /// `HTTP/1.1`, `HTTP/2.0`.
    #[serde(default)]
    pub version: String,
    /// Variables referenced by the request that had no value anywhere.
    pub missing_vars: Vec<String>,
}

pub struct ExecContext<'a> {
    pub root: &'a Path,
    pub vars: &'a HashMap<String, String>,
    pub scopes: &'a crate::collection::Scopes,
    pub options: &'a ExecOptions,
    /// The cookie jar for this collection, or `None` to send none.
    pub cookies: Option<Arc<crate::cookies::Jar>>,
}

/// Everything a request resolves to before it goes on the wire.
///
/// `execute` sends a plan and `curl::render` prints one. Keeping a single
/// builder for both is what makes "Copy as cURL" match what Send sends:
/// same variables, same inherited auth, same header overrides, same query.
#[derive(Debug, Clone)]
pub struct Plan {
    pub method: String,
    /// With enabled params, and an API key sent in the query, appended.
    pub url: String,
    /// In order. A later header replaces an earlier one of the same name,
    /// case-insensitively, so there is at most one of each.
    pub headers: Vec<(String, String)>,
    /// Basic credentials, kept apart so curl can print `-u`; sent as a header.
    pub basic: Option<(String, String)>,
    /// Digest credentials. Nothing can be written until the server has
    /// challenged, so they travel with the plan and `execute` answers.
    pub digest: Option<(String, String)>,
    /// NTLM credentials: username, password, domain. Same reason, three legs.
    pub ntlm: Option<(String, String, String)>,
    /// AWS credentials. The signature covers the finished request, so it is
    /// added after the plan rather than inside it.
    pub aws: Option<AwsAuth>,
    pub body: PlanBody,
    /// The header an API key is sent in, when it is sent in one. Only
    /// `execute` reads it: reqwest strips `Authorization` and `Cookie` across
    /// an origin change but not a header volt invented, so it has to know.
    pub api_key_header: Option<String>,
    /// Variables referenced but not defined, sorted and unique.
    pub missing: Vec<String>,
}

/// Resolved AWS credentials, for `aws::sign`.
#[derive(Debug, Clone)]
pub struct AwsAuth {
    pub key_id: String,
    pub secret: String,
    pub region: String,
    pub service: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PlanBody {
    None,
    /// Sent as-is; `default_type` is used only when no Content-Type is set.
    Text { content: String, default_type: &'static str },
    UrlEncoded(Vec<(String, String)>),
    Multipart(Vec<Part>),
    File { path: std::path::PathBuf },
}
/// One part of a multipart body, after resolution.
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    /// The text to send, or the path to upload when `file`.
    pub value: String,
    pub file: bool,
}


pub struct PlanContext<'a> {
    pub root: &'a Path,
    pub vars: &'a HashMap<String, String>,
    pub scopes: &'a crate::collection::Scopes,
    /// A URL that does not parse (say `{{baseUrl}}` left unresolved on
    /// purpose) is an error when sending, but can still be printed.
    pub lenient_url: bool,
}

pub fn plan(request: &Request, ctx: &PlanContext<'_>) -> Result<Plan> {
    let mut missing: Vec<String> = Vec::new();
    let mut resolve = |input: &str| -> String {
        let out = vars::interpolate(input, ctx.vars);
        missing.extend(out.missing);
        out.value
    };

    // An HTTP method is a token: letters, digits and a few symbols (M-SEARCH is valid).
    let method = request.method.trim().to_uppercase();
    let token = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    if method.is_empty() || !method.bytes().all(token) {
        return Err(Error::Invalid(format!("unsupported method `{}`", request.method)));
    }

    // --- Query --------------------------------------------------------------
    let raw_url = resolve(request.url.trim());
    let mut api_key_header: Option<String> = None;
    let mut query: Vec<(String, String)> = request
        .params
        .iter()
        .filter(|p| p.enabled)
        .map(|p| (resolve(&p.name), resolve(&p.value)))
        .collect();

    // --- Headers ------------------------------------------------------------
    // Collection headers first so a request can override them by name.
    let mut headers: Vec<(String, String)> = Vec::new();
    // Collection, then each folder, then the request: a nearer scope replaces
    // a header of the same name set further out.
    for kv in ctx.scopes.headers().chain(request.headers.iter()).filter(|h| h.enabled) {
        let name = resolve(&kv.name);
        if !name.is_empty() {
            set_header(&mut headers, name, resolve(&kv.value));
        }
    }

    // --- Auth ---------------------------------------------------------------
    let auth = match &request.auth {
        Auth::Inherit => ctx.scopes.auth(),
        other => other,
    };
    let mut basic = None;
    let mut digest = None;
    let mut ntlm = None;
    let mut aws = None;
    match auth {
        Auth::None | Auth::Inherit => {}
        Auth::Bearer { token } => {
            let value = format!("Bearer {}", resolve(token));
            set_header(&mut headers, "authorization".into(), value);
        }
        Auth::Basic { username, password } => {
            // Basic auth wins over any Authorization header, as when sending.
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
            basic = Some((resolve(username), resolve(password)));
        }
        Auth::Digest { username, password } => {
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
            digest = Some((resolve(username), resolve(password)));
        }
        Auth::Ntlm { username, password, domain } => {
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
            ntlm = Some((resolve(username), resolve(password), resolve(domain)));
        }
        Auth::AwsSigV4 { key_id, secret, region, service, session_token } => {
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
            let token = resolve(session_token);
            aws = Some(AwsAuth {
                key_id: resolve(key_id),
                secret: resolve(secret),
                region: resolve(region),
                service: resolve(service),
                session_token: (!token.trim().is_empty()).then_some(token),
            });
        }
        Auth::ApiKey { key, value, location } => {
            let key = resolve(key);
            let value = resolve(value);
            match location {
                ApiKeyLocation::Header => {
                    api_key_header = Some(key.clone());
                    set_header(&mut headers, key, value)
                }
                ApiKeyLocation::Query => query.push((key, value)),
            }
        }
    }

    let url = join_query(&raw_url, &query, ctx.lenient_url)?;

    // --- Body ---------------------------------------------------------------
    let fields = |fields: &[KeyValue], resolve: &mut dyn FnMut(&str) -> String| -> Vec<(String, String)> {
        fields.iter().filter(|f| f.enabled).map(|f| (resolve(&f.name), resolve(&f.value))).collect()
    };
    let body = match &request.body {
        Body::None => PlanBody::None,
        Body::Text { content } => PlanBody::Text { content: resolve(content), default_type: "text/plain" },
        Body::Json { content } => PlanBody::Text { content: resolve(content), default_type: "application/json" },
        Body::Xml { content } => PlanBody::Text { content: resolve(content), default_type: "application/xml" },
        Body::UrlEncoded { fields: f } => PlanBody::UrlEncoded(fields(f, &mut resolve)),
        Body::Form { fields: f } => PlanBody::Multipart(
            f.iter()
                .filter(|field| field.enabled)
                .map(|field| Part { name: resolve(&field.name), value: resolve(&field.value), file: field.file })
                .filter(|part| !part.name.is_empty())
                .collect(),
        ),
        Body::Binary { path } => PlanBody::File { path: ctx.root.join(resolve(path)) },
        // On the wire GraphQL is a JSON POST; the shape is the specification's,
        // and invalid variables travel as written rather than being repaired.
        // gRPC does not go out through `execute`; it has its own transport and
        // its own command. A plan is still built, for the URL and the headers.
        Body::Grpc { .. } => PlanBody::None,
        Body::GraphQl { query, variables } => {
            let variables = resolve(variables);
            let variables: serde_json::Value = if variables.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(variables.trim()).unwrap_or(serde_json::Value::Null)
            };
            let payload = serde_json::json!({ "query": resolve(query), "variables": variables });
            PlanBody::Text {
                content: serde_json::to_string(&payload).unwrap_or_default(),
                default_type: "application/json",
            }
        }
    };

    missing.sort();
    missing.dedup();
    Ok(Plan { method, url, headers, basic, digest, ntlm, aws, body, api_key_header, missing })
}

/// Replace every header of the same name, then add this one.
fn set_header(headers: &mut Vec<(String, String)>, name: String, value: String) {
    headers.retain(|(existing, _)| !existing.eq_ignore_ascii_case(&name));
    headers.push((name, value));
}

fn join_query(raw_url: &str, query: &[(String, String)], lenient: bool) -> Result<String> {
    match url::Url::parse(raw_url) {
        Ok(mut url) => {
            if !query.is_empty() {
                let mut pairs = url.query_pairs_mut();
                for (name, value) in query {
                    pairs.append_pair(name, value);
                }
            }
            if url.query() == Some("") {
                url.set_query(None);
            }
            Ok(url.to_string())
        }
        Err(e) if !lenient => Err(Error::Url { url: raw_url.to_string(), message: e.to_string() }),
        Err(_) => {
            if query.is_empty() {
                return Ok(raw_url.to_string());
            }
            let encoded = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query.iter().map(|(n, v)| (n.as_str(), v.as_str())))
                .finish();
            let separator = if raw_url.contains('?') { '&' } else { '?' };
            Ok(format!("{raw_url}{separator}{encoded}"))
        }
    }
}


/// A urlencoded body, in the one encoding the signature, the wire and the
/// printed curl command all agree on.
pub(crate) fn urlencoded_body(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", crate::aws::encode(name), crate::aws::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn execute(request: &Request, ctx: &ExecContext<'_>) -> Result<HttpResponse> {
    let plan = plan(
        request,
        &PlanContext {
            root: ctx.root,
            vars: ctx.vars,
            scopes: ctx.scopes,
            lenient_url: false,
        },
    )?;

    let method = reqwest::Method::from_bytes(plan.method.as_bytes())
        .map_err(|_| Error::Invalid(format!("unsupported method `{}`", plan.method)))?;
    let url = url::Url::parse(&plan.url).map_err(|e| Error::Url { url: plan.url.clone(), message: e.to_string() })?;

    let mut headers = HeaderMap::new();
    for (name, value) in &plan.headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::Invalid(format!("invalid header name `{name}`")))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| Error::Invalid(format!("invalid value for header `{name}`")))?;
        headers.insert(header_name, header_value);
    }
    if let Some((username, password)) = &plan.basic {
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        let value = HeaderValue::from_str(&format!("Basic {encoded}"))
            .map_err(|_| Error::Invalid("invalid basic auth credentials".into()))?;
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }

    // An explicit Content-Type on the request always wins over the default.
    let has_type = headers.contains_key(CONTENT_TYPE);

    // The settings with this request's own overrides on top, worked out once.
    let options = ctx.options.for_request(request);

    // Record where redirects went, so the timeline can show the trail rather
    // than only the address it ended up at.
    let hops: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();

    // reqwest strips `Authorization` and `Cookie` when a redirect crosses an
    // origin, but nothing else. An API key in a header of its own, and an AWS
    // session token, would follow a redirect to another host — or down to plain
    // http — and be handed to whoever answers there.
    let carries_credentials = plan.api_key_header.is_some()
        || plan.aws.as_ref().is_some_and(|aws| aws.session_token.is_some());

    let redirects = if options.follow_redirects {
        let log = hops.clone();
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            if carries_credentials {
                let previous = attempt.previous().last();
                let crosses = previous.is_some_and(|from| {
                    from.host_str() != attempt.url().host_str()
                        || from.port_or_known_default() != attempt.url().port_or_known_default()
                        || from.scheme() != attempt.url().scheme()
                });
                if crosses {
                    // Still logged, so the timeline shows where it wanted to go.
                    log.lock().expect("redirect log").push(attempt.url().to_string());
                    return attempt.stop();
                }
            }
            log.lock().expect("redirect log").push(attempt.url().to_string());
            attempt.follow()
        })
    } else {
        reqwest::redirect::Policy::none()
    };
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_millis(options.timeout_ms))
        .danger_accept_invalid_certs(!options.verify_tls)
        .redirect(redirects)
        .user_agent(concat!("volt/", env!("CARGO_PKG_VERSION")));

    match options.proxy_url() {
        Some(url) => {
            let proxy = reqwest::Proxy::all(url).map_err(|e| Error::Invalid(format!("proxy `{url}`: {e}")))?;
            builder = builder.proxy(proxy);
        }
        // An explicit empty proxy means "go direct", which also turns off the
        // HTTP_PROXY variables reqwest would otherwise pick up by itself.
        None if options.proxy.is_some() => builder = builder.no_proxy(),
        None => {}
    }
    // A client certificate the server asks for. rustls wants one PEM holding
    // both the certificate and its key; a passphrase-protected key is not
    // something it can open, which is why there is no password field.
    if let Some(cert) = options.client_cert.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        let path = ctx.root.join(cert);
        let pem = std::fs::read(&path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
        let identity = reqwest::Identity::from_pem(&pem)
            .map_err(|e| Error::Invalid(format!("client certificate `{cert}`: {e}")))?;
        builder = builder.identity(identity);
    }


    // One jar per collection, handed in by the caller. Without it every send
    // builds a fresh client and a session cookie never survives to the next
    // request.
    if options.send_cookies {
        if let Some(jar) = ctx.cookies.clone() {
            builder = builder.cookie_provider(jar);
        }
    }

    // NTLM authenticates the connection, not the request, so the three legs
    // have to go down one socket: HTTP/1.1, one pooled connection, and no
    // redirects in the middle of a handshake.
    if plan.ntlm.is_some() {
        builder = builder.http1_only().pool_max_idle_per_host(1);
    }

    let client = builder.build().map_err(|e| Error::Http(e.to_string()))?;

    // The default Content-Type is decided here rather than inside the match,
    // so the headers can be snapshotted for the timeline exactly as sent.
    // multipart is left out on purpose: its type carries the boundary reqwest
    // generates. urlencoded is here because volt now serialises that body
    // itself — see the send below.
    let default_type = match &plan.body {
        PlanBody::Text { default_type, .. } => Some(*default_type),
        PlanBody::File { .. } => Some("application/octet-stream"),
        PlanBody::UrlEncoded(_) => Some("application/x-www-form-urlencoded"),
        _ => None,
    };
    if let Some(mime) = default_type.filter(|_| !has_type) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(mime));
    }
    // AWS signs the request as it will go out, so this is the last thing done
    // to the headers before they are snapshotted and sent.
    if let Some(credentials) = &plan.aws {
        let mut for_signing = plan.clone();
        if let Some(mime) = default_type.filter(|_| !has_type) {
            for_signing.headers.push(("content-type".into(), mime.to_string()));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let signed = crate::aws::sign(
            &for_signing,
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
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Error::Invalid(format!("invalid header name `{name}`")))?;
            let value = HeaderValue::from_str(&value)
                .map_err(|_| Error::Invalid("the AWS signature is not a valid header".into()))?;
            headers.insert(name, value);
        }
    }

    let sent_headers: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
        .collect();

    let mut body_bytes = 0;
    let builder = match plan.body {
        PlanBody::None => client.request(method, url.clone()).headers(headers),
        PlanBody::Text { content, .. } => {
            body_bytes = content.len();
            client.request(method, url.clone()).headers(headers).body(content)
        }
        PlanBody::UrlEncoded(pairs) => {
            // Serialised here rather than by `.form()`, which writes a space as
            // `+`. SigV4 hashes the RFC 3986 form, so with `.form()` the bytes
            // signed and the bytes sent differed and every field with a space
            // in it broke the signature.
            let encoded = urlencoded_body(&pairs);
            body_bytes = encoded.len();
            client.request(method, url.clone()).headers(headers).body(encoded)
        }
        PlanBody::Multipart(parts) => {
            let mut form = reqwest::multipart::Form::new();
            for part in parts {
                if !part.file {
                    form = form.text(part.name, part.value);
                    continue;
                }
                // Read it rather than stream it: an API client sends fixtures,
                // not disk images, and this keeps the error at the send rather
                // than half way through the upload.
                let path = ctx.root.join(&part.value);
                let bytes = std::fs::read(&path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                body_bytes += bytes.len();
                form = form.part(part.name, reqwest::multipart::Part::bytes(bytes).file_name(name));
            }
            client.request(method, url.clone()).headers(headers).multipart(form)
        }
        PlanBody::File { path } => {
            let bytes = std::fs::read(&path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
            body_bytes = bytes.len();
            client.request(method, url.clone()).headers(headers).body(bytes)
        }
    };
    let missing = plan.missing;

    // --- Send ------------------------------------------------------------
    // Digest needs the request again once the server has challenged, and a
    // multipart body cannot be cloned — which is fine: nobody sends one to a
    // Digest endpoint, and the 401 comes back either way.
    let retry = builder.try_clone();
    let retry_ntlm = builder.try_clone();

    let started = Instant::now();
    let mut response = builder.send().await.map_err(|e| Error::Http(describe(&e)))?;
    let mut ttfb = started.elapsed();

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let (Some((username, password)), Some(retry)) = (&plan.digest, retry) {
            let challenge = response
                .headers()
                .get_all(reqwest::header::WWW_AUTHENTICATE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .find_map(crate::digest::challenge);

            if let Some(challenge) = challenge {
                let path = match url.query() {
                    Some(query) => format!("{}?{query}", url.path()),
                    None => url.path().to_string(),
                };
                let cnonce = crate::vars::dynamics().remove("$guid").unwrap_or_default();
                let answer =
                    crate::digest::answer(&challenge, username, password, &plan.method, &path, &cnonce);

                let again = Instant::now();
                response = retry
                    .header(reqwest::header::AUTHORIZATION, answer)
                    .send()
                    .await
                    .map_err(|e| Error::Http(describe(&e)))?;
                ttfb = again.elapsed();
            }
        }
    }

    // NTLM: negotiate, read the challenge, authenticate. Three sends down the
    // same connection, which is why the client above was built for it.
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let (Some((username, password, domain)), Some(first)) = (&plan.ntlm, retry_ntlm) {
            use base64::Engine as _;
            let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);

            let offered = response
                .headers()
                .get_all(reqwest::header::WWW_AUTHENTICATE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .any(|value| value.trim().to_ascii_uppercase().starts_with("NTLM"));

            if offered {
                let again = Instant::now();
                let second = first
                    .try_clone()
                    .ok_or_else(|| Error::Invalid("this body cannot be sent twice, which NTLM needs".into()))?;

                let challenged = first
                    .header(reqwest::header::AUTHORIZATION, format!("NTLM {}", encode(&crate::ntlm::negotiate())))
                    .send()
                    .await
                    .map_err(|e| Error::Http(describe(&e)))?;

                let token = challenged
                    .headers()
                    .get_all(reqwest::header::WWW_AUTHENTICATE)
                    .iter()
                    .filter_map(|value| value.to_str().ok())
                    .find_map(|value| value.trim().strip_prefix("NTLM ").map(str::trim))
                    .ok_or_else(|| Error::Invalid("the server did not send an NTLM challenge back".into()))?;

                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(token)
                    .map_err(|_| Error::Invalid("the NTLM challenge was not base64".into()))?;
                let challenge = crate::ntlm::parse_challenge(&bytes)?;

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                // An NTLMv2 client challenge has to be unguessable, so it comes
                // from the OS rather than from the clock-seeded test-data source.
                let mut nonce = [0u8; 8];
                getrandom::fill(&mut nonce).map_err(|e| Error::Http(format!("no random source: {e}")))?;
                let answer = crate::ntlm::authenticate(
                    &challenge,
                    username,
                    password,
                    domain,
                    "volt",
                    now,
                    nonce,
                );

                response = second
                    .header(reqwest::header::AUTHORIZATION, format!("NTLM {}", encode(&answer)))
                    .send()
                    .await
                    .map_err(|e| Error::Http(describe(&e)))?;
                ttfb = again.elapsed();
            }
        }
    }

    let version = format!("{:?}", response.version());

    let status = response.status();
    let final_url = response.url().to_string();
    let response_headers = response
        .headers()
        .iter()
        .map(|(name, value)| KeyValue {
            name: name.to_string(),
            value: value.to_str().unwrap_or("<binary>").to_string(),
            enabled: true,
            description: None,
        })
        .collect();

    let bytes = response.bytes().await.map_err(|e| Error::Http(describe(&e)))?;
    let elapsed = started.elapsed();

    let trail = hops.lock().expect("redirect log").clone();

    let (body, body_is_base64) = match std::str::from_utf8(&bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (base64::engine::general_purpose::STANDARD.encode(&bytes), true),
    };

    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers: response_headers,
        body,
        body_is_base64,
        size_bytes: bytes.len(),
        duration_ms: elapsed.as_millis() as u64,
        time_to_first_byte_ms: ttfb.as_millis() as u64,
        final_url,
        sent_url: url.to_string(),
        sent_method: plan.method.clone(),
        sent_headers: sent_headers
            .iter()
            .map(|(name, value)| KeyValue { name: name.clone(), value: value.clone(), enabled: true, description: None })
            .collect(),
        sent_body_bytes: body_bytes,
        redirects: trail,
        version,
        missing_vars: missing,
    })
}

/// reqwest's Display is terse; surface the part that tells the user what broke.
fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "timed out".to_string();
    }
    if error.is_connect() {
        return format!("could not connect: {error}");
    }
    match std::error::Error::source(error) {
        Some(source) => format!("{error}: {source}"),
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Body, KeyValue, Request};

    fn ctx_vars() -> HashMap<String, String> {
        [("baseUrl".to_string(), "https://httpbin.org".to_string()), ("userId".to_string(), "42".to_string())]
            .into_iter()
            .collect()
    }

    /// The UI sends camelCase over IPC. If these names drift, the options are
    /// silently replaced by their defaults rather than failing loudly.
    #[test]
    fn exec_options_deserialize_from_what_the_ui_sends() {
        let options: ExecOptions = serde_json::from_str(
            r#"{"timeoutMs": 5000, "followRedirects": false, "verifyTls": false}"#,
        )
        .expect("the UI payload should deserialize");

        assert_eq!(options.timeout_ms, 5000);
        assert!(!options.follow_redirects);
        assert!(!options.verify_tls);
    }

    /// `send_request` takes `Option<ExecOptions>`, so an older UI that sends
    /// nothing must still get safe behaviour rather than a zero timeout.
    #[test]
    fn exec_options_fall_back_to_safe_defaults() {
        let partial: ExecOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(partial.timeout_ms, 30_000);
        assert!(partial.follow_redirects);
        assert!(partial.verify_tls, "TLS verification must default to on");

        let absent = ExecOptions::default();
        assert_eq!(absent.timeout_ms, 30_000);
        assert!(absent.verify_tls);
    }

    fn plan_of(request: &Request, collection_headers: &[KeyValue], collection_auth: &Auth, lenient: bool) -> Result<Plan> {
        let scopes = crate::collection::Scopes {
            collection: crate::model::CollectionMeta {
                headers: collection_headers.to_vec(),
                auth: collection_auth.clone(),
                ..Default::default()
            },
            folders: Vec::new(),
        };
        plan_in(request, &scopes, lenient)
    }

    fn plan_in(request: &Request, scopes: &crate::collection::Scopes, lenient: bool) -> Result<Plan> {
        let vars = ctx_vars();
        plan(request, &PlanContext { root: Path::new("/c"), vars: &vars, scopes, lenient_url: lenient })
    }

    fn kv(name: &str, value: &str) -> KeyValue {
        KeyValue { name: name.into(), value: value.into(), enabled: true, description: None }
    }

    #[test]
    fn a_request_overrides_only_the_send_options_it_names() {
        let app = ExecOptions { timeout_ms: 30_000, proxy: Some("http://corp:8080".into()), ..Default::default() };
        let mut request = Request { method: "GET".into(), url: "https://x.test".into(), ..Default::default() };

        assert_eq!(app.for_request(&request).timeout_ms, 30_000, "no overrides: the settings stand");

        request.options = Some(crate::model::RequestOptions { timeout_ms: Some(1_000), ..Default::default() });
        let merged = app.for_request(&request);
        assert_eq!(merged.timeout_ms, 1_000);
        assert!(merged.verify_tls, "what the request does not name is left alone");
        assert_eq!(merged.proxy_url(), Some("http://corp:8080"), "the settings proxy still applies");

        // An empty proxy on the request is not "nothing said", it is "go direct".
        request.options = Some(crate::model::RequestOptions { proxy: Some(String::new()), ..Default::default() });
        let direct = app.for_request(&request);
        assert_eq!(direct.proxy_url(), None);
        assert!(direct.proxy.is_some(), "…which is different from never having had one");
    }

    #[test]
    fn a_graphql_body_goes_out_as_the_json_post_the_spec_says() {
        let request = Request {
            method: "POST".into(),
            url: "https://api.test/graphql".into(),
            body: crate::model::Body::GraphQl {
                query: "query($id: ID!) { user(id: $id) { name } }".into(),
                variables: "{\"id\": \"{{userId}}\"}".into(),
            },
            ..Default::default()
        };

        let plan = plan_of(&request, &[], &Auth::None, false).unwrap();
        let PlanBody::Text { content, default_type } = &plan.body else { panic!("{:?}", plan.body) };
        assert_eq!(*default_type, "application/json");

        let sent: serde_json::Value = serde_json::from_str(content).expect("valid JSON");
        assert_eq!(sent["query"], "query($id: ID!) { user(id: $id) { name } }");
        assert_eq!(sent["variables"]["id"], "42", "variables are interpolated like everything else");
    }

    #[test]
    fn graphql_without_variables_still_sends_an_object() {
        let request = Request {
            method: "POST".into(),
            url: "https://api.test/graphql".into(),
            body: crate::model::Body::GraphQl { query: "{ me { id } }".into(), variables: String::new() },
            ..Default::default()
        };

        let plan = plan_of(&request, &[], &Auth::None, false).unwrap();
        let PlanBody::Text { content, .. } = &plan.body else { panic!() };
        let sent: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(sent["variables"], serde_json::json!({}), "an empty object, not null: servers reject null");
    }

    #[test]
    fn a_nearer_scope_wins_over_a_wider_one() {
        let scopes = crate::collection::Scopes {
            collection: crate::model::CollectionMeta {
                headers: vec![kv("X-Trace", "collection"), kv("Accept", "application/json")],
                auth: crate::model::Auth::Bearer { token: "collection".into() },
                ..Default::default()
            },
            folders: vec![
                crate::model::FolderMeta {
                    headers: vec![kv("x-trace", "outer")],
                    auth: crate::model::Auth::Inherit,
                    ..Default::default()
                },
                crate::model::FolderMeta {
                    headers: vec![kv("X-Trace", "inner")],
                    auth: crate::model::Auth::Bearer { token: "folder".into() },
                    ..Default::default()
                },
            ],
        };

        let request = Request {
            method: "GET".into(),
            url: "https://x.test/".into(),
            auth: crate::model::Auth::Inherit,
            ..Default::default()
        };
        let plan = plan_in(&request, &scopes, false).unwrap();

        let header = |name: &str| plan.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());
        assert_eq!(header("X-Trace"), Some("inner"), "the innermost folder sets it, whatever the case");
        assert_eq!(plan.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case("x-trace")).count(), 1);
        assert_eq!(header("Accept"), Some("application/json"), "and the collection still contributes");
        assert_eq!(header("authorization"), Some("Bearer folder"), "a folder set to inherit passes it outwards");

        // The request always has the last word.
        let mut own = request.clone();
        own.headers = vec![kv("X-TRACE", "request")];
        own.auth = crate::model::Auth::Bearer { token: "request".into() };
        let plan = plan_in(&own, &scopes, false).unwrap();
        assert_eq!(plan.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("x-trace")).map(|(_, v)| v.as_str()), Some("request"));
        assert!(plan.headers.iter().any(|(_, v)| v == "Bearer request"));
    }

    #[test]
    fn a_request_header_replaces_a_collection_header_whatever_its_case() {
        let request = Request {
            method: "get".into(),
            url: "{{baseUrl}}/x".into(),
            headers: vec![kv("accept", "text/plain"), KeyValue { enabled: false, ..kv("X-Off", "1") }],
            ..Default::default()
        };
        let p = plan_of(&request, &[kv("Accept", "application/json"), kv("X-Team", "core")], &Auth::None, false).unwrap();
        assert_eq!(p.method, "GET");
        assert_eq!(p.headers, [("X-Team".to_string(), "core".to_string()), ("accept".to_string(), "text/plain".to_string())]);
    }

    #[test]
    fn inherited_auth_lands_where_it_is_sent() {
        let base = Request { method: "GET".into(), url: "{{baseUrl}}/x".into(), params: vec![kv("q", "1")], ..Default::default() };

        let key = Auth::ApiKey { key: "api_key".into(), value: "k".into(), location: ApiKeyLocation::Query };
        let p = plan_of(&base, &[], &key, false).unwrap();
        assert_eq!(p.url, "https://httpbin.org/x?q=1&api_key=k", "the key follows the params");

        // Basic auth replaces an Authorization header rather than sending both.
        let mut with_header = base.clone();
        with_header.headers = vec![kv("Authorization", "Bearer stale")];
        with_header.auth = Auth::Basic { username: "u".into(), password: "p".into() };
        let p = plan_of(&with_header, &[], &Auth::None, false).unwrap();
        assert!(p.headers.is_empty());
        assert_eq!(p.basic, Some(("u".into(), "p".into())));
    }

    #[test]
    fn an_unparseable_url_fails_to_send_but_can_still_be_printed() {
        let request = Request { method: "GET".into(), url: "{{host}}/x".into(), params: vec![kv("a b", "c&d")], ..Default::default() };
        assert!(plan_of(&request, &[], &Auth::None, false).is_err());
        let p = plan_of(&request, &[], &Auth::None, true).unwrap();
        assert_eq!(p.url, "{{host}}/x?a+b=c%26d");
        assert_eq!(p.missing, ["host"]);
    }

    /// Hits the network, so it is opt-in: `cargo test -- --ignored`.
    #[ignore]
    #[tokio::test]
    async fn sends_params_headers_and_auth() {
        let request = Request {
            name: "probe".into(),
            seq: 1,
            method: "GET".into(),
            url: "{{baseUrl}}/get".into(),
            params: vec![KeyValue {
                name: "q".into(),
                value: "volt".into(),
                enabled: true,
                description: None,
            }],
            headers: vec![KeyValue {
                name: "X-Probe".into(),
                value: "1".into(),
                enabled: true,
                description: None,
            }],
            body: Body::None,
            auth: crate::model::Auth::Bearer { token: "tok123".into() },
            ..Default::default()
        };

        let vars = ctx_vars();
        let options = ExecOptions::default();
        let response = execute(
            &request,
            &ExecContext {
                root: Path::new("."),
                vars: &vars,
                scopes: &Default::default(),
                options: &options,
                cookies: None,
            },
        )
        .await
        .expect("request should succeed");

        assert_eq!(response.status, 200);
        assert!(response.missing_vars.is_empty());
        // httpbin echoes the request back, so we can check what actually went out.
        assert!(response.body.contains("\"q\": \"volt\""), "{}", response.body);
        assert!(response.body.contains("Bearer tok123"), "{}", response.body);
        assert!(response.body.contains("X-Probe"), "{}", response.body);
    }
}

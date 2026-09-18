//! OAuth 2.0, as a step rather than a kind of auth.
//!
//! The obvious design is an `Auth::OAuth2` that fetches a token on every send.
//! This does the opposite: you get a token once, it lands in the environment
//! as a secret variable, and requests use `{{token}}` like any other. That
//! means the token is visible, reusable, shareable as a `{{name}}`, and gone
//! when the environment is — all the properties everything else in volt has,
//! and none of them true of a token hidden inside an auth object.
//!
//! Two grants: client credentials, which is a single POST, and authorization
//! code with PKCE, which needs a browser and somewhere for it to come back to.
//! The somewhere is a loopback listener that lives for one redirect.

use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub token_url: String,
    #[serde(default)]
    pub auth_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub scope: String,
    /// Some providers want an `audience`; it is passed through when set.
    #[serde(default)]
    pub audience: String,
    /// Send the client id and secret as Basic auth rather than form fields.
    /// Providers disagree about which they want; this is the switch.
    #[serde(default)]
    pub basic_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
    /// Unix seconds, when the provider said how long it lasts.
    pub expires_at: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    // The error shape from RFC 6749 §5.2, so a refusal reads as one.
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn post_token(config: &Config, form: Vec<(String, String)>) -> Result<Token> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("volt/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Http(e.to_string()))?;

    let mut form = form;
    let mut request = client.post(config.token_url.trim());
    if config.basic_auth {
        request = request.basic_auth(config.client_id.trim(), Some(config.client_secret.trim()));
    } else {
        form.push(("client_id".into(), config.client_id.trim().to_string()));
        if !config.client_secret.trim().is_empty() {
            form.push(("client_secret".into(), config.client_secret.trim().to_string()));
        }
    }

    let response = request
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| Error::Http(format!("the token endpoint could not be reached: {e}")))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let parsed: TokenResponse = serde_json::from_str(&body).map_err(|_| {
        Error::Invalid(format!("the token endpoint answered {status} with something that is not a token: {body}"))
    })?;

    if let Some(error) = parsed.error {
        let detail = parsed.error_description.unwrap_or_default();
        return Err(Error::Invalid(format!("{error}{}{detail}", if detail.is_empty() { "" } else { ": " })));
    }
    if parsed.access_token.is_empty() {
        return Err(Error::Invalid(format!("the token endpoint answered {status} without an access token")));
    }

    Ok(Token {
        expires_at: parsed.expires_in.map(|seconds| now() + seconds),
        access_token: parsed.access_token,
        token_type: if parsed.token_type.is_empty() { "Bearer".into() } else { parsed.token_type },
        refresh_token: parsed.refresh_token,
        scope: parsed.scope,
    })
}

/// The machine-to-machine grant: one POST, no browser.
pub async fn client_credentials(config: &Config) -> Result<Token> {
    let mut form = vec![("grant_type".to_string(), "client_credentials".to_string())];
    if !config.scope.trim().is_empty() {
        form.push(("scope".into(), config.scope.trim().to_string()));
    }
    if !config.audience.trim().is_empty() {
        form.push(("audience".into(), config.audience.trim().to_string()));
    }
    post_token(config, form).await
}

/// Trade a refresh token for a new access token.
pub async fn refresh(config: &Config, refresh_token: &str) -> Result<Token> {
    let form = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("refresh_token".to_string(), refresh_token.to_string()),
    ];
    let mut token = post_token(config, form).await?;
    // Providers often do not return the refresh token again; keep the old one
    // rather than losing the ability to refresh a second time.
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh_token.to_string());
    }
    Ok(token)
}

/// Authorization code with PKCE.
///
/// Opens the system browser, listens on loopback for the redirect, and trades
/// the code for a token. PKCE is not optional here: a desktop app cannot keep
/// a client secret, and the verifier is what makes that safe.
pub async fn authorization_code(config: &Config) -> Result<Token> {
    if config.auth_url.trim().is_empty() {
        return Err(Error::Invalid("this grant needs an authorization URL to send the browser to".into()));
    }

    let verifier = random_string(64);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_string(24);

    // Bind first, so the redirect URL names a port that is already listening.
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| Error::Invalid(format!("could not listen for the redirect: {e}")))?;
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let redirect = format!("http://127.0.0.1:{port}/callback");

    let mut url = url::Url::parse(config.auth_url.trim())
        .map_err(|e| Error::Invalid(format!("the authorization URL does not parse: {e}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", config.client_id.trim());
        query.append_pair("redirect_uri", &redirect);
        query.append_pair("state", &state);
        query.append_pair("code_challenge", &challenge);
        query.append_pair("code_challenge_method", "S256");
        if !config.scope.trim().is_empty() {
            query.append_pair("scope", config.scope.trim());
        }
        if !config.audience.trim().is_empty() {
            query.append_pair("audience", config.audience.trim());
        }
    }

    open_browser(url.as_str())?;

    // Three minutes is long enough to find a password manager and short enough
    // that a forgotten window does not hold a port forever.
    let code = tokio::time::timeout(Duration::from_secs(180), wait_for_code(&listener, &state))
        .await
        .map_err(|_| Error::Invalid("no redirect came back within three minutes".into()))??;

    let form = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code),
        ("redirect_uri".to_string(), redirect),
        ("code_verifier".to_string(), verifier),
    ];
    post_token(config, form).await
}

/// One redirect, then done. Anything else gets a page saying so.
async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| Error::Invalid(format!("the redirect could not be read: {e}")))?;

        let mut buffer = vec![0u8; 8192];
        let read = stream.read(&mut buffer).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let target = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");

        let query: Vec<(String, String)> = url::Url::parse(&format!("http://127.0.0.1{target}"))
            .map(|url| url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect())
            .unwrap_or_default();
        let value = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());

        if let Some(error) = value("error") {
            let detail = value("error_description").unwrap_or_default();
            reply(&mut stream, "Authorization was refused. You can close this tab.").await;
            return Err(Error::Invalid(format!(
                "{error}{}{detail}",
                if detail.is_empty() { "" } else { ": " }
            )));
        }

        let Some(code) = value("code") else {
            reply(&mut stream, "Waiting for the authorization redirect…").await;
            continue;
        };
        if value("state").as_deref() != Some(state) {
            reply(&mut stream, "That redirect did not match this request. Nothing was accepted.").await;
            return Err(Error::Invalid(
                "the redirect came back with the wrong state, so it was not accepted".into(),
            ));
        }

        reply(&mut stream, "Signed in. You can close this tab and go back to volt.").await;
        return Ok(code);
    }
}

async fn reply(stream: &mut tokio::net::TcpStream, message: &str) {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>volt</title>\
         <body style=\"font:16px/1.6 system-ui;display:grid;place-items:center;height:80vh;margin:0\">\
         <p>{message}</p></body>"
    );
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

/// The system browser, through whatever this platform calls it. No plugin for
/// one line per platform.
fn open_browser(url: &str) -> Result<()> {
    let result = if cfg!(target_os = "windows") {
        // NOT `cmd /C start`: cmd would parse the URL, and an authorization URL
        // is full of `&`. Everything after the first one was being cut off and
        // then run as a command. rundll32 is not a shell, so the URL arrives as
        // one argument with `&`, `|`, `^` and `%` all literal.
        std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    result.map(|_| ()).map_err(|e| Error::Invalid(format!("could not open the browser: {e}")))
}

/// Unreserved characters only, so it is safe in a URL without encoding.
///
/// From the OS random source, not from the splitmix64 in `vars`. That one is seeded
/// from the clock for test data; a PKCE verifier recoverable from the `state`
/// sitting beside it in a proxy log is a verifier that protects nothing.
fn random_string(length: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    // 3 * 66 = 198: rejecting above it keeps every character equally likely,
    // rather than making the first 58 of them slightly more so.
    const LIMIT: u8 = 198;

    let mut out = String::with_capacity(length);
    let mut buffer = [0u8; 32];
    while out.len() < length {
        getrandom::fill(&mut buffer).expect("the operating system's random source");
        for byte in buffer {
            if byte >= LIMIT {
                continue;
            }
            out.push(ALPHABET[(byte % ALPHABET.len() as u8) as usize] as char);
            if out.len() == length {
                break;
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    fn config(token_url: &str) -> Config {
        Config {
            token_url: token_url.into(),
            auth_url: String::new(),
            client_id: "volt".into(),
            client_secret: "shh".into(),
            scope: "read write".into(),
            audience: String::new(),
            basic_auth: false,
        }
    }

    /// A token endpoint on loopback, so the grant is exercised end to end
    /// rather than mocked at the seam that matters.
    async fn token_endpoint(answer: &'static str, status: u16) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 4096];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();

            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                answer.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(answer.as_bytes()).await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
            request
        });

        (format!("http://127.0.0.1:{port}/token"), handle)
    }

    #[tokio::test]
    async fn client_credentials_sends_the_grant_and_reads_the_token() {
        let (url, sent) = token_endpoint(
            r#"{"access_token":"at-1","token_type":"Bearer","expires_in":3600,"scope":"read"}"#,
            200,
        )
        .await;

        let token = client_credentials(&config(&url)).await.expect("a token");
        assert_eq!(token.access_token, "at-1");
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_at.is_some_and(|at| at > now()), "an expiry becomes a moment, not a duration");

        let request = sent.await.unwrap();
        assert!(request.contains("grant_type=client_credentials"), "{request}");
        assert!(request.contains("scope=read+write"), "{request}");
        assert!(request.contains("client_secret=shh"), "the secret goes in the form unless asked otherwise");
    }

    #[tokio::test]
    async fn basic_auth_puts_the_client_in_the_header_instead() {
        let (url, sent) = token_endpoint(r#"{"access_token":"at-2"}"#, 200).await;
        let mut config = config(&url);
        config.basic_auth = true;

        let token = client_credentials(&config).await.unwrap();
        assert_eq!(token.token_type, "Bearer", "a provider that omits the type still means Bearer");

        let request = sent.await.unwrap();
        assert!(request.to_lowercase().contains("authorization: basic"), "{request}");
        assert!(!request.contains("client_secret"), "and not in the form as well: {request}");
    }

    #[tokio::test]
    async fn a_refusal_reads_as_one() {
        let (url, _sent) = token_endpoint(
            r#"{"error":"invalid_client","error_description":"Client authentication failed"}"#,
            401,
        )
        .await;

        let error = client_credentials(&config(&url)).await.unwrap_err().to_string();
        assert!(error.contains("invalid_client"), "{error}");
        assert!(error.contains("Client authentication failed"), "{error}");
    }

    #[tokio::test]
    async fn refreshing_keeps_the_refresh_token_when_the_provider_does_not_return_one() {
        let (url, sent) = token_endpoint(r#"{"access_token":"at-3"}"#, 200).await;

        let token = refresh(&config(&url), "rt-1").await.unwrap();
        assert_eq!(token.access_token, "at-3");
        assert_eq!(token.refresh_token.as_deref(), Some("rt-1"), "losing it would end the chain");

        assert!(sent.await.unwrap().contains("grant_type=refresh_token"));
    }

    #[test]
    fn the_authorization_grant_needs_somewhere_to_send_the_browser() {
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(authorization_code(&config("http://127.0.0.1:1/token")))
            .unwrap_err()
            .to_string();
        assert!(error.contains("authorization URL"), "{error}");
    }

    #[test]
    fn a_verifier_is_long_and_url_safe() {
        let verifier = random_string(64);
        assert_eq!(verifier.len(), 64);
        assert!(verifier.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)), "{verifier}");
        assert_ne!(verifier, random_string(64));
    }

    /// Not a randomness test — a source test. The old code drew from the
    /// clock-seeded generator, so two verifiers made in the same millisecond
    /// were related and the `state` beside them gave the verifier away.
    #[test]
    fn a_verifier_is_unguessable_and_url_safe() {
        const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

        let made: Vec<String> = (0..64).map(|_| random_string(43)).collect();
        assert!(made.iter().all(|s| s.chars().count() == 43));
        assert!(
            made.iter().all(|s| s.chars().all(|c| ALPHABET.contains(c))),
            "unreserved characters only, so no encoding is needed in the URL"
        );

        let distinct: std::collections::HashSet<&String> = made.iter().collect();
        assert_eq!(distinct.len(), made.len(), "64 verifiers in a row, all different");

        // A clock-seeded source makes neighbours share a long prefix; the OS
        // source does not.
        let shared = made.windows(2).filter(|p| p[0][..8] == p[1][..8]).count();
        assert_eq!(shared, 0, "no two consecutive verifiers start the same way");
    }
}

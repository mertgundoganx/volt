//! HTTP Digest authentication.
//!
//! Digest is a conversation, not a header you can write in advance: the server
//! sends a challenge, the client answers it. So it lives outside `plan` — the
//! plan is sent, and if a 401 comes back with a Digest challenge, `execute`
//! answers it and sends once more. Exactly once: a second 401 is a wrong
//! password, and retrying it is how accounts get locked.

use md5::Md5;
use sha2::{Digest as _, Sha256};

/// What the server asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub qop: Option<String>,
    pub algorithm: String,
}

/// Pull the Digest challenge out of a `WWW-Authenticate` header. Servers that
/// offer several schemes send several headers, or one with commas; only the
/// Digest one is ours.
pub fn challenge(header: &str) -> Option<Challenge> {
    let rest = header.trim().strip_prefix("Digest ").or_else(|| header.trim().strip_prefix("digest "))?;

    let mut found = Challenge { algorithm: "MD5".into(), ..Default::default() };
    for part in split_parameters(rest) {
        let Some((name, value)) = part.split_once('=') else { continue };
        let value = value.trim().trim_matches('"').to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "realm" => found.realm = value,
            "nonce" => found.nonce = value,
            "opaque" => found.opaque = Some(value),
            "qop" => found.qop = Some(value),
            "algorithm" => found.algorithm = value.to_uppercase(),
            _ => {}
        }
    }

    (!found.nonce.is_empty()).then_some(found)
}

/// Commas separate parameters, except inside quotes.
fn split_parameters(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for ch in text.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                current.push(ch);
            }
            ',' if !quoted => {
                parts.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// The `Authorization` value to answer a challenge with.
///
/// `cnonce` is a parameter so a test can pin it; in use it comes from the same
/// place `{{$guid}}` does.
pub fn answer(
    challenge: &Challenge,
    username: &str,
    password: &str,
    method: &str,
    path: &str,
    cnonce: &str,
) -> String {
    let sha = challenge.algorithm.starts_with("SHA-256");
    let hash = |text: &str| -> String {
        if sha {
            crate::aws::hex(&Sha256::digest(text.as_bytes()))
        } else {
            crate::aws::hex(&Md5::digest(text.as_bytes()))
        }
    };

    let ha1 = hash(&format!("{username}:{}:{password}", challenge.realm));
    // `-sess` mixes the nonces into the secret; rare, but cheap to support.
    let ha1 = if challenge.algorithm.ends_with("-SESS") {
        hash(&format!("{ha1}:{}:{cnonce}", challenge.nonce))
    } else {
        ha1
    };
    let ha2 = hash(&format!("{method}:{path}"));

    // `qop` may offer several; `auth` is the only one without a body hash.
    let qop = challenge
        .qop
        .as_deref()
        .and_then(|offered| offered.split(',').map(str::trim).find(|one| *one == "auth"));

    let (response, extra) = match qop {
        Some(qop) => (
            hash(&format!("{ha1}:{}:00000001:{cnonce}:{qop}:{ha2}", challenge.nonce)),
            format!(", qop={qop}, nc=00000001, cnonce=\"{cnonce}\""),
        ),
        None => (hash(&format!("{ha1}:{}:{ha2}", challenge.nonce)), String::new()),
    };

    let mut out = format!(
        "Digest username=\"{username}\", realm=\"{}\", nonce=\"{}\", uri=\"{path}\", response=\"{response}\"",
        challenge.realm, challenge.nonce
    );
    if challenge.algorithm != "MD5" {
        out.push_str(&format!(", algorithm={}", challenge.algorithm));
    }
    out.push_str(&extra);
    if let Some(opaque) = &challenge.opaque {
        out.push_str(&format!(", opaque=\"{opaque}\""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_challenge_is_read_commas_quotes_and_all() {
        let found = challenge(
            r#"Digest realm="test@example.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e", opaque="5ccc069c""#,
        )
        .expect("a Digest challenge");

        assert_eq!(found.realm, "test@example.com");
        assert_eq!(found.nonce, "dcd98b7102dd2f0e");
        assert_eq!(found.qop.as_deref(), Some("auth,auth-int"));
        assert_eq!(found.opaque.as_deref(), Some("5ccc069c"));
        assert_eq!(found.algorithm, "MD5", "the default when the server does not say");

        assert!(challenge("Basic realm=\"x\"").is_none(), "another scheme is not ours");
        assert!(challenge("Digest realm=\"x\"").is_none(), "and a challenge without a nonce is no challenge");
    }

    /// RFC 7616's own example: the values in the specification, so if this
    /// matches, the hashing and the ordering are right.
    #[test]
    fn it_answers_the_example_in_the_rfc() {
        let found = challenge(
            r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm=MD5, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#,
        )
        .unwrap();

        let answer = answer(&found, "Mufasa", "Circle of Life", "GET", "/dir/index.html", "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ");
        assert!(answer.contains("response=\"8ca523f5e9506fed4657c9700eebdbec\""), "{answer}");
        assert!(answer.contains("qop=auth, nc=00000001"), "{answer}");
        assert!(answer.contains("opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\""), "{answer}");
    }

    #[test]
    fn sha_256_is_used_when_the_server_asks_for_it() {
        let mut found = challenge(r#"Digest realm="r", nonce="n", qop="auth""#).unwrap();
        found.algorithm = "SHA-256".into();

        let answer = answer(&found, "u", "p", "GET", "/", "c");
        assert!(answer.contains("algorithm=SHA-256"), "{answer}");
        // 64 hex characters is a SHA-256; 32 would be an MD5.
        let response = answer.split("response=\"").nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(response.len(), 64, "{answer}");
    }
}

//! AWS Signature Version 4.
//!
//! SigV4 signs the request as it will go out — method, path, query, headers,
//! and a hash of the body — so it cannot be another `Auth` variant that `plan`
//! expands into a header. It is a step between planning and sending, and
//! `curl::export` runs the same step, which is why a copied command works
//! (for the fifteen minutes AWS allows a signature to live).
//!
//! Written against the specification rather than pulled from the AWS SDK: the
//! SDK brings a runtime, a credentials chain and a retry policy, and volt
//! wants none of those — it wants four strings the user typed.

use hmac::{Hmac, KeyInit as _, Mac};
use sha2::{Digest, Sha256};

use crate::http::{Plan, PlanBody};

type HmacSha256 = Hmac<Sha256>;

pub struct Credentials<'a> {
    pub key_id: &'a str,
    pub secret: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    pub session_token: Option<&'a str>,
}

/// The headers to add. `now` is Unix seconds, so a test can pin it.
pub fn sign(plan: &Plan, credentials: &Credentials<'_>, now: u64) -> Vec<(String, String)> {
    let stamp = timestamp(now);
    let date = &stamp[..8];

    let url = url::Url::parse(&plan.url).ok();
    let host = url.as_ref().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
    let host = match url.as_ref().and_then(|u| u.port()) {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };

    let payload = payload_hash(&plan.body);

    // Sign host, x-amz-date and x-amz-content-sha256 plus whatever the request
    // already carries: more headers signed is strictly safer, and AWS requires
    // host and the date.
    let mut headers: Vec<(String, String)> = plan
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        // Only the four this function adds itself are dropped. Stripping every
        // `x-amz-*` left the user's own — `x-amz-acl` and friends — out of
        // SignedHeaders while still sending them, and AWS rejects that.
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "authorization" | "x-amz-date" | "x-amz-content-sha256" | "x-amz-security-token"
            )
        })
        .collect();
    headers.push(("host".into(), host));
    headers.push(("x-amz-date".into(), stamp.clone()));
    // S3 is the service that requires the payload hash as a header; adding it
    // everywhere would be harmless but would stop the signature matching the
    // vectors AWS publishes, which is the only proof this code is right.
    let content_header = credentials.service.eq_ignore_ascii_case("s3");
    if content_header {
        headers.push(("x-amz-content-sha256".into(), payload.clone()));
    }
    if let Some(token) = credentials.session_token {
        headers.push(("x-amz-security-token".into(), token.to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));
    headers.dedup_by(|a, b| a.0 == b.0);

    let signed_names: Vec<&str> = headers.iter().map(|(name, _)| name.as_str()).collect();
    let signed = signed_names.join(";");

    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        plan.method.to_uppercase(),
        canonical_path(url.as_ref()),
        canonical_query(url.as_ref()),
        headers.iter().map(|(name, value)| format!("{name}:{value}\n")).collect::<String>(),
        signed,
        payload,
    );

    let scope = format!("{date}/{}/{}/aws4_request", credentials.region, credentials.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}", hex(&Sha256::digest(canonical.as_bytes())));

    let key = signing_key(credentials.secret, date, credentials.region, credentials.service);
    let signature = hex(&hmac(&key, to_sign.as_bytes()));

    let mut out = vec![
        ("x-amz-date".to_string(), stamp),
        (
            "authorization".to_string(),
            format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
                credentials.key_id
            ),
        ),
    ];
    if content_header {
        out.push(("x-amz-content-sha256".to_string(), payload));
    }
    if let Some(token) = credentials.session_token {
        out.push(("x-amz-security-token".to_string(), token.to_string()));
    }
    out
}

/// A multipart body is assembled by reqwest, boundary and all, so there is
/// nothing here to hash. AWS accepts UNSIGNED-PAYLOAD for exactly this.
fn payload_hash(body: &PlanBody) -> String {
    match body {
        PlanBody::None => hex(&Sha256::digest(b"")),
        PlanBody::Text { content, .. } => hex(&Sha256::digest(content.as_bytes())),
        // The same bytes `execute` sends, so the hash matches what arrives.
        PlanBody::UrlEncoded(pairs) => {
            hex(&Sha256::digest(crate::http::urlencoded_body(pairs).as_bytes()))
        }
        PlanBody::File { path } => match std::fs::read(path) {
            Ok(bytes) => hex(&Sha256::digest(&bytes)),
            Err(_) => "UNSIGNED-PAYLOAD".into(),
        },
        PlanBody::Multipart(_) => "UNSIGNED-PAYLOAD".into(),
    }
}

fn canonical_path(url: Option<&url::Url>) -> String {
    let path = url.map(|u| u.path()).unwrap_or("/");
    if path.is_empty() {
        "/".into()
    } else {
        path.to_string()
    }
}

/// Sorted by name, then value, each percent-encoded the strict way.
fn canonical_query(url: Option<&url::Url>) -> String {
    let Some(url) = url else { return String::new() };
    let mut pairs: Vec<(String, String)> =
        url.query_pairs().map(|(name, value)| (encode(&name), encode(&value))).collect();
    pairs.sort();
    pairs.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("&")
}

/// RFC 3986, which is stricter than a form encoder: a space is `%20`, and
/// `-_.~` are the only unreserved punctuation.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let start = format!("AWS4{secret}");
    let key = hmac(start.as_bytes(), date.as_bytes());
    let key = hmac(&key, region.as_bytes());
    let key = hmac(&key, service.as_bytes());
    hmac(&key, b"aws4_request")
}

fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `20240229T134500Z`, from Unix seconds.
fn timestamp(now: u64) -> String {
    let (year, month, day) = crate::vars::civil_from_days((now / 86_400) as i64);
    let (hours, minutes, seconds) = (now / 3600 % 24, now / 60 % 60, now % 60);
    format!("{year:04}{month:02}{day:02}T{hours:02}{minutes:02}{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(method: &str, url: &str, headers: &[(&str, &str)], body: PlanBody) -> Plan {
        Plan {
            method: method.into(),
            url: url.into(),
            headers: headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
            basic: None,
            digest: None,
            ntlm: None,
            aws: None,
            api_key_header: None,
            body,
            missing: Vec::new(),
        }
    }

    fn credentials() -> Credentials<'static> {
        Credentials {
            key_id: "AKIDEXAMPLE",
            secret: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "service",
            session_token: None,
        }
    }

    fn header<'a>(headers: &'a [(String, String)], name: &str) -> &'a str {
        headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str()).unwrap_or_default()
    }

    /// The `get-vanilla` case from AWS's own SigV4 test suite: 2015-08-30
    /// 12:36:00Z, `GET /` on `example.amazonaws.com`. If this matches, the
    /// canonical request, the scope and the key derivation are all right.
    #[test]
    fn it_matches_the_signature_aws_publishes_for_the_vanilla_case() {
        let signed = sign(
            &plan("GET", "https://example.amazonaws.com/", &[], PlanBody::None),
            &credentials(),
            1_440_938_160,
        );

        assert_eq!(header(&signed, "x-amz-date"), "20150830T123600Z");
        let expected = "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
SignedHeaders=host;x-amz-date, Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31";
        assert_eq!(header(&signed, "authorization"), expected);
    }

    #[test]
    fn a_body_is_hashed_and_a_multipart_one_is_honestly_unsigned() {
        // S3 is where the payload hash travels as a header.
        let s3 = Credentials { service: "s3", ..credentials() };
        let json = sign(
            &plan(
                "POST",
                "https://example.amazonaws.com/orders",
                &[("Content-Type", "application/json")],
                PlanBody::Text { content: "{\"a\":1}".into(), default_type: "application/json" },
            ),
            &s3,
            1_440_938_160,
        );
        assert_eq!(
            header(&json, "x-amz-content-sha256"),
            hex(&Sha256::digest(b"{\"a\":1}")),
            "the payload hash is the body"
        );
        assert!(
            header(&json, "authorization").contains("content-type;host"),
            "and the request's own headers are signed too: {}",
            header(&json, "authorization")
        );

        let form = sign(
            &plan("POST", "https://example.amazonaws.com/x", &[], PlanBody::Multipart(vec![])),
            &s3,
            1_440_938_160,
        );
        assert_eq!(header(&form, "x-amz-content-sha256"), "UNSIGNED-PAYLOAD");
    }

    #[test]
    fn the_query_is_canonical_whatever_order_it_was_written_in() {
        let one = sign(&plan("GET", "https://x.amazonaws.com/?b=2&a=1", &[], PlanBody::None), &credentials(), 0);
        let two = sign(&plan("GET", "https://x.amazonaws.com/?a=1&b=2", &[], PlanBody::None), &credentials(), 0);
        assert_eq!(header(&one, "authorization"), header(&two, "authorization"));
    }

    #[test]
    fn a_session_token_is_signed_and_sent() {
        let mut credentials = credentials();
        credentials.session_token = Some("FQoGZXIvYXdz");
        let signed = sign(&plan("GET", "https://x.amazonaws.com/", &[], PlanBody::None), &credentials, 0);

        assert_eq!(header(&signed, "x-amz-security-token"), "FQoGZXIvYXdz");
        assert!(header(&signed, "authorization").contains("x-amz-security-token"), "it has to be signed too");
    }

    #[test]
    fn strict_encoding_is_used_where_aws_wants_it() {
        assert_eq!(encode("a b+c/d~e"), "a%20b%2Bc%2Fd~e");
    }

    /// Two ways the signature used to disagree with the request that was sent.
    #[test]
    fn what_is_signed_is_what_goes_out() {
        // `x-amz-acl` is the user's own: it is sent, so it has to be signed.
        // AWS rejects a request whose SignedHeaders leaves out a header that
        // arrived.
        let plan = plan(
            "PUT",
            "https://examplebucket.s3.amazonaws.com/photo.jpg",
            &[("x-amz-acl", "private"), ("Content-Type", "text/plain")],
            PlanBody::UrlEncoded(vec![
                ("note".into(), "hello there".into()),
                ("sign".into(), "a+b&c".into()),
            ]),
        );

        // s3, because that is the service that carries the payload hash as a
        // header — which is how the hash can be checked from the outside.
        let credentials = Credentials { service: "s3", ..credentials() };
        let signed = sign(&plan, &credentials, 1369353600);

        let authorization = signed
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| value.clone())
            .expect("an Authorization header");
        assert!(
            authorization.contains("x-amz-acl"),
            "the user's own x-amz header is in SignedHeaders: {authorization}"
        );

        // And the payload hash is over the bytes `execute` writes, which encode
        // a space as %20 rather than as +.
        let body = crate::http::urlencoded_body(&[
            ("note".to_string(), "hello there".to_string()),
            ("sign".to_string(), "a+b&c".to_string()),
        ]);
        assert_eq!(body, "note=hello%20there&sign=a%2Bb%26c");
        let expected = hex(&Sha256::digest(body.as_bytes()));
        let sent = signed
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("x-amz-content-sha256"))
            .map(|(_, value)| value.clone())
            .expect("s3 carries the payload hash");
        assert_eq!(sent, expected, "the hash is of the bytes that go out");
    }
}

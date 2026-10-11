//! OAuth 1.0a (RFC 5849): a signature over the finished request.
//!
//! Like SigV4 it cannot be a header `plan` writes, because what is signed is
//! the request as it goes out — the method, the address, every query
//! parameter and every field of a urlencoded body — together with a timestamp
//! and a nonce that are new each time. So the credentials travel with the plan
//! and `sign` runs between `plan` and the send, and again when the request is
//! printed as curl so that the printed command works.
//!
//! The parameters go in the `Authorization` header, the first of the three
//! places the RFC allows and the one every provider accepts. The two HMAC
//! methods are here; `RSA-SHA1` needs a private key file rather than a secret,
//! and `PLAINTEXT` is the secret itself in a header, which is not something to
//! make easy.

use base64::Engine as _;
use hmac::{Hmac, KeyInit as _, Mac as _};

use crate::aws::encode;
use crate::http::{Plan, PlanBody};
use crate::model::OAuth1Signature;

pub struct Credentials<'a> {
    pub consumer_key: &'a str,
    pub consumer_secret: &'a str,
    /// Empty for a request made on the consumer's own behalf ("two-legged"),
    /// and for the first step of the dance, before there is a token.
    pub token: &'a str,
    pub token_secret: &'a str,
    pub method: OAuth1Signature,
}

/// The `Authorization` header for this plan.
///
/// `timestamp` and `nonce` are passed in rather than read here so that a test
/// can sign with the values a specification's example used and compare.
pub fn sign(plan: &Plan, credentials: &Credentials<'_>, timestamp: u64, nonce: &str) -> String {
    let mut oauth: Vec<(&str, String)> = vec![
        ("oauth_consumer_key", credentials.consumer_key.to_string()),
        ("oauth_nonce", nonce.to_string()),
        ("oauth_signature_method", credentials.method.name().to_string()),
        ("oauth_timestamp", timestamp.to_string()),
        ("oauth_version", "1.0".to_string()),
    ];
    // "The parameter MAY be omitted if the request is not associated with a
    // resource owner" — and an empty one that is sent has to be signed too,
    // which half of all servers then get wrong.
    if !credentials.token.is_empty() {
        oauth.push(("oauth_token", credentials.token.to_string()));
    }

    let body: &[(String, String)] = match &plan.body {
        PlanBody::UrlEncoded(pairs) => pairs,
        // Only a urlencoded body is part of the signature (§3.4.1.3.1). JSON,
        // multipart and files go out unsigned, as the RFC has it.
        _ => &[],
    };
    let base = base_string(&plan.method, &plan.url, body, &oauth);
    let signature = signature(&base, credentials.consumer_secret, credentials.token_secret, credentials.method);
    oauth.push(("oauth_signature", signature));
    oauth.sort();

    let fields: Vec<String> =
        oauth.iter().map(|(name, value)| format!("{name}=\"{}\"", encode(value))).collect();
    format!("OAuth {}", fields.join(", "))
}

/// The signature base string (§3.4.1): the method, the address without its
/// query, and every parameter from the query, the urlencoded body and the
/// `oauth_` set, each encoded, sorted, joined — and then encoded once more.
fn base_string(method: &str, url: &str, body: &[(String, String)], oauth: &[(&str, String)]) -> String {
    let (address, query) = address_and_query(url);

    let mut parameters: Vec<(String, String)> = query;
    parameters.extend(body.iter().cloned());
    parameters.extend(oauth.iter().map(|(name, value)| (name.to_string(), value.clone())));

    // Sorted by encoded name, and by encoded value where names repeat — as
    // bytes, which is what sorting the encoded strings gives.
    let mut encoded: Vec<(String, String)> =
        parameters.iter().map(|(name, value)| (encode(name), encode(value))).collect();
    encoded.sort();
    let normalized =
        encoded.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("&");

    format!("{}&{}&{}", encode(&method.to_ascii_uppercase()), encode(&address), encode(&normalized))
}

/// The base string URI (§3.4.1.2) and the query's parameters, decoded.
///
/// Scheme and host in lower case, the port only when it is not the scheme's
/// own, the path as written, and no query or fragment. A URL that does not
/// parse — one printed with a `{{variable}}` still in it — is cut at the `?`
/// by hand, so that printing never fails where sending would.
fn address_and_query(url: &str) -> (String, Vec<(String, String)>) {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
            let port = parsed.port().map(|port| format!(":{port}")).unwrap_or_default();
            let address = format!("{}://{host}{port}{}", parsed.scheme(), parsed.path());
            let query = parsed.query_pairs().map(|(name, value)| (name.into_owned(), value.into_owned())).collect();
            (address, query)
        }
        Err(_) => {
            let without_fragment = url.split_once('#').map_or(url, |(before, _)| before);
            let (address, query) = without_fragment.split_once('?').unwrap_or((without_fragment, ""));
            let query = url::form_urlencoded::parse(query.as_bytes())
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect();
            (address.to_string(), query)
        }
    }
}

/// HMAC over the base string, keyed with both secrets (§3.4.2). The `&`
/// between them is there even when the token secret is empty.
fn signature(base: &str, consumer_secret: &str, token_secret: &str, method: OAuth1Signature) -> String {
    let key = format!("{}&{}", encode(consumer_secret), encode(token_secret));
    let digest = match method {
        OAuth1Signature::HmacSha1 => {
            let mut mac = Hmac::<sha1::Sha1>::new_from_slice(key.as_bytes()).expect("HMAC takes a key of any length");
            mac.update(base.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }
        OAuth1Signature::HmacSha256 => {
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).expect("HMAC takes a key of any length");
            mac.update(base.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }
    };
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// A nonce: sixteen bytes from the OS, in hex. It only has to be unique per
/// timestamp, but it is cheap to make it unguessable as well, and a clock-
/// seeded one repeats when two requests leave in the same millisecond.
pub fn nonce() -> String {
    let mut bytes = [0u8; 16];
    // A failed random source is not a reason to refuse to send: the clock is a
    // worse nonce, not a wrong one.
    if getrandom::fill(&mut bytes).is_err() {
        return crate::vars::dynamics().remove("$guid").unwrap_or_default().replace('-', "");
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(method: &str, url: &str, body: PlanBody) -> Plan {
        Plan {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            basic: None,
            digest: None,
            ntlm: None,
            aws: None,
            oauth1: None,
            body,
            api_key_header: None,
            missing: Vec::new(),
        }
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    fn field<'a>(header: &'a str, name: &str) -> Option<&'a str> {
        header
            .strip_prefix("OAuth ")?
            .split(", ")
            .find_map(|part| part.strip_prefix(&format!("{name}=\"")))
            .and_then(|value| value.strip_suffix('"'))
    }

    /// The worked example in RFC 5849 §3.4.1.1, which is there precisely to be
    /// compared against: a query with an encoded `=`, an empty value, a name
    /// with `@` in it, the same name twice, and a body.
    #[test]
    fn the_base_string_is_the_one_the_rfc_works_out() {
        let base = base_string(
            "post",
            "http://EXAMPLE.COM:80/request?b5=%3D%253D&a3=a&c%40=&a2=r%20b",
            &pairs(&[("c2", ""), ("a3", "2 q")]),
            &[
                ("oauth_consumer_key", "9djdj82h48djs9d2".into()),
                ("oauth_token", "kkk9d7dh3k39sjv7".into()),
                ("oauth_signature_method", "HMAC-SHA1".into()),
                ("oauth_timestamp", "137131201".into()),
                ("oauth_nonce", "7d8f3e4a".into()),
            ],
        );
        assert_eq!(
            base,
            "POST&http%3A%2F%2Fexample.com%2Frequest&a2%3Dr%2520b%26a3%3D2%2520q%26a3%3Da%26b5%3D%253D%25253D\
             %26c%2540%3D%26c2%3D%26oauth_consumer_key%3D9djdj82h48djs9d2%26oauth_nonce%3D7d8f3e4a\
             %26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D137131201%26oauth_token%3Dkkk9d7dh3k39sjv7"
        );
    }

    /// §3.4.1.2's own examples of what the address becomes.
    #[test]
    fn the_address_drops_a_default_port_and_keeps_any_other() {
        assert_eq!(address_and_query("HTTP://EXAMPLE.COM:80/r%20v/X?id=123").0, "http://example.com/r%20v/X");
        assert_eq!(address_and_query("https://www.example.net:8080/?q=1").0, "https://www.example.net:8080/");
        assert_eq!(address_and_query("https://api.test:443/a#frag").0, "https://api.test/a");
        // Printed with a variable still in it: cut by hand rather than refused.
        let (address, query) = address_and_query("{{baseUrl}}/users?page=2#top");
        assert_eq!(address, "{{baseUrl}}/users");
        assert_eq!(query, pairs(&[("page", "2")]));
    }

    /// The request Twitter's "Creating a signature" page signs step by step.
    #[test]
    fn it_matches_the_signature_twitter_documents() {
        let plan = plan(
            "POST",
            "https://api.twitter.com/1.1/statuses/update.json?include_entities=true",
            PlanBody::UrlEncoded(pairs(&[("status", "Hello Ladies + Gentlemen, a signed OAuth request!")])),
        );
        let credentials = Credentials {
            consumer_key: "xvz1evFS4wEEPTGEFPHBog",
            consumer_secret: "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw",
            token: "370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb",
            token_secret: "LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE",
            method: OAuth1Signature::HmacSha1,
        };
        let header = sign(&plan, &credentials, 1318622958, "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg");

        assert_eq!(field(&header, "oauth_signature"), Some("hCtSmYh%2BiHYCEqBWrE7C7hYmtUk%3D"), "{header}");
        assert_eq!(field(&header, "oauth_consumer_key"), Some("xvz1evFS4wEEPTGEFPHBog"));
        assert_eq!(field(&header, "oauth_signature_method"), Some("HMAC-SHA1"));
        assert_eq!(field(&header, "oauth_version"), Some("1.0"));
        assert_eq!(field(&header, "oauth_token"), Some("370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb"));
        assert!(!header.contains("status"), "the request's own parameters are signed, not repeated: {header}");
    }

    #[test]
    fn a_request_without_a_token_signs_with_an_empty_secret_and_sends_no_token() {
        let plan = plan("GET", "https://api.test/items?b=2&a=1", PlanBody::None);
        let credentials = Credentials {
            consumer_key: "key",
            consumer_secret: "sec ret",
            token: "",
            token_secret: "",
            method: OAuth1Signature::HmacSha256,
        };
        let header = sign(&plan, &credentials, 1700000000, "abc");
        assert!(field(&header, "oauth_token").is_none(), "{header}");
        assert_eq!(field(&header, "oauth_signature_method"), Some("HMAC-SHA256"));

        // Worked out by hand from the RFC's steps, with an HMAC made here.
        let base = "GET&https%3A%2F%2Fapi.test%2Fitems&a%3D1%26b%3D2%26oauth_consumer_key%3Dkey%26oauth_nonce%3Dabc\
                    %26oauth_signature_method%3DHMAC-SHA256%26oauth_timestamp%3D1700000000%26oauth_version%3D1.0";
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"sec%20ret&").unwrap();
        mac.update(base.as_bytes());
        let expected = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        assert_eq!(field(&header, "oauth_signature").map(crate::wire::percent_decode), Some(expected));
    }

    #[test]
    fn a_json_body_is_not_part_of_the_signature_and_a_form_is() {
        let credentials = Credentials {
            consumer_key: "key",
            consumer_secret: "secret",
            token: "token",
            token_secret: "token-secret",
            method: OAuth1Signature::HmacSha1,
        };
        let sign_with = |body: PlanBody| {
            let header = sign(&plan("POST", "https://api.test/items", body), &credentials, 1, "n");
            field(&header, "oauth_signature").unwrap().to_string()
        };
        let bare = sign_with(PlanBody::None);
        let json = sign_with(PlanBody::Text { content: r#"{"a":1}"#.into(), default_type: "application/json" });
        let form = sign_with(PlanBody::UrlEncoded(pairs(&[("a", "1")])));
        assert_eq!(bare, json, "the RFC signs form parameters and nothing else of a body");
        assert_ne!(bare, form);
    }

    #[test]
    fn a_nonce_is_never_the_same_twice() {
        let made: std::collections::HashSet<String> = (0..64).map(|_| nonce()).collect();
        assert_eq!(made.len(), 64);
        assert!(made.iter().all(|nonce| nonce.len() == 32 && nonce.chars().all(|c| c.is_ascii_hexdigit())));
    }
}

//! The cookie jar.
//!
//! reqwest ships one, but it cannot be read back or emptied, and a jar you
//! cannot look inside is a debugging trap: "why am I still logged in" has no
//! answer. This is a small RFC 6265 store — enough for API work — that can be
//! listed, deleted from and cleared.
//!
//! Jars live in memory only. A session cookie is a credential, and volt's
//! whole premise is that a collection is safe to commit, so nothing here
//! touches the disk. Closing the app signs you out.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// Without a leading dot. `host_only` says whether subdomains match.
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub host_only: bool,
    /// Unix seconds, or `None` for a session cookie.
    pub expires: Option<u64>,
}

impl Cookie {
    fn matches(&self, host: &str, path: &str, secure: bool, now: u64) -> bool {
        if self.secure && !secure {
            return false;
        }
        if self.expires.is_some_and(|at| at <= now) {
            return false;
        }
        domain_matches(host, &self.domain, self.host_only) && path_matches(path, &self.path)
    }

    /// Same name, domain and path is the same cookie, whatever its value.
    fn is(&self, other: &Cookie) -> bool {
        self.name == other.name && self.domain == other.domain && self.path == other.path
    }
}

#[derive(Default, Debug)]
pub struct Jar(Mutex<Vec<Cookie>>);

impl Jar {
    pub fn list(&self) -> Vec<Cookie> {
        let now = now();
        let mut cookies: Vec<Cookie> =
            self.0.lock().expect("jar").iter().filter(|c| !c.expires.is_some_and(|at| at <= now)).cloned().collect();
        cookies.sort_by_key(|c| (c.domain.clone(), c.path.clone(), c.name.clone()));
        cookies
    }

    pub fn remove(&self, name: &str, domain: &str, path: &str) {
        self.0.lock().expect("jar").retain(|c| !(c.name == name && c.domain == domain && c.path == path));
    }

    pub fn clear(&self) {
        self.0.lock().expect("jar").clear();
    }

    fn store(&self, cookie: Cookie) {
        let mut jar = self.0.lock().expect("jar");
        // A cookie already in the past is how a server deletes one.
        if cookie.expires.is_some_and(|at| at <= now()) {
            jar.retain(|c| !c.is(&cookie));
            return;
        }
        match jar.iter_mut().find(|c| c.is(&cookie)) {
            Some(existing) => *existing = cookie,
            None => jar.push(cookie),
        }
    }
}

impl reqwest::cookie::CookieStore for Jar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>, url: &url::Url) {
        for header in headers {
            let Ok(text) = header.to_str() else { continue };
            if let Some(cookie) = parse(text, url) {
                self.store(cookie);
            }
        }
    }

    fn cookies(&self, url: &url::Url) -> Option<reqwest::header::HeaderValue> {
        let host = url.host_str()?.to_lowercase();
        let path = url.path();
        let secure = url.scheme() == "https";
        let now = now();

        let jar = self.0.lock().expect("jar");
        let mut hits: Vec<&Cookie> = jar.iter().filter(|c| c.matches(&host, path, secure, now)).collect();
        // RFC 6265: longer paths first, so the more specific value wins.
        hits.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        if hits.is_empty() {
            return None;
        }

        let header = hits.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ");
        reqwest::header::HeaderValue::from_str(&header).ok()
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `name=value; Domain=x; Path=/y; Max-Age=60; Expires=…; Secure; HttpOnly`
fn parse(header: &str, url: &url::Url) -> Option<Cookie> {
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let host = url.host_str()?.to_lowercase();
    let mut cookie = Cookie {
        name: name.to_string(),
        value: value.trim().to_string(),
        domain: host.clone(),
        path: default_path(url.path()),
        secure: false,
        http_only: false,
        host_only: true,
        expires: None,
    };

    // Max-Age wins over Expires, as the spec says.
    let mut max_age: Option<i64> = None;
    for attribute in parts {
        let (key, value) = attribute.split_once('=').unwrap_or((attribute, ""));
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "domain" => {
                // RFC 6265: a Domain that is not the host or one of its
                // parents means the whole cookie is ignored, not just the
                // attribute. A bare `test` or `com` is refused too, since it
                // would hand the cookie to everyone. Without a public suffix
                // list this misses `co.uk`; servers you chose to talk to do
                // not try that.
                let domain = value.trim_start_matches('.').to_lowercase();
                let is_own = host == domain || host.ends_with(&format!(".{domain}"));
                if !domain.contains('.') || !is_own {
                    return None;
                }
                cookie.domain = domain;
                cookie.host_only = false;
            }
            "path" if value.starts_with('/') => cookie.path = value.to_string(),
            "max-age" => max_age = value.parse().ok(),
            "expires" => cookie.expires = parse_http_date(value),
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            _ => {}
        }
    }
    if let Some(seconds) = max_age {
        cookie.expires = Some(now().saturating_add_signed(seconds.max(-(now() as i64))));
    }

    Some(cookie)
}

/// The directory of the request path: `/a/b/c` → `/a/b`, `/a` → `/`.
fn default_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".into(),
        Some(at) => path[..at].to_string(),
    }
}

fn domain_matches(host: &str, domain: &str, host_only: bool) -> bool {
    if host == domain {
        return true;
    }
    !host_only && host.ends_with(&format!(".{domain}"))
}

fn path_matches(path: &str, cookie_path: &str) -> bool {
    if path == cookie_path {
        return true;
    }
    match path.strip_prefix(cookie_path) {
        Some(rest) => cookie_path.ends_with('/') || rest.starts_with('/'),
        None => false,
    }
}

/// `Wed, 21 Oct 2015 07:28:00 GMT`, the only shape servers actually send.
/// Anything else is treated as a session cookie rather than guessed at.
fn parse_http_date(text: &str) -> Option<u64> {
    let text = text.trim().trim_end_matches("GMT").trim();
    let rest = text.split_once(", ").map(|(_, rest)| rest).unwrap_or(text);
    let mut fields = rest.split_whitespace();
    let day: i64 = fields.next()?.parse().ok()?;
    let month_name = fields.next()?;
    let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(month_name))? as i64 + 1;
    let year: i64 = fields.next()?.parse().ok()?;
    // `days_from_civil` multiplies the era by 146_097, which overflows well
    // before i64 runs out — and a server can put any number here. An HTTP date
    // has four digits, so anything outside this is not a date at all.
    if !(0..=9999).contains(&year) || !(1..=31).contains(&day) {
        return None;
    }

    let mut clock = fields.next()?.split(':');
    let hours: u64 = clock.next()?.parse().ok()?;
    let minutes: u64 = clock.next()?.parse().ok()?;
    let seconds: u64 = clock.next().unwrap_or("0").parse().ok()?;

    // Checked throughout: a server is free to send `Expires=Sat, 01 Jan
    // 999999999 00:00:00 GMT`, and in a release build the overflow would wrap
    // silently into a date in the past, deleting the cookie instead of keeping
    // it. In a debug build it panics outright.
    let days = days_from_civil(year, month, day);
    let seconds_from_days = days.checked_mul(86_400)?;
    u64::try_from(seconds_from_days)
        .ok()?
        .checked_add(hours.checked_mul(3600)?)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)
}

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Howard Hinnant's `days_from_civil`: days since 1970-01-01.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::cookie::CookieStore as _;

    fn url(text: &str) -> url::Url {
        url::Url::parse(text).unwrap()
    }

    fn set(jar: &Jar, at: &str, headers: &[&str]) {
        let values: Vec<reqwest::header::HeaderValue> = headers.iter().map(|h| reqwest::header::HeaderValue::from_str(h).unwrap()).collect();
        jar.set_cookies(&mut values.iter(), &url(at));
    }

    fn sent(jar: &Jar, to: &str) -> String {
        jar.cookies(&url(to)).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
    }

    #[test]
    fn a_cookie_comes_back_on_the_next_request_to_the_same_host() {
        let jar = Jar::default();
        set(&jar, "https://api.test/login", &["session=abc; Path=/; HttpOnly"]);

        assert_eq!(sent(&jar, "https://api.test/orders"), "session=abc");
        assert_eq!(sent(&jar, "https://other.test/orders"), "", "and nowhere else");
        assert!(jar.list()[0].http_only);
    }

    #[test]
    fn the_domain_attribute_widens_to_subdomains_but_not_to_strangers() {
        let jar = Jar::default();
        set(&jar, "https://api.shop.test/x", &["a=1; Domain=shop.test", "b=2"]);
        // Nobody gets to set a cookie for a domain that is not theirs.
        set(&jar, "https://api.shop.test/x", &["evil=1; Domain=test", "worse=1; Domain=elsewhere.test"]);

        assert_eq!(sent(&jar, "https://www.shop.test/x"), "a=1", "Domain= reaches siblings");
        assert_eq!(sent(&jar, "https://api.shop.test/x"), "a=1; b=2", "a host-only cookie stays home");
        assert!(jar.list().iter().all(|c| c.domain.ends_with("shop.test")), "{:?}", jar.list());
    }

    #[test]
    fn paths_and_secure_are_respected() {
        let jar = Jar::default();
        set(&jar, "https://api.test/admin/panel", &["deep=1"]);
        set(&jar, "https://api.test/", &["top=1; Path=/", "tls=1; Path=/; Secure"]);

        assert_eq!(sent(&jar, "https://api.test/admin/panel/users"), "deep=1; top=1; tls=1");
        assert_eq!(sent(&jar, "https://api.test/other"), "top=1; tls=1", "a deeper path is not sent up");
        assert_eq!(sent(&jar, "http://api.test/other"), "top=1", "Secure means https only");
    }

    #[test]
    fn expiry_removes_a_cookie_and_that_is_how_logout_works() {
        let jar = Jar::default();
        set(&jar, "https://api.test/", &["session=abc; Max-Age=60", "old=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT"]);
        assert_eq!(sent(&jar, "https://api.test/"), "session=abc", "the past one never lands");

        set(&jar, "https://api.test/", &["session=abc; Max-Age=0"]);
        assert_eq!(sent(&jar, "https://api.test/"), "", "Max-Age=0 signs you out");
        assert!(jar.list().is_empty());
    }

    #[test]
    fn a_later_value_replaces_the_same_cookie_rather_than_piling_up() {
        let jar = Jar::default();
        set(&jar, "https://api.test/", &["t=one; Path=/"]);
        set(&jar, "https://api.test/", &["t=two; Path=/"]);

        assert_eq!(jar.list().len(), 1);
        assert_eq!(sent(&jar, "https://api.test/"), "t=two");

        jar.remove("t", "api.test", "/");
        assert!(jar.list().is_empty(), "and it can be taken out by hand");
    }

    #[test]
    fn http_dates_are_read_the_way_servers_write_them() {
        assert_eq!(parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"), Some(1_445_412_480));
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_http_date("29 Feb 2024 00:00:00 GMT"), Some(1_709_164_800));
        assert_eq!(parse_http_date("whenever"), None, "a session cookie, not a guess");
    }

    /// A server is free to send any date it likes. Unchecked, the multiply
    /// wrapped in release and panicked in debug — and a panic inside a command
    /// takes the whole app down.
    #[test]
    fn an_absurd_expiry_is_refused_rather_than_overflowing() {
        for text in [
            "Sat, 01 Jan 999999999999 00:00:00 GMT",
            "Sat, 01 Jan -999999999999 00:00:00 GMT",
            "Sat, 01 Jan 9223372036854775807 00:00:00 GMT",
        ] {
            assert_eq!(parse_http_date(text), None, "refused rather than wrapped: {text}");
        }

        // A four-digit year is what an HTTP date has; anything else is not one.
        assert_eq!(parse_http_date("Mon, 99999999 Dec 99999999 99999999:00:00 GMT"), None);
        assert!(parse_http_date("Fri, 31 Dec 9999 23:59:59 GMT").is_some(), "the far end still reads");

        // And the ordinary ones still read.
        assert_eq!(parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"), Some(1_445_412_480));
    }
}

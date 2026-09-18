//! `{{variable}}` interpolation.
//!
//! Values may themselves contain variables, so we run repeated passes until
//! the string stops changing or we hit `MAX_DEPTH` (which is what stops a
//! self-referencing variable from looping forever).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DEPTH: usize = 5;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Interpolated {
    pub value: String,
    /// Names that had no definition. They are left verbatim in `value` so the
    /// user sees `{{token}}` in the request rather than a silent empty string.
    pub missing: Vec<String>,
}

pub fn interpolate(input: &str, ctx: &HashMap<String, String>) -> Interpolated {
    let mut current = input.to_string();
    let mut missing: Vec<String> = Vec::new();

    for _ in 0..MAX_DEPTH {
        let (next, changed) = pass(&current, ctx, &mut missing);
        current = next;
        if !changed {
            break;
        }
    }

    missing.sort();
    missing.dedup();
    Interpolated { value: current, missing }
}

fn pass(input: &str, ctx: &HashMap<String, String>, missing: &mut Vec<String>) -> (String, bool) {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut changed = false;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find("}}") {
                let raw = &input[i + 2..i + 2 + end];
                let name = raw.trim();
                match ctx.get(name) {
                    Some(value) => {
                        out.push_str(value);
                        changed = true;
                    }
                    None => {
                        // Leave the placeholder in place and report it.
                        out.push_str(&input[i..i + 2 + end + 2]);
                        if !name.is_empty() {
                            missing.push(name.to_string());
                        }
                    }
                }
                i += 2 + end + 2;
                continue;
            }
        }

        // Not a placeholder: copy one character (not one byte) across.
        let ch = input[i..].chars().next().expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }

    (out, changed)
}

// ---------------------------------------------------------------------------
// Values volt supplies itself
// ---------------------------------------------------------------------------

/// The variables a collection does not have to define: `{{$timestamp}}` (Unix
/// seconds), `{{$isoTimestamp}}`, `{{$guid}}` and `{{$randomInt}}` (0–1000).
///
/// They are generated once per resolution rather than once per occurrence, so
/// two uses of `{{$guid}}` in one request agree with each other — an id in the
/// body and the header that refers to it would otherwise disagree. A variable
/// the user defines under the same name wins, because `env_context` lays the
/// environment on top of these.
///
/// The randomness is for test data. It is not unguessable and must never be
/// used for anything that needs to be.
pub fn dynamics() -> HashMap<String, String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    HashMap::from([
        ("$timestamp".to_string(), now.to_string()),
        ("$isoTimestamp".to_string(), iso8601(now)),
        ("$guid".to_string(), guid()),
        ("$randomInt".to_string(), (random_u64() % 1001).to_string()),
    ])
}

/// splitmix64 over the clock and a counter, so two calls in the same
/// nanosecond still differ.
fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let mut x = nanos.wrapping_add(COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed));
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A version 4 UUID in the usual 8-4-4-4-12 shape.
fn guid() -> String {
    let (hi, lo) = (random_u64(), random_u64());
    let hi = (hi & 0xFFFF_FFFF_FFFF_0FFF) | 0x0000_0000_0000_4000; // version 4
    let lo = (lo & 0x3FFF_FFFF_FFFF_FFFF) | 0x8000_0000_0000_0000; // variant 1
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xFFFF,
        hi & 0xFFFF,
        lo >> 48,
        lo & 0xFFFF_FFFF_FFFF
    )
}

fn iso8601(secs: u64) -> String {
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's `civil_from_days`, for days since 1970-01-01. Shorter and
/// more obvious than pulling in a date crate for one format string.
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }


    #[test]
    fn supplies_its_own_variables_without_reporting_them_missing() {
        let out = interpolate("{{$timestamp}}/{{$randomInt}}", &dynamics());
        assert!(out.missing.is_empty(), "{:?}", out.missing);
        let (stamp, n) = out.value.split_once('/').expect("both were substituted");
        assert!(stamp.parse::<u64>().unwrap() > 1_700_000_000, "{stamp}");
        assert!(n.parse::<u32>().unwrap() <= 1000, "{n}");
    }

    #[test]
    fn one_value_per_resolution_so_two_uses_agree() {
        let out = interpolate("{{$guid}} {{$guid}}", &dynamics());
        let (first, second) = out.value.split_once(' ').unwrap();
        assert_eq!(first, second);

        // …but a later resolution gets its own.
        assert_ne!(dynamics()["$guid"], dynamics()["$guid"]);
    }

    #[test]
    fn a_guid_has_the_shape_a_guid_has() {
        let id = guid();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12], "{id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'), "{id}");
        assert_eq!(parts[2].as_bytes()[0], b'4', "version 4: {id}");
        assert!("89ab".contains(parts[3].chars().next().unwrap()), "variant 1: {id}");
    }

    #[test]
    fn iso_timestamps_survive_leap_years_and_the_epoch() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(iso8601(1_709_251_199), "2024-02-29T23:59:59Z");
    }
    #[test]
    fn substitutes_a_plain_variable() {
        let out = interpolate("{{base}}/users", &ctx(&[("base", "http://x")]));
        assert_eq!(out.value, "http://x/users");
        assert!(out.missing.is_empty());
    }

    #[test]
    fn tolerates_spaces_inside_the_braces() {
        let out = interpolate("{{ base }}/u", &ctx(&[("base", "http://x")]));
        assert_eq!(out.value, "http://x/u");
    }

    #[test]
    fn resolves_nested_variables() {
        let out = interpolate("{{url}}", &ctx(&[("url", "{{host}}/v1"), ("host", "http://x")]));
        assert_eq!(out.value, "http://x/v1");
    }

    #[test]
    fn leaves_unknown_variables_visible_and_reports_them() {
        let out = interpolate("{{base}}/{{token}}", &ctx(&[("base", "http://x")]));
        assert_eq!(out.value, "http://x/{{token}}");
        assert_eq!(out.missing, vec!["token"]);
    }

    #[test]
    fn does_not_hang_on_a_self_referencing_variable() {
        let out = interpolate("{{a}}", &ctx(&[("a", "{{a}}")]));
        assert_eq!(out.value, "{{a}}");
    }

    #[test]
    fn keeps_multibyte_text_intact() {
        let out = interpolate("selam {{who}} ğüşiöç", &ctx(&[("who", "dünya")]));
        assert_eq!(out.value, "selam dünya ğüşiöç");
    }
}

//! Domain-name matching: a faithful port of the legacy `utils/domains.nix`
//! part-wise longest-suffix matcher.
//!
//! A subdomain "belongs to" the most-specific base domain whose labels form a
//! suffix of the subdomain's labels. Matching is done label-by-label, padding the
//! shorter label list on the **left** with `None`, then scoring each position.

/// Split a domain into its labels: `"my.example.com"` → `["my", "example", "com"]`.
pub fn get_parts(domain: &str) -> Vec<&str> {
    domain.split('.').collect()
}

/// Score one aligned label pair (`comparePart`):
/// equal → `1`; a padding (`None`) base → `0`; mismatch → `-1`.
fn compare_part(sub: Option<&str>, base: Option<&str>) -> i32 {
    if sub == base {
        1
    } else if base.is_none() {
        0
    } else {
        -1
    }
}

/// Left-pad the shorter label list with `None` so both align at the suffix
/// (`comparableParts` + `fillList`).
fn comparable_parts<'a>(
    sub: &[&'a str],
    base: &[&'a str],
) -> (Vec<Option<&'a str>>, Vec<Option<&'a str>>) {
    let some = |v: &[&'a str]| v.iter().map(|s| Some(*s)).collect::<Vec<_>>();
    match sub.len().cmp(&base.len()) {
        std::cmp::Ordering::Equal => (some(sub), some(base)),
        std::cmp::Ordering::Less => {
            let mut s = vec![None; base.len() - sub.len()];
            s.extend(some(sub));
            (s, some(base))
        }
        std::cmp::Ordering::Greater => {
            let mut b = vec![None; sub.len() - base.len()];
            b.extend(some(base));
            (some(sub), b)
        }
    }
}

/// Per-position scores for an aligned (sub, base) pair (`rate`).
fn rate(sub: &[&str], base: &[&str]) -> Vec<i32> {
    let (s, b) = comparable_parts(sub, base);
    s.iter()
        .zip(b.iter())
        .map(|(x, y)| compare_part(*x, *y))
        .collect()
}

/// Result of [`validate_sub_domain`]: whether `base` is a valid suffix of `sub`
/// and how specific the match is (higher = more specific).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Validation {
    pub valid: bool,
    pub value: i32,
}

/// Whether `base` labels are a suffix of `sub` labels, and the specificity score
/// (`validateSubDomain`). A single mismatched label makes it invalid.
pub fn validate_sub_domain(sub: &[&str], base: &[&str]) -> Validation {
    let info = rate(sub, base);
    Validation {
        valid: !info.iter().any(|&i| i < 0),
        value: info.iter().sum(),
    }
}

/// Find the most-specific base domain for `sub_domain` among `base_domains`, or
/// `None` if none matches (`getMostSpecific`). On ties the first candidate in
/// `base_domains` order wins (the legacy `foldl'` uses strict `>`).
pub fn get_most_specific<S: AsRef<str>>(sub_domain: &str, base_domains: &[S]) -> Option<String> {
    let sub_parts = get_parts(sub_domain);
    let mut best: Option<String> = None;
    let mut best_value = -1;
    for base in base_domains {
        let base = base.as_ref();
        let v = validate_sub_domain(&sub_parts, &get_parts(base));
        if v.valid && v.value > best_value {
            best_value = v.value;
            best = Some(base.to_string());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts() {
        assert_eq!(get_parts("my.example.com"), ["my", "example", "com"]);
    }

    #[test]
    fn compare() {
        assert_eq!(compare_part(Some("example"), Some("example")), 1);
        assert_eq!(compare_part(Some("org"), Some("com")), -1);
        assert_eq!(compare_part(Some("subdomain"), None), 0);
    }

    #[test]
    fn comparable_same_len() {
        let (s, b) = comparable_parts(&["example", "net"], &["example", "org"]);
        assert_eq!(s, [Some("example"), Some("net")]);
        assert_eq!(b, [Some("example"), Some("org")]);
    }

    #[test]
    fn comparable_pads_sub() {
        let (s, b) = comparable_parts(&["example", "net"], &["my", "example", "org"]);
        assert_eq!(s, [None, Some("example"), Some("net")]);
        assert_eq!(b, [Some("my"), Some("example"), Some("org")]);
    }

    #[test]
    fn comparable_pads_base() {
        let (s, b) = comparable_parts(&["my", "example", "net"], &["example", "org"]);
        assert_eq!(s, [Some("my"), Some("example"), Some("net")]);
        assert_eq!(b, [None, Some("example"), Some("org")]);
    }

    #[test]
    fn rate_domain() {
        assert_eq!(
            rate(&["subdomain", "xample", "com"], &["example", "com"]),
            [0, -1, 1]
        );
    }

    #[test]
    fn validate() {
        assert_eq!(
            validate_sub_domain(&["subdomain", "example", "com"], &["example", "com"]),
            Validation {
                valid: true,
                value: 2
            }
        );
        assert_eq!(
            validate_sub_domain(&["subdomain", "xample", "com"], &["example", "com"]),
            Validation {
                valid: false,
                value: 0
            }
        );
    }

    #[test]
    fn most_specific() {
        assert_eq!(
            get_most_specific(
                "subdomain.example.com",
                &["example.com", "subdomain.example.com"]
            ),
            Some("subdomain.example.com".to_string())
        );
        assert_eq!(
            get_most_specific("subdomain.example.com", &["xample.com"]),
            None
        );
    }
}

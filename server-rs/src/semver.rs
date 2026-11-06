//! Minimal SemVer comparison for pack versions ("must strictly increase" rule
//! on updates). Tolerant of build/pre-release suffixes: compares numeric
//! dotted core, falls back to string compare for non-numeric parts.

fn strip_v(s: &str) -> &str {
    s.trim().trim_start_matches('v')
}

/// Split "core" (numeric dots) from the pre-release/build suffix.
fn split_core(s: &str) -> (&str, Option<&str>) {
    match s.split_once('-') {
        Some((core, rest)) => (core, Some(rest)),
        None => (s.split_once('+').map_or(s, |(c, _)| c), None),
    }
}

/// Positive when `a` > `b`, negative when `a` < `b`, zero when equal.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (strip_v(a), strip_v(b));
    let (a_core, a_pre) = split_core(a);
    let (b_core, b_pre) = split_core(b);
    let mut pa = a_core.split('.');
    let mut pb = b_core.split('.');
    loop {
        match (pa.next(), pb.next()) {
            (None, None) => break,
            (x, y) => {
                let ord = match (x.and_then(parse_num), y.and_then(parse_num)) {
                    (Some(na), Some(nb)) => na.cmp(&nb),
                    // Numeric beats missing/absent; otherwise string compare.
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    _ => x.unwrap_or("").cmp(y.unwrap_or("")),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }
    // Same core: a pre-release sorts BELOW the plain release (SemVer rule).
    match (a_pre, b_pre) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

fn parse_num(s: &str) -> Option<u64> {
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn strict_increase_rules() {
        assert_eq!(compare("0.2.0", "0.1.9"), Ordering::Greater);
        assert_eq!(compare("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare("0.9.0", "1.0.0"), Ordering::Less);
        assert_eq!(compare("v2.1", "1.9.9"), Ordering::Greater);
        assert_eq!(compare("0.1.1", "0.1.1-beta"), Ordering::Greater);
    }
}

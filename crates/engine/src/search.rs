//! Search: substring, MQTT wildcard filter and regex, plus `topic_matches`.

use regex::Regex;
use std::cmp::Ordering;

/// MQTT topic filter match (`+` one level, `#` the rest, including the parent level).
/// Filters starting with a wildcard do not match topics starting with `$` (MQTT 4.7.2).
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    if topic.starts_with('$') && (filter.starts_with('+') || filter.starts_with('#')) {
        return false;
    }
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    loop {
        match (f.next(), t.next()) {
            (Some("#"), _) => return true,
            (Some("+"), Some(_)) => {}
            (Some(fs), Some(ts)) => {
                if fs != ts {
                    return false;
                }
            }
            (None, None) => return true,
            _ => return false,
        }
    }
}

/// A compiled search.
#[derive(Debug, Clone)]
pub enum Matcher {
    /// Matches everything.
    All,
    /// Matches nothing (invalid regex).
    Nothing,
    /// Case-insensitive substring.
    Substring(Regex),
    /// MQTT filter.
    Filter(String),
    /// Regex on the full path.
    Regex(Regex),
}

impl Matcher {
    /// Compiles a search string: empty = all; leading `/` = regex; contains `+` or `#` = MQTT
    /// filter; otherwise a case-insensitive substring.
    pub fn new(search: &str) -> Matcher {
        if search.is_empty() {
            Matcher::All
        } else if let Some(re) = search.strip_prefix('/') {
            match regex::RegexBuilder::new(re).size_limit(1 << 22).build() {
                Ok(r) => Matcher::Regex(r),
                Err(_) => Matcher::Nothing,
            }
        } else if search.contains(['+', '#']) {
            Matcher::Filter(search.to_string())
        } else {
            match regex::RegexBuilder::new(&regex::escape(search))
                .case_insensitive(true)
                .build()
            {
                Ok(r) => Matcher::Substring(r),
                Err(_) => Matcher::Nothing,
            }
        }
    }

    /// Tests a full topic path.
    pub fn is_match(&self, path: &str) -> bool {
        match self {
            Matcher::All => true,
            Matcher::Nothing => false,
            Matcher::Substring(r) | Matcher::Regex(r) => r.is_match(path),
            Matcher::Filter(f) => topic_matches(f, path),
        }
    }
}

/// Natural order of two segments: digit runs compare numerically ("r2" < "r10"),
/// the rest case-insensitively, ties broken by the raw bytes.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let (mut i, mut j) = (0, 0);
    while i < ab.len() && j < bb.len() {
        let (ca, cb) = (ab[i], bb[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let si = i;
            while i < ab.len() && ab[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < bb.len() && bb[j].is_ascii_digit() {
                j += 1;
            }
            let na = trim_zeros(&ab[si..i]);
            let nb = trim_zeros(&bb[sj..j]);
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            let ord = ca.to_ascii_lowercase().cmp(&cb.to_ascii_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
            i += 1;
            j += 1;
        }
    }
    (ab.len() - i).cmp(&(bb.len() - j)).then_with(|| ab.cmp(bb))
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let n = d.iter().take_while(|&&c| c == b'0').count();
    if n == d.len() {
        &d[d.len().saturating_sub(1)..]
    } else {
        &d[n..]
    }
}

/// Tree order of two full topic paths: segment by segment in natural order, parents first.
pub fn path_cmp(a: &str, b: &str) -> Ordering {
    let mut x = a.split('/');
    let mut y = b.split('/');
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(s), Some(t)) => {
                let o = natural_cmp(s, t);
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(topic_matches("#", "a/b"));
        assert!(topic_matches("a/#", "a"));
        assert!(topic_matches("a/#", "a/b/c"));
        assert!(topic_matches("a/+/c", "a/b/c"));
        assert!(!topic_matches("a/+/c", "a/b/d"));
        assert!(!topic_matches("a/+", "a/b/c"));
        assert!(topic_matches("+/+", "/b"));
        assert!(!topic_matches("#", "$SYS/x"));
        assert!(topic_matches("$SYS/#", "$SYS/x"));
        assert!(topic_matches("a//b", "a//b"));
    }

    #[test]
    fn natural() {
        assert_eq!(natural_cmp("r2", "r10"), Ordering::Less);
        assert_eq!(natural_cmp("R2", "r2"), Ordering::Less);
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert_eq!(natural_cmp("x01", "x1"), Ordering::Less);
        assert_eq!(path_cmp("a/b", "a/b/c"), Ordering::Less);
        assert_eq!(path_cmp("a/b/c", "a/c"), Ordering::Less);
    }

    #[test]
    fn matcher_modes() {
        assert!(Matcher::new("TEMP").is_match("building/f1/temperature"));
        assert!(Matcher::new("building/+/temperature").is_match("building/f1/temperature"));
        assert!(Matcher::new("/^building/f\\d$").is_match("building/f1"));
        assert!(!Matcher::new("/[unclosed").is_match("anything"));
    }
}

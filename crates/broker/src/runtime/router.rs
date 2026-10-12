//! Subscription index: a trie over topic levels with `+` and `#` wildcards and shared subscriptions.
//!
//! Topics whose first level starts with `$` are not matched by a wildcard at the first level (MQTT 4.7.2).

use std::collections::HashMap;

/// Options of one subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SubOpts {
    pub qos: u8,
    pub no_local: bool,
    pub retain_as_published: bool,
    /// 0 = send retained on subscribe, 1 = only if new, 2 = never.
    pub retain_handling: u8,
    pub sub_id: Option<usize>,
}

#[derive(Debug, Default)]
struct Node {
    children: HashMap<String, Node>,
    subs: HashMap<String, SubOpts>,
    shared: HashMap<String, SharedGroup>,
}

#[derive(Debug, Default)]
struct SharedGroup {
    members: Vec<(String, SubOpts)>,
    next: usize,
}

impl Node {
    fn is_empty(&self) -> bool {
        self.children.is_empty() && self.subs.is_empty() && self.shared.is_empty()
    }
}

/// A matched subscriber.
#[derive(Debug, Clone)]
pub(crate) struct Match {
    pub client_id: String,
    pub opts: SubOpts,
}

/// Shared subscription target groups: (group members) for the caller to pick one connected member.
#[derive(Debug, Default)]
pub(crate) struct Matches {
    pub direct: Vec<Match>,
    /// One entry per matching shared group: members in round-robin order starting at the next one.
    pub shared: Vec<Vec<Match>>,
}

/// Splits `$share/<group>/<filter>` into (group, filter).
pub(crate) fn parse_shared(filter: &str) -> Option<(&str, &str)> {
    let rest = filter.strip_prefix("$share/")?;
    let (group, f) = rest.split_once('/')?;
    if group.is_empty() || group.contains(['+', '#']) || f.is_empty() {
        return None;
    }
    Some((group, f))
}

/// Valid topic name for PUBLISH: non-empty, no wildcards, no NUL, at most 65535 bytes.
pub(crate) fn valid_topic(topic: &str) -> bool {
    !topic.is_empty() && topic.len() <= 65535 && !topic.contains(['+', '#', '\0'])
}

/// Valid topic filter: `#` only last and alone in its level, `+` alone in its level.
pub(crate) fn valid_filter(filter: &str) -> bool {
    if filter.is_empty() || filter.len() > 65535 || filter.contains('\0') {
        return false;
    }
    let levels: Vec<&str> = filter.split('/').collect();
    let last = levels.len() - 1;
    for (i, l) in levels.iter().enumerate() {
        if l.contains('#') && (*l != "#" || i != last) {
            return false;
        }
        if l.contains('+') && *l != "+" {
            return false;
        }
    }
    true
}

/// True when `topic` matches `filter` (same rules as the trie).
pub(crate) fn topic_matches(filter: &str, topic: &str) -> bool {
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    let mut first = true;
    loop {
        match (f.next(), t.next()) {
            (Some("#"), Some(tl)) => return !(first && tl.starts_with('$')),
            (Some("#"), None) => return true,
            (Some("+"), Some(tl)) => {
                if first && tl.starts_with('$') {
                    return false;
                }
            }
            (Some(fl), Some(tl)) => {
                if fl != tl {
                    return false;
                }
            }
            (None, None) => return true,
            _ => return false,
        }
        first = false;
    }
}

#[derive(Debug, Default)]
pub(crate) struct Router {
    root: Node,
    count: usize,
}

impl Router {
    /// Adds or replaces a subscription. `filter` may be a `$share/group/filter`.
    pub fn subscribe(&mut self, client_id: &str, filter: &str, opts: SubOpts) {
        let (group, f) = match parse_shared(filter) {
            Some((g, f)) => (Some(g), f),
            None => (None, filter),
        };
        let mut node = &mut self.root;
        for level in f.split('/') {
            node = node.children.entry(level.to_string()).or_default();
        }
        match group {
            None => {
                if node.subs.insert(client_id.to_string(), opts).is_none() {
                    self.count += 1;
                }
            }
            Some(g) => {
                let grp = node.shared.entry(g.to_string()).or_default();
                if let Some(m) = grp.members.iter_mut().find(|(c, _)| c == client_id) {
                    m.1 = opts;
                } else {
                    grp.members.push((client_id.to_string(), opts));
                    self.count += 1;
                }
            }
        }
    }

    /// Removes a subscription; returns true when it existed.
    pub fn unsubscribe(&mut self, client_id: &str, filter: &str) -> bool {
        let (group, f) = match parse_shared(filter) {
            Some((g, f)) => (Some(g), f),
            None => (None, filter),
        };
        let levels: Vec<&str> = f.split('/').collect();
        let removed = remove_rec(&mut self.root, &levels, client_id, group);
        if removed {
            self.count = self.count.saturating_sub(1);
        }
        removed
    }

    /// Number of subscriptions in the index.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Subscribers matching `topic`.
    pub fn matches(&mut self, topic: &str) -> Matches {
        let levels: Vec<&str> = topic.split('/').collect();
        let mut out = Matches::default();
        walk(&mut self.root, &levels, 0, &mut out);
        out
    }
}

fn remove_rec(node: &mut Node, levels: &[&str], client_id: &str, group: Option<&str>) -> bool {
    match levels.split_first() {
        None => match group {
            None => node.subs.remove(client_id).is_some(),
            Some(g) => {
                let mut removed = false;
                if let Some(grp) = node.shared.get_mut(g) {
                    let before = grp.members.len();
                    grp.members.retain(|(c, _)| c != client_id);
                    removed = grp.members.len() != before;
                    if grp.members.is_empty() {
                        node.shared.remove(g);
                    }
                }
                removed
            }
        },
        Some((first, rest)) => {
            let Some(child) = node.children.get_mut(*first) else { return false };
            let removed = remove_rec(child, rest, client_id, group);
            if child.is_empty() {
                node.children.remove(*first);
            }
            removed
        }
    }
}

fn collect(node: &mut Node, out: &mut Matches) {
    for (c, o) in &node.subs {
        out.direct.push(Match { client_id: c.clone(), opts: *o });
    }
    for grp in node.shared.values_mut() {
        if grp.members.is_empty() {
            continue;
        }
        let n = grp.members.len();
        let start = grp.next % n;
        grp.next = (start + 1) % n;
        let ordered =
            (0..n).map(|i| &grp.members[(start + i) % n]).map(|(c, o)| Match { client_id: c.clone(), opts: *o }).collect();
        out.shared.push(ordered);
    }
}

fn walk(node: &mut Node, levels: &[&str], idx: usize, out: &mut Matches) {
    if idx == levels.len() {
        collect(node, out);
        // "a/#" also matches "a".
        if let Some(h) = node.children.get_mut("#") {
            collect(h, out);
        }
        return;
    }
    let level = levels[idx];
    let dollar_root = idx == 0 && level.starts_with('$');
    if let Some(child) = node.children.get_mut(level) {
        walk(child, levels, idx + 1, out);
    }
    if dollar_root {
        return;
    }
    if level != "+" {
        if let Some(child) = node.children.get_mut("+") {
            walk(child, levels, idx + 1, out);
        }
    }
    if level != "#" {
        if let Some(h) = node.children.get_mut("#") {
            collect(h, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SubOpts {
        SubOpts { qos: 0, no_local: false, retain_as_published: false, retain_handling: 0, sub_id: None }
    }

    fn ids(m: &Matches) -> Vec<String> {
        let mut v: Vec<String> = m.direct.iter().map(|m| m.client_id.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn wildcards_and_dollar_topics() {
        let mut r = Router::default();
        r.subscribe("all", "#", opts());
        r.subscribe("plus", "a/+/c", opts());
        r.subscribe("hash", "a/#", opts());
        r.subscribe("exact", "a/b/c", opts());
        r.subscribe("sys", "$SYS/#", opts());
        assert_eq!(ids(&r.matches("a/b/c")), vec!["all", "exact", "hash", "plus"]);
        assert_eq!(ids(&r.matches("a")), vec!["all", "hash"]);
        assert_eq!(ids(&r.matches("$SYS/x")), vec!["sys"]);
        assert!(r.unsubscribe("all", "#"));
        assert!(!r.unsubscribe("all", "#"));
        assert_eq!(ids(&r.matches("x")), Vec::<String>::new());
        assert!(topic_matches("a/+/c", "a/b/c"));
        assert!(topic_matches("a/#", "a"));
        assert!(!topic_matches("#", "$SYS/a"));
        assert!(!topic_matches("+/a", "$x/a"));
        assert!(topic_matches("$x/+", "$x/a"));
        assert!(!topic_matches("a/b", "a/b/c"));
    }

    #[test]
    fn shared_round_robin() {
        let mut r = Router::default();
        r.subscribe("c1", "$share/g/t/#", opts());
        r.subscribe("c2", "$share/g/t/#", opts());
        let a = r.matches("t/x").shared[0][0].client_id.clone();
        let b = r.matches("t/x").shared[0][0].client_id.clone();
        assert_ne!(a, b);
        assert!(valid_filter("a/+/#"));
        assert!(!valid_filter("a/#/b"));
        assert!(!valid_filter("a/b+"));
        assert!(!valid_topic("a/+"));
    }
}

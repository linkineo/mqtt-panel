//! Tree view: visible rows, expansion, reveal, next changed, pinned.

use crate::engine::{Engine, TreeCache, SPARK_POINTS};
use crate::search::{path_cmp, Matcher};
use crate::store::{flags, Node, NodeId, NONE, ROOT};
use crate::types::{Filters, TreeQuery, TreeRow, TreeSlice};

/// Window for the "active" flag.
pub const ACTIVE_MS: i64 = 10_000;

/// Compiled query: search plus filters.
pub(crate) struct Compiled<'a> {
    pub matcher: Matcher,
    pub filters: &'a Filters,
    pub payload: Option<regex::bytes::Regex>,
    pub now_ms: i64,
}

impl<'a> Compiled<'a> {
    pub fn new(q: &'a TreeQuery, now_ms: i64) -> Self {
        Self::from_parts(&q.search, &q.filters, now_ms)
    }

    pub fn from_parts(search: &str, filters: &'a Filters, now_ms: i64) -> Self {
        let payload = if filters.payload_contains.is_empty() {
            None
        } else {
            regex::bytes::RegexBuilder::new(&regex::escape(&filters.payload_contains))
                .case_insensitive(true)
                .build()
                .ok()
        };
        Compiled {
            matcher: Matcher::new(search),
            filters,
            payload,
            now_ms,
        }
    }

    /// Filters only (the search is tested separately).
    pub fn passes_filters(&self, n: &Node) -> bool {
        let f = self.filters;
        if !f.is_active() {
            return true;
        }
        let Some(td) = n.topic.as_ref() else {
            return false;
        };
        let last = td.buffer.back();
        if f.retained_only && !n.has(flags::RETAINED) {
            return false;
        }
        if f.has_value && last.is_none() {
            return false;
        }
        if let Some(q) = f.qos {
            if last.map(|m| m.qos) != Some(q) {
                return false;
            }
        }
        if let Some(min) = f.changed_within_min {
            let window = (min * 60_000.0) as i64;
            if td.last_at == 0 {
                return false;
            }
            if td.last_at < self.now_ms.saturating_sub(window) {
                return false;
            }
        }
        if !f.payload_contains.is_empty() {
            match (&self.payload, last) {
                (Some(re), Some(m)) => {
                    if !re.is_match(&m.payload) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    /// Topic node matches search and filters.
    pub fn topic_matches(&self, n: &Node) -> bool {
        n.topic.is_some() && self.passes_filters(n) && self.matcher.is_match(&n.path)
    }
}

impl Engine {
    fn tree_key(&self, q: &TreeQuery, now_ms: i64) -> (TreeQuery, u64, u64, u64, i64) {
        let data_dep = q.filters.is_active();
        let time_bucket = if q.filters.changed_within_min.is_some() {
            now_ms / 1000
        } else {
            0
        };
        (
            q.clone(),
            self.structure_ver,
            self.expand_ver,
            if data_dep { self.change_seq } else { 0 },
            time_bucket,
        )
    }

    /// Makes sure the cached visible rows match the query.
    pub(crate) fn ensure_tree(&mut self, q: &TreeQuery, now_ms: i64) {
        let key = self.tree_key(q, now_ms);
        if self.tree_cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        self.ensure_sorted();
        let mut rows = self
            .tree_cache
            .take()
            .map(|c| {
                let mut r = c.rows;
                r.clear();
                r
            })
            .unwrap_or_default();
        if q.is_active() {
            self.visible_query(q, now_ms, &mut rows);
        } else {
            self.visible_plain(&mut rows);
        }
        self.tree_cache = Some(TreeCache { key, rows });
    }

    fn visible_plain(&self, rows: &mut Vec<(NodeId, bool)>) {
        let mut stack: Vec<NodeId> = self.nodes[ROOT as usize]
            .children
            .iter()
            .rev()
            .copied()
            .collect();
        while let Some(id) = stack.pop() {
            let n = &self.nodes[id as usize];
            let exp = n.has(flags::EXPANDED) && !n.children.is_empty();
            rows.push((id, exp));
            if exp {
                stack.extend(n.children.iter().rev().copied());
            }
        }
    }

    fn visible_query(&self, q: &TreeQuery, now_ms: i64, rows: &mut Vec<(NodeId, bool)>) {
        let c = Compiled::new(q, now_ms);
        let mut marked = vec![false; self.nodes.len()];
        let filters_on = q.filters.is_active();
        for (i, n) in self.nodes.iter().enumerate().skip(1) {
            let hit = if n.topic.is_some() {
                c.topic_matches(n)
            } else {
                !filters_on && !q.search.is_empty() && c.matcher.is_match(&n.path)
            };
            if hit {
                let mut cur = i as NodeId;
                while cur != NONE && cur != ROOT && !marked[cur as usize] {
                    marked[cur as usize] = true;
                    cur = self.nodes[cur as usize].parent;
                }
            }
        }
        let mut stack: Vec<NodeId> = self.nodes[ROOT as usize]
            .children
            .iter()
            .rev()
            .copied()
            .filter(|&i| marked[i as usize])
            .collect();
        while let Some(id) = stack.pop() {
            let n = &self.nodes[id as usize];
            let before = stack.len();
            stack.extend(
                n.children
                    .iter()
                    .rev()
                    .copied()
                    .filter(|&i| marked[i as usize]),
            );
            rows.push((id, stack.len() > before));
        }
    }

    pub(crate) fn tree_row(&self, id: NodeId, expanded: bool, now_ms: i64) -> TreeRow {
        let n = &self.nodes[id as usize];
        let (value, spark, spark_times) = match &n.topic {
            Some(td) => {
                let value = (!td.buffer.is_empty()).then(|| td.preview.to_string());
                let (s, t) = td
                    .default_series
                    .and_then(|i| td.series.get(i as usize))
                    .map(|s| s.last_n(SPARK_POINTS))
                    .unwrap_or_default();
                (value, s, t)
            }
            None => (None, Vec::new(), Vec::new()),
        };
        let format_hint = if n.parent == ROOT && n.name() == "spBv1.0" {
            Some("Sparkplug B".to_string())
        } else {
            None
        };
        TreeRow {
            path: n.path.to_string(),
            name: n.name().to_string(),
            depth: u32::from(n.depth),
            has_children: !n.children.is_empty(),
            expanded,
            topic_count: u64::from(n.topic_count),
            messages: n.messages,
            is_topic: n.topic.is_some(),
            value,
            retained: n.has(flags::RETAINED),
            last_received_at: n.last_at(),
            active: n
                .last_at()
                .is_some_and(|t| t >= now_ms.saturating_sub(ACTIVE_MS)),
            rate: n.rate.rate(now_ms),
            spark,
            spark_times,
            format_hint,
            pinned: n.has(flags::PINNED),
        }
    }

    /// Rows `start..start+count` of the visible tree.
    pub fn tree_slice(
        &mut self,
        q: &TreeQuery,
        start: usize,
        count: usize,
        now_ms: i64,
    ) -> TreeSlice {
        self.ensure_tree(q, now_ms);
        let cache = self
            .tree_cache
            .as_ref()
            .map(|c| c.rows.as_slice())
            .unwrap_or(&[]);
        let total = cache.len();
        let s = start.min(total);
        let e = s.saturating_add(count).min(total);
        let rows = cache[s..e]
            .iter()
            .map(|&(id, exp)| self.tree_row(id, exp, now_ms))
            .collect();
        TreeSlice {
            total: total as u64,
            start: s as u64,
            rows,
            seq: self.change_seq,
        }
    }

    fn expansion_changed(&mut self) {
        self.expand_ver += 1;
        self.bump();
    }

    /// Expands or collapses one node. Unknown paths are remembered until the node appears.
    pub fn set_expanded(&mut self, path: &str, expanded: bool) {
        match self.lookup(path) {
            Some(id) => self.nodes[id as usize].set(flags::EXPANDED, expanded),
            None => {
                if expanded {
                    self.pending_expanded.insert(path.to_string());
                } else {
                    self.pending_expanded.remove(path);
                }
            }
        }
        self.expansion_changed();
    }

    /// Expands every node above `depth` (top level is depth 0) and collapses the rest.
    pub fn expand_to_depth(&mut self, depth: usize) {
        for n in self.nodes.iter_mut().skip(1) {
            let on = (n.depth as usize) < depth && !n.children.is_empty();
            n.set(flags::EXPANDED, on);
        }
        self.pending_expanded.clear();
        self.expansion_changed();
    }

    /// Expands every node whose subtree changed within `within_ms` of `now_ms`.
    pub fn expand_changed(&mut self, within_ms: i64, now_ms: i64) {
        let since = now_ms.saturating_sub(within_ms);
        for n in self.nodes.iter_mut().skip(1) {
            if !n.children.is_empty()
                && n.last_received_at >= since
                && n.last_received_at != i64::MIN
            {
                n.set(flags::EXPANDED, true);
            }
        }
        self.expansion_changed();
    }

    /// Collapses every node.
    pub fn collapse_all(&mut self) {
        for n in self.nodes.iter_mut() {
            n.set(flags::EXPANDED, false);
        }
        self.pending_expanded.clear();
        self.expansion_changed();
    }

    /// Paths of the expanded nodes (saved with the view).
    pub fn expanded(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.has(flags::EXPANDED))
            .map(|n| n.path.to_string())
            .chain(self.pending_expanded.iter().cloned())
            .collect();
        v.sort_by(|a, b| path_cmp(a, b));
        v
    }

    /// Replaces the expansion state (restoring a saved view).
    pub fn set_expanded_set(&mut self, paths: Vec<String>) {
        for n in self.nodes.iter_mut() {
            n.set(flags::EXPANDED, false);
        }
        self.pending_expanded.clear();
        for p in paths {
            match self.lookup(&p) {
                Some(id) => self.nodes[id as usize].set(flags::EXPANDED, true),
                None => {
                    self.pending_expanded.insert(p);
                }
            }
        }
        self.expansion_changed();
    }

    /// Replaces the pinned set.
    pub fn set_pinned(&mut self, paths: Vec<String>) {
        for n in self.nodes.iter_mut() {
            n.set(flags::PINNED, false);
        }
        self.pinned_paths = paths.into_iter().collect();
        let ids: Vec<NodeId> = self
            .pinned_paths
            .iter()
            .filter_map(|p| self.lookup(p))
            .collect();
        for id in ids {
            self.nodes[id as usize].set(flags::PINNED, true);
        }
        self.table_cache = None;
        self.bump();
    }

    /// Expands the ancestors of `topic` and returns its visible row index, or -1.
    pub fn reveal_topic(&mut self, topic: &str, q: &TreeQuery) -> i64 {
        let Some(id) = self.lookup(topic) else {
            return -1;
        };
        let mut cur = self.nodes[id as usize].parent;
        let mut changed = false;
        while cur != NONE && cur != ROOT {
            let n = &mut self.nodes[cur as usize];
            if !n.has(flags::EXPANDED) {
                n.set(flags::EXPANDED, true);
                changed = true;
            }
            cur = n.parent;
        }
        if changed {
            self.expansion_changed();
        }
        let now = self.nodes[id as usize].last_received_at.max(0);
        self.ensure_tree(q, now);
        self.tree_cache
            .as_ref()
            .and_then(|c| c.rows.iter().position(|&(r, _)| r == id))
            .map_or(-1, |i| i as i64)
    }

    /// Next topic in tree order after `from` that changed within `within_ms` of `now_ms`
    /// (wraps around), for the "next changed topic" shortcut.
    pub fn next_changed(&self, from: Option<&str>, within_ms: i64, now_ms: i64) -> Option<String> {
        let since = now_ms.saturating_sub(within_ms);
        let mut cands: Vec<&str> = self
            .nodes
            .iter()
            .filter(|n| {
                n.topic
                    .as_ref()
                    .is_some_and(|td| !td.buffer.is_empty() && td.last_at >= since)
            })
            .map(|n| &*n.path)
            .collect();
        if cands.is_empty() {
            return None;
        }
        cands.sort_by(|a, b| path_cmp(a, b));
        let pick = match from {
            None => cands[0],
            Some(f) => cands
                .iter()
                .find(|c| path_cmp(c, f) == std::cmp::Ordering::Greater)
                .copied()
                .unwrap_or(cands[0]),
        };
        Some(pick.to_string())
    }

    /// Topics under `topic` (its subtree without itself), as the tree's subtopic count shows them.
    pub fn subtopic_count(&self, topic: &str) -> u64 {
        match self.lookup(topic) {
            Some(id) => {
                let n = &self.nodes[id as usize];
                u64::from(n.topic_count).saturating_sub(u64::from(n.topic.is_some()))
            }
            None => 0,
        }
    }

    /// Known topics starting with `prefix`, in tree order, at most `limit`.
    pub fn known_topics(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut v: Vec<&str> = self
            .nodes
            .iter()
            .filter(|n| n.topic.is_some() && n.path.starts_with(prefix))
            .map(|n| &*n.path)
            .collect();
        if v.len() > limit && limit > 0 {
            v.select_nth_unstable_by(limit - 1, |a, b| path_cmp(a, b));
            v.truncate(limit);
        }
        v.sort_by(|a, b| path_cmp(a, b));
        v.truncate(limit);
        v.into_iter().map(str::to_string).collect()
    }

    /// Retained topics at or under `prefix` with a non-empty payload: (topic, preview).
    pub fn retained_under(&self, prefix: &str) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .nodes
            .iter()
            .skip(1)
            .filter(|n| {
                n.has(flags::RETAINED)
                    && n.topic.is_some()
                    && (prefix.is_empty()
                        || &*n.path == prefix
                        || (n.path.starts_with(prefix)
                            && n.path.as_bytes().get(prefix.len()) == Some(&b'/'))
                        || prefix.ends_with('/') && n.path.starts_with(prefix))
            })
            .map(|n| {
                let preview = n
                    .topic
                    .as_ref()
                    .map(|t| t.preview.to_string())
                    .unwrap_or_default();
                (n.path.to_string(), preview)
            })
            .collect();
        v.sort_by(|a, b| path_cmp(&a.0, &b.0));
        v
    }

    /// Tree-order rank of every node (cached by structure version).
    pub(crate) fn ensure_rank(&mut self) {
        if self
            .rank
            .as_ref()
            .is_some_and(|(v, r)| *v == self.structure_ver && r.len() == self.nodes.len())
        {
            return;
        }
        self.ensure_sorted();
        let mut rank = vec![0u32; self.nodes.len()];
        let mut stack: Vec<NodeId> = self.nodes[ROOT as usize]
            .children
            .iter()
            .rev()
            .copied()
            .collect();
        let mut k = 0u32;
        while let Some(id) = stack.pop() {
            rank[id as usize] = k;
            k += 1;
            stack.extend(self.nodes[id as usize].children.iter().rev().copied());
        }
        self.rank = Some((self.structure_ver, rank));
    }
}

//! Table view: leaf topics flat, filtered and sorted.

use crate::engine::{Engine, TableCache, SPARK_POINTS};
use crate::store::{flags, Node, NodeId};
use crate::tree::Compiled;
use crate::types::{SortDir, TableColumn, TableQuery, TableRow, TableSlice};
use std::cmp::Ordering;

fn opt_f64_cmp(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn last_spark(n: &Node) -> Option<f64> {
    let td = n.topic.as_ref()?;
    let s = td.series.get(td.default_series? as usize)?;
    s.points.back().map(|p| p.1)
}

impl Engine {
    fn table_key(&self, q: &TableQuery, now_ms: i64) -> (TableQuery, u64, u64, i64) {
        let data_sort = !matches!(q.sort.column, TableColumn::Topic | TableColumn::Pin);
        let data_dep = q.filters.is_active() || data_sort;
        let time_dep = q.filters.changed_within_min.is_some() || q.sort.column == TableColumn::Rate;
        (
            q.clone(),
            self.structure_ver,
            if data_dep { self.change_seq } else { 0 },
            if time_dep { now_ms / 1000 } else { 0 },
        )
    }

    fn ensure_table(&mut self, q: &TableQuery, now_ms: i64) {
        let key = self.table_key(q, now_ms);
        if self.table_cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        self.ensure_rank();
        let c = Compiled::from_parts(&q.search, &q.filters, now_ms);
        let mut rows: Vec<NodeId> = self
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, n)| c.topic_matches(n))
            .map(|(i, _)| i as NodeId)
            .collect();
        let rank = self.rank.as_ref().map(|(_, r)| r.as_slice()).unwrap_or(&[]);
        let nodes = &self.nodes;
        let col = q.sort.column;
        let desc = q.sort.dir == SortDir::Desc;
        let rank_of = |id: NodeId| rank.get(id as usize).copied().unwrap_or(u32::MAX);
        let by_col = |a: &Node, b: &Node| -> Ordering {
            let ta = a.topic.as_deref();
            let tb = b.topic.as_deref();
            match col {
                TableColumn::Pin => b.has(flags::PINNED).cmp(&a.has(flags::PINNED)),
                TableColumn::Topic => Ordering::Equal,
                TableColumn::Value => {
                    let na = ta.and_then(|t| t.value_num);
                    let nb = tb.and_then(|t| t.value_num);
                    let ha = ta.is_some_and(|t| !t.buffer.is_empty());
                    let hb = tb.is_some_and(|t| !t.buffer.is_empty());
                    hb.cmp(&ha)
                        .then_with(|| nb.is_some().cmp(&na.is_some()))
                        .then_with(|| opt_f64_cmp(na, nb))
                        .then_with(|| {
                            let pa = ta.map(|t| &*t.preview).unwrap_or("");
                            let pb = tb.map(|t| &*t.preview).unwrap_or("");
                            pa.cmp(pb)
                        })
                }
                TableColumn::Spark => opt_f64_cmp(last_spark(a), last_spark(b)),
                TableColumn::Updated => ta.map(|t| t.last_at).cmp(&tb.map(|t| t.last_at)),
                TableColumn::Rate => a.rate.rate(now_ms).total_cmp(&b.rate.rate(now_ms)),
                TableColumn::Messages => a.own_messages.cmp(&b.own_messages),
                TableColumn::Qos => ta.map(|t| t.last_qos).cmp(&tb.map(|t| t.last_qos)),
                TableColumn::Retained => a.has(flags::RETAINED).cmp(&b.has(flags::RETAINED)),
                TableColumn::Size => ta.map(|t| t.last_size).cmp(&tb.map(|t| t.last_size)),
            }
        };
        if col == TableColumn::Rate {
            // Precompute rates once instead of per comparison.
            let mut keyed: Vec<(bool, f64, u32, NodeId)> = rows
                .iter()
                .map(|&id| {
                    let n = &nodes[id as usize];
                    (n.has(flags::PINNED), n.rate.rate(now_ms), rank_of(id), id)
                })
                .collect();
            keyed.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| {
                        let o = a.1.total_cmp(&b.1);
                        if desc {
                            o.reverse()
                        } else {
                            o
                        }
                    })
                    .then_with(|| a.2.cmp(&b.2))
            });
            rows = keyed.into_iter().map(|k| k.3).collect();
        } else {
            rows.sort_by(|&x, &y| {
                let a = &nodes[x as usize];
                let b = &nodes[y as usize];
                let pinned = b.has(flags::PINNED).cmp(&a.has(flags::PINNED));
                let o = by_col(a, b).then_with(|| {
                    if col == TableColumn::Topic {
                        rank_of(x).cmp(&rank_of(y))
                    } else {
                        Ordering::Equal
                    }
                });
                let o = if desc { o.reverse() } else { o };
                pinned.then(o).then_with(|| rank_of(x).cmp(&rank_of(y)))
            });
        }
        self.table_cache = Some(TableCache { key, rows });
    }

    pub(crate) fn table_row(&self, id: NodeId, now_ms: i64) -> TableRow {
        let n = &self.nodes[id as usize];
        let td = n.topic.as_deref();
        let (spark, spark_times) = td
            .and_then(|t| t.default_series.and_then(|i| t.series.get(i as usize)))
            .map(|s| s.last_n(SPARK_POINTS))
            .unwrap_or_default();
        let has_last = td.is_some_and(|t| !t.buffer.is_empty());
        TableRow {
            path: n.path.to_string(),
            value: td.filter(|_| has_last).map(|t| t.preview.to_string()),
            spark,
            spark_times,
            last_received_at: td.map(|t| t.last_at).filter(|_| n.last_at().is_some()),
            rate: n.rate.rate(now_ms),
            messages: n.own_messages,
            qos: td.filter(|_| has_last).map(|t| t.last_qos),
            retained: n.has(flags::RETAINED),
            size: td.map_or(0, |t| t.last_size),
            pinned: n.has(flags::PINNED),
        }
    }

    /// Rows `start..start+count` of the table.
    pub fn table_slice(
        &mut self,
        q: &TableQuery,
        start: usize,
        count: usize,
        now_ms: i64,
    ) -> TableSlice {
        self.ensure_table(q, now_ms);
        let ids = self
            .table_cache
            .as_ref()
            .map(|c| c.rows.as_slice())
            .unwrap_or(&[]);
        let total = ids.len();
        let s = start.min(total);
        let e = s.saturating_add(count).min(total);
        let rows = ids[s..e]
            .iter()
            .map(|&id| self.table_row(id, now_ms))
            .collect();
        TableSlice {
            total: total as u64,
            topics_in_view: self.topics as u64,
            start: s as u64,
            rows,
            seq: self.change_seq,
        }
    }
}

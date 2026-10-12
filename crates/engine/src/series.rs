//! Series: one topic plus one field path, cast to plottable values.

use crate::cast::{Cast, Rule};
use crate::types::{CastRule, SeriesData};
use std::collections::VecDeque;

const RULE_NUMBER: u8 = 1;
const RULE_PARSED: u8 = 2;
const RULE_BOOL: u8 = 4;

/// Points of one field of one topic, in receive order (never sorted).
#[derive(Debug, Clone)]
pub struct Series {
    pub path: Box<str>,
    pub points: VecDeque<(i64, f64)>,
    pub skipped: u64,
    pub first_skip: Option<Box<str>>,
    rules: u8,
    pub precision_loss: bool,
    /// Points recorded in total, before the cap dropped any.
    pub count: u64,
}

impl Series {
    /// An empty series.
    pub fn new(path: &str) -> Self {
        Series {
            path: path.into(),
            points: VecDeque::new(),
            skipped: 0,
            first_skip: None,
            rules: 0,
            precision_loss: false,
            count: 0,
        }
    }

    /// Records a cast value received at `t`, dropping the oldest point above `cap`.
    pub fn record(&mut self, t: i64, cast: &Cast, cap: usize) {
        match cast {
            Cast::Value {
                value,
                rule,
                precision_loss,
            } => {
                self.rules |= match rule {
                    Rule::Number => RULE_NUMBER,
                    Rule::ParsedNumber => RULE_PARSED,
                    Rule::Boolean => RULE_BOOL,
                };
                self.precision_loss |= *precision_loss;
                while self.points.len() >= cap.max(1) {
                    self.points.pop_front();
                }
                self.points.push_back((t, *value));
                self.count += 1;
            }
            Cast::Skip(reason) => self.skip(reason),
        }
    }

    /// Counts a skipped value.
    pub fn skip(&mut self, reason: &str) {
        self.skipped += 1;
        if self.first_skip.is_none() {
            self.first_skip = Some(reason.into());
        }
    }

    /// Drops points above a new cap.
    pub fn truncate(&mut self, cap: usize) {
        while self.points.len() > cap {
            self.points.pop_front();
        }
        self.points.shrink_to(cap);
    }

    /// Aggregated cast rule.
    pub fn cast_rule(&self) -> CastRule {
        match self.rules {
            0 => CastRule::None,
            RULE_NUMBER => CastRule::Number,
            RULE_PARSED => CastRule::ParsedNumber,
            RULE_BOOL => CastRule::Boolean,
            _ => CastRule::Mixed,
        }
    }

    /// Heap bytes owned by this series.
    pub fn heap_bytes(&self) -> usize {
        self.path.len()
            + self.points.capacity() * std::mem::size_of::<(i64, f64)>()
            + self.first_skip.as_ref().map_or(0, |s| s.len())
    }

    /// Last `n` points, oldest first.
    pub fn last_n(&self, n: usize) -> (Vec<f64>, Vec<i64>) {
        let start = self.points.len().saturating_sub(n);
        let mut v = Vec::with_capacity(n);
        let mut t = Vec::with_capacity(n);
        for &(ti, vi) in self.points.iter().skip(start) {
            v.push(vi);
            t.push(ti);
        }
        (v, t)
    }

    /// View for the UI: points since `since_ms`, with gaps.
    pub fn to_data(&self, topic: &str, since_ms: Option<i64>, gap_factor: f64) -> SeriesData {
        let points: Vec<(i64, f64)> = match since_ms {
            Some(s) => self.points.iter().copied().filter(|p| p.0 >= s).collect(),
            None => self.points.iter().copied().collect(),
        };
        let gaps = gaps(&points, gap_factor);
        let cast_rule = self.cast_rule();
        SeriesData {
            topic: topic.to_string(),
            path: self.path.to_string(),
            points,
            gaps,
            skipped: self.skipped,
            first_skip_reason: self.first_skip.as_ref().map(|s| s.to_string()),
            cast_rule,
            step: cast_rule == CastRule::Boolean,
            precision_loss: self.precision_loss,
            count: self.count,
        }
    }
}

/// Indexes `i` where a gap starts before `points[i]`: the interval exceeds `factor` times the
/// median interval.
pub fn gaps(points: &[(i64, f64)], factor: f64) -> Vec<u64> {
    if points.len() < 3 || factor <= 0.0 {
        return Vec::new();
    }
    let mut intervals: Vec<i64> = points.windows(2).map(|w| w[1].0 - w[0].0).collect();
    let mut sorted = intervals.clone();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    let median = if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    } else {
        sorted[mid] as f64
    };
    if median <= 0.0 {
        return Vec::new();
    }
    let limit = factor * median;
    let mut out = Vec::new();
    for (i, d) in intervals.drain(..).enumerate() {
        if d as f64 > limit {
            out.push((i + 1) as u64);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_detection() {
        let pts: Vec<(i64, f64)> = [0, 1000, 2000, 3000, 10000, 11000]
            .iter()
            .map(|&t| (t, 1.0))
            .collect();
        assert_eq!(gaps(&pts, 5.0), vec![4]);
        assert!(gaps(&pts, 10.0).is_empty());
    }

    #[test]
    fn cap_drops_oldest() {
        let mut s = Series::new("$");
        for i in 0..10 {
            s.record(
                i,
                &Cast::Value {
                    value: i as f64,
                    rule: Rule::Number,
                    precision_loss: false,
                },
                4,
            );
        }
        assert_eq!(s.points.len(), 4);
        assert_eq!(s.points[0], (6, 6.0));
        assert_eq!(s.count, 10);
    }
}

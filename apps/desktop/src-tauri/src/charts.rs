//! The dashboard charts of the Charts screen, as the app side knows them. The window owns them for the session
//! (`packages/ui/src/store/charts.ts`) and sends every change with `sync_charts`; the assistant's chart tools change
//! this list and send it back to the window (`AssistantEvent::Charts`). Same fields as the TypeScript `Chart`.

use serde::{Deserialize, Serialize};

pub const COLOURS: [&str; 5] = ["sage", "clay", "slate", "brick", "ink2"];
pub const STYLES: [&str; 3] = ["line", "step", "points"];
pub const AXES: [&str; 2] = ["left", "right"];
pub const CASTS: [&str; 3] = ["number", "parsedNumber", "boolean"];
pub const RANGES: [&str; 4] = ["5m", "15m", "1h", "session"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    pub id: String,
    pub name: String,
    pub topic: String,
    pub path: String,
    pub color: String,
    pub style: String,
    pub axis: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub id: String,
    pub name: String,
    pub conn_id: String,
    pub range: String,
    #[serde(default)]
    pub custom_from: Option<i64>,
    #[serde(default)]
    pub custom_to: Option<i64>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub paused_at: Option<i64>,
    #[serde(default)]
    pub series: Vec<ChartSeries>,
}

impl Chart {
    /// The start of the chart's time range at `now` (None: the whole session), as the Charts screen computes it.
    pub fn since(&self, now: i64) -> Option<i64> {
        let end = if self.paused { self.paused_at.unwrap_or(now) } else { now };
        match self.range.as_str() {
            "5m" => Some(end - 5 * 60_000),
            "15m" => Some(end - 15 * 60_000),
            "1h" => Some(end - 60 * 60_000),
            "custom" => self.custom_from,
            _ => None,
        }
    }

    /// The first colour no series uses yet, like the window's `addSeries`.
    pub fn free_colour(&self) -> String {
        COLOURS.iter().find(|c| !self.series.iter().any(|s| s.color == **c)).unwrap_or(&COLOURS[self.series.len() % COLOURS.len()]).to_string()
    }
}

/// The charts and the counter of the ids the assistant gives ("ac1", "as1"; the window uses "c1", "s1").
#[derive(Debug, Default)]
pub struct Board {
    pub charts: Vec<Chart>,
    next: u64,
}

impl Board {
    pub fn next_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("a{prefix}{}", self.next)
    }

    pub fn find(&self, needle: &str) -> Option<&Chart> {
        self.charts.iter().find(|c| c.id == needle).or_else(|| self.charts.iter().find(|c| c.name.eq_ignore_ascii_case(needle)))
    }
}

/// The window's charts after every change (session only, never on disk).
#[tauri::command]
pub fn sync_charts(state: tauri::State<'_, crate::state::AppState>, charts: Vec<Chart>) {
    crate::state::lock(&state.charts).charts = charts;
}

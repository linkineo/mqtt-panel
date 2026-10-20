//! Charts group (docs/MCP.md section 4): the dashboard charts of the Charts screen (session, `crate::charts`), the
//! topic charts under the payload and the sparklines (saved in the profile, `TopicPrefs`), and the chart export.

use super::{ask_prop, conn_prop, file_prop, schema, shown, BoxFuture, Call, Fail, Outcome, ToolDef, Wait};
use crate::charts::{Chart, ChartSeries, AXES, CASTS, COLOURS, RANGES, STYLES};
use crate::commands as cmd;
use crate::mcp::{text, Ask, AssistantEvent};
use crate::state::{lock, AppState};
use mqttpanel_engine::ChartSeriesRef;
use mqttpanel_profiles::model::{ChartField, TopicPrefs};
use serde_json::{json, Value};

macro_rules! h {
    ($f:path) => {
        |c: Call| -> BoxFuture<Result<Outcome, Fail>> { Box::pin($f(c)) }
    };
}

fn chart_prop() -> Value {
    json!({ "type": "string", "description": "The chart id from list_charts (a chart name also works)." })
}

fn series_props() -> Value {
    json!({
        "name": { "type": "string", "description": "Shown in the legend; follows the field when omitted." },
        "colour": { "type": "string", "enum": COLOURS },
        "style": { "type": "string", "enum": STYLES },
        "axis": { "type": "string", "enum": AXES },
        "unit": { "type": "string" },
        "cast": { "type": "string", "enum": CASTS, "description": "How values become numbers; omit for the rule MQTT Panel detects." }
    })
}

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_charts",
            description: "The dashboard charts of the Charts screen (id, name, connection, time range, series) and the topic charts and sparklines saved in the connections. Read-only.",
            schema: || schema(json!({}), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(list_charts),
        },
        ToolDef {
            name: "create_chart",
            description: "Create an empty dashboard chart on the Charts screen for one connection (the only connected one when omitted). range: 5m, 15m, 1h (default) or session. Then add_series and show_chart.",
            schema: || schema(json!({ "name": { "type": "string" }, "connection": conn_prop(), "range": { "type": "string", "enum": RANGES, "default": "1h" } }), &["name"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(create_chart),
        },
        ToolDef {
            name: "rename_chart",
            description: "Rename a dashboard chart.",
            schema: || schema(json!({ "id": chart_prop(), "name": { "type": "string" } }), &["id", "name"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(rename_chart),
        },
        ToolDef {
            name: "delete_chart",
            description: "Delete a dashboard chart (the data stays in the connection). Confirm with the user first.",
            schema: || schema(json!({ "id": chart_prop() }), &["id"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(delete_chart),
        },
        ToolDef {
            name: "add_series",
            description: "Add a value to a dashboard chart: a topic and the field path in its payload (\"$\" for the whole payload, \"$.temperature\", \"$.sensors[0].value\"; get_topic lists the fields that can be charted). The series fills from the history already received.",
            schema: || {
                let mut p = series_props();
                p["chart"] = chart_prop();
                p["connection"] = conn_prop();
                p["topic"] = json!({ "type": "string" });
                p["field_path"] = json!({ "type": "string", "default": "$" });
                schema(p, &["chart", "topic"])
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(add_series),
        },
        ToolDef {
            name: "update_series",
            description: "Change a series of a dashboard chart: name, colour, style, axis, unit, cast, or enabled (shown or hidden).",
            schema: || {
                let mut p = series_props();
                p["chart"] = chart_prop();
                p["series"] = json!({ "type": "string", "description": "The series id from list_charts (a series name also works)." });
                p["enabled"] = json!({ "type": "boolean" });
                schema(p, &["chart", "series"])
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(update_series),
        },
        ToolDef {
            name: "remove_series",
            description: "Remove a series from a dashboard chart.",
            schema: || schema(json!({ "chart": chart_prop(), "series": { "type": "string" } }), &["chart", "series"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(remove_series),
        },
        ToolDef {
            name: "set_topic_chart",
            description: "The fields shown on a topic's own chart, under its payload in the Explorer (saved with the connection). fields: field paths (\"$.temperature\") or objects with path, colour, style, axis, unit; fields kept keep their settings; an empty list removes the chart.",
            schema: || {
                schema(
                    json!({
                        "connection": conn_prop(),
                        "topic": { "type": "string" },
                        "fields": { "type": "array", "items": { "anyOf": [ { "type": "string" }, { "type": "object", "properties": { "path": { "type": "string" }, "colour": { "type": "string", "enum": COLOURS }, "style": { "type": "string", "enum": STYLES }, "axis": { "type": "string", "enum": AXES }, "unit": { "type": "string" } }, "required": ["path"] } ] } }
                    }),
                    &["connection", "topic", "fields"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(set_topic_chart),
        },
        ToolDef {
            name: "set_sparkline",
            description: "The value a topic's sparkline shows in the topic tree (saved with the connection): a field path, or off true to hide the sparkline.",
            schema: || schema(json!({ "connection": conn_prop(), "topic": { "type": "string" }, "path": { "type": "string" }, "off": { "type": "boolean" } }), &["connection", "topic"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(set_sparkline),
        },
        ToolDef {
            name: "export_chart",
            description: "Export a dashboard chart over its time range: csv (time plus one column per series) or png (the picture MQTT Panel draws). Recommended: leave file out, the user picks the place in MQTT Panel's save dialog; or give an absolute file path. Returns the path.",
            schema: || schema(json!({ "id": chart_prop(), "format": { "type": "string", "enum": ["csv", "png"], "default": "csv" }, "ask_in_app": ask_prop(), "file": file_prop() }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(export_chart),
        },
    ]
}

// ---------------------------------------------------------------- shared

/// list_charts and `mqttpanel://charts`.
pub fn charts_json(st: &AppState) -> Value {
    let charts = lock(&st.charts).charts.clone();
    let profiles = st.store.profiles();
    let name_of = |id: &str| profiles.iter().find(|p| p.id == id).map(|p| p.name.clone());
    let dash: Vec<Value> = charts
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "connection": c.conn_id,
                "connectionName": name_of(&c.conn_id),
                "range": c.range,
                "paused": c.paused,
                "series": c.series,
            })
        })
        .collect();
    let mut topic_charts = vec![];
    let mut sparklines = vec![];
    for p in &profiles {
        for (topic, prefs) in &p.topics {
            if let Some(f) = prefs.chart_fields.as_ref().filter(|f| !f.is_empty()) {
                topic_charts.push(json!({ "connection": p.id, "connectionName": p.name, "topic": topic, "fields": f }));
            }
            if prefs.sparkline_path.is_some() || prefs.sparkline_off {
                sparklines.push(json!({ "connection": p.id, "topic": topic, "path": prefs.sparkline_path, "off": prefs.sparkline_off }));
            }
        }
    }
    json!({ "charts": dash, "topicCharts": topic_charts, "sparklines": sparklines })
}

/// The window shows the new list of dashboard charts.
fn publish(c: &Call) {
    let charts = lock(&c.state().charts).charts.clone();
    c.link.emit(AssistantEvent::Charts { charts });
}

fn no_chart(id: &str) -> Fail {
    Fail::from(text::um("charts.noChart", &[("id", json!(id))]))
}

fn chart(c: &Call, key: &str) -> Result<Chart, Fail> {
    let id = c.str(key)?;
    lock(&c.state().charts).find(&id).cloned().ok_or_else(|| no_chart(&id))
}

/// Replaces a chart in the board and tells the window.
fn store(c: &Call, ch: Chart) {
    {
        let mut b = lock(&c.state().charts);
        if let Some(x) = b.charts.iter_mut().find(|x| x.id == ch.id) {
            *x = ch;
        }
    }
    publish(c);
}

/// "$", "$.a.b" or "a.b" as the engine writes paths; None when it is not a field path.
pub fn field_path(p: &str) -> Option<String> {
    let p = p.trim();
    let p = if p.is_empty() {
        "$".to_string()
    } else if p.starts_with('$') {
        p.to_string()
    } else if p.starts_with('[') {
        format!("${p}")
    } else {
        format!("$.{p}")
    };
    mqttpanel_engine::path::normalize(&p)
}

fn check(c: &Call, key: &str, allowed: &[&str]) -> Result<Option<String>, Fail> {
    c.opt_enum(key, allowed)
}

/// The series' looks from the arguments (only the given ones).
fn apply_looks(c: &Call, s: &mut ChartSeries) -> Result<Vec<&'static str>, Fail> {
    let mut changed = vec![];
    if let Some(n) = c.opt_str("name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        s.name = n;
        changed.push("name");
    }
    if let Some(v) = check(c, "colour", &COLOURS)? {
        s.color = v;
        changed.push("colour");
    }
    if let Some(v) = check(c, "style", &STYLES)? {
        s.style = v;
        changed.push("style");
    }
    if let Some(v) = check(c, "axis", &AXES)? {
        s.axis = v;
        changed.push("axis");
    }
    if let Some(v) = c.opt_str("unit") {
        s.unit = v;
        changed.push("unit");
    }
    if let Some(v) = check(c, "cast", &CASTS)? {
        s.cast = Some(v);
        changed.push("cast");
    }
    Ok(changed)
}

fn series_of<'a>(ch: &'a mut Chart, needle: &str) -> Option<&'a mut ChartSeries> {
    let i = ch.series.iter().position(|s| s.id == needle).or_else(|| ch.series.iter().position(|s| s.name.eq_ignore_ascii_case(needle)))?;
    ch.series.get_mut(i)
}

// ---------------------------------------------------------------- dashboard charts

async fn list_charts(c: Call) -> Result<Outcome, Fail> {
    let data = charts_json(c.state());
    let list: Vec<String> = data["charts"].as_array().map(|a| a.iter().map(|x| format!("{} ({})", x["name"].as_str().unwrap_or_default(), x["series"].as_array().map_or(0, Vec::len))).collect()).unwrap_or_default();
    let topic_n = data["topicCharts"].as_array().map_or(0, Vec::len);
    let text = if list.is_empty() {
        c.t("charts.none", &[("topicCharts", json!(topic_n))])
    } else {
        c.t("charts.list", &[("count", json!(list.len())), ("list", json!(list.join(", "))), ("topicCharts", json!(topic_n))])
    };
    Ok(Outcome::new(text, data))
}

async fn create_chart(c: Call) -> Result<Outcome, Fail> {
    let name = c.str("name")?.trim().to_string();
    let range = check(&c, "range", &RANGES)?.unwrap_or_else(|| "1h".into());
    let conn_id = match c.opt_str("connection").filter(|s| !s.trim().is_empty()) {
        Some(_) => c.profile("connection")?.profile.id,
        None => {
            let open: Vec<String> = lock(&c.state().conns).values().filter(|x| x.is_open()).map(|x| x.id.clone()).collect();
            match open.as_slice() {
                [one] => one.clone(),
                _ => return Err(Fail::args("connection is required when not exactly one connection is connected")),
            }
        }
    };
    let conn_name = c.state().store.get(&conn_id).map(|p| p.profile.name).unwrap_or_default();
    let ch = {
        let mut b = lock(&c.state().charts);
        let id = b.next_id("c");
        let ch = Chart { id, name: name.clone(), conn_id: conn_id.clone(), range, custom_from: None, custom_to: None, paused: false, paused_at: None, series: vec![] };
        b.charts.push(ch.clone());
        ch
    };
    publish(&c);
    let text = c.t("charts.created", &[("name", json!(name)), ("connection", json!(conn_name))]);
    Ok(Outcome::new(text, json!({ "id": ch.id, "chart": ch })))
}

async fn rename_chart(c: Call) -> Result<Outcome, Fail> {
    let mut ch = chart(&c, "id")?;
    let old = ch.name.clone();
    ch.name = c.str("name")?.trim().to_string();
    let text = c.t("charts.renamed", &[("old", json!(old)), ("name", json!(ch.name))]);
    let data = json!({ "id": ch.id, "name": ch.name });
    store(&c, ch);
    Ok(Outcome::new(text, data))
}

async fn delete_chart(c: Call) -> Result<Outcome, Fail> {
    let ch = chart(&c, "id")?;
    lock(&c.state().charts).charts.retain(|x| x.id != ch.id);
    publish(&c);
    Ok(Outcome::new(c.t("charts.deleted", &[("name", json!(ch.name))]), json!({ "id": ch.id, "deleted": true })))
}

async fn add_series(c: Call) -> Result<Outcome, Fail> {
    let mut ch = chart(&c, "chart")?;
    if c.opt_str("connection").is_some_and(|s| !s.trim().is_empty()) {
        let p = c.profile("connection")?.profile;
        if p.id != ch.conn_id {
            return Err(Fail::args("the chart belongs to another connection; create a chart for this one"));
        }
    }
    let topic = c.str("topic")?;
    if topic.contains('+') || topic.contains('#') {
        return Err(Fail::args("topic must be one topic, without + or #"));
    }
    let path = field_path(&c.opt_str("field_path").unwrap_or_default()).ok_or_else(|| Fail::args("field_path is not a field path like $.temperature"))?;
    if let Some(s) = ch.series.iter().find(|s| s.topic == topic && s.path == path) {
        let text = c.t("charts.seriesExists", &[("chart", json!(ch.name)), ("path", json!(path)), ("topic", json!(topic))]);
        return Ok(Outcome::new(text, json!({ "chart": ch.id, "series": s.id, "added": false })));
    }
    let data = lock(&c.state().conns).get(&ch.conn_id).map(|conn| lock(&conn.engine).series(&topic, &path, None));
    let label = mqttpanel_engine::path::label(&path);
    let default_name = if label == "value" { topic.rsplit('/').next().unwrap_or(&topic).to_string() } else { label };
    let mut s = ChartSeries {
        id: lock(&c.state().charts).next_id("s"),
        name: default_name,
        topic: topic.clone(),
        path: path.clone(),
        color: ch.free_colour(),
        style: if data.as_ref().is_some_and(|d| d.step) { "step".into() } else { "line".into() },
        axis: "left".into(),
        unit: String::new(),
        enabled: true,
        cast: None,
    };
    apply_looks(&c, &mut s)?;
    let points = data.as_ref().map_or(0, |d| d.points.len());
    let mut text = c.t("charts.seriesAdded", &[("series", json!(s.name)), ("topic", json!(topic)), ("path", json!(path)), ("chart", json!(ch.name)), ("points", json!(points))]);
    if points == 0 {
        text = format!("{text} {}", c.t("charts.noPointsYet", &[]));
    }
    let out = json!({ "chart": ch.id, "series": s.id, "added": true, "points": points, "seriesData": s.clone() });
    ch.series.push(s);
    store(&c, ch);
    Ok(Outcome::new(text, out))
}

async fn update_series(c: Call) -> Result<Outcome, Fail> {
    let mut ch = chart(&c, "chart")?;
    let needle = c.str("series")?;
    let chart_name = ch.name.clone();
    let chart_id = ch.id.clone();
    let s = series_of(&mut ch, &needle).ok_or_else(|| Fail::from(text::um("charts.noSeries", &[("chart", json!(chart_name)), ("id", json!(needle))])))?;
    let mut changed = apply_looks(&c, s)?;
    if let Some(on) = c.opt_bool("enabled") {
        s.enabled = on;
        changed.push("enabled");
    }
    let text = c.t("charts.seriesUpdated", &[("series", json!(s.name)), ("chart", json!(chart_name)), ("fields", json!(changed.join(", ")))]);
    let data = json!({ "chart": chart_id, "series": s.clone(), "changed": changed });
    store(&c, ch);
    Ok(Outcome::new(text, data))
}

async fn remove_series(c: Call) -> Result<Outcome, Fail> {
    let mut ch = chart(&c, "chart")?;
    let needle = c.str("series")?;
    let chart_name = ch.name.clone();
    let chart_id = ch.id.clone();
    let s = series_of(&mut ch, &needle).ok_or_else(|| Fail::from(text::um("charts.noSeries", &[("chart", json!(chart_name)), ("id", json!(needle))])))?.clone();
    ch.series.retain(|x| x.id != s.id);
    let text = c.t("charts.seriesRemoved", &[("series", json!(s.name)), ("chart", json!(chart_name))]);
    let data = json!({ "chart": chart_id, "removed": s.id });
    store(&c, ch);
    Ok(Outcome::new(text, data))
}

// ---------------------------------------------------------------- topic charts and sparklines

/// Changes the per-topic preferences of a profile like the window (`lib/topicPrefs.ts`): empty parts are dropped.
fn update_prefs(c: &Call, key: &str, topic: &str, f: impl FnOnce(&mut TopicPrefs)) -> Result<mqttpanel_profiles::Profile, Fail> {
    let mut p = c.profile(key)?.profile;
    let mut prefs = p.topics.get(topic).cloned().unwrap_or_default();
    f(&mut prefs);
    if prefs.sparkline_path.as_deref().is_some_and(str::is_empty) {
        prefs.sparkline_path = None;
    }
    if prefs.chart_fields.as_ref().is_some_and(Vec::is_empty) {
        prefs.chart_fields = None;
    }
    if prefs == TopicPrefs::default() {
        p.topics.remove(topic);
    } else {
        p.topics.insert(topic.to_string(), prefs);
    }
    Ok(cmd::save_profile_on(c.state(), p)?)
}

async fn set_topic_chart(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("connection")?.profile;
    let topic = c.str("topic")?;
    let items = c.args.get("fields").and_then(Value::as_array).cloned().ok_or_else(|| Fail::args("fields must be a list"))?;
    let existing = p.topics.get(&topic).and_then(|t| t.chart_fields.clone()).unwrap_or_default();
    let mut out: Vec<ChartField> = vec![];
    for it in &items {
        let (raw, obj) = match it {
            Value::String(s) => (s.clone(), None),
            Value::Object(o) => (o.get("path").and_then(Value::as_str).unwrap_or_default().to_string(), Some(o)),
            _ => return Err(Fail::args("every field is a path or an object with a path")),
        };
        let path = field_path(&raw).ok_or_else(|| Fail::args("a field is not a field path like $.temperature"))?;
        if out.iter().any(|f| f.path == path) {
            continue;
        }
        let mut f = existing.iter().find(|f| f.path == path).cloned().unwrap_or_else(|| ChartField { path: path.clone(), colour: String::new(), style: "line".into(), axis: "left".into(), unit: String::new() });
        if let Some(o) = obj {
            let pick = |k: &str, allowed: &[&str]| -> Result<Option<String>, Fail> {
                match o.get(k).and_then(Value::as_str) {
                    None => Ok(None),
                    Some(v) if allowed.is_empty() || allowed.contains(&v) => Ok(Some(v.to_string())),
                    Some(_) => Err(Fail::args(&format!("{k} must be one of {}", allowed.join(", ")))),
                }
            };
            if let Some(v) = pick("colour", &COLOURS)? {
                f.colour = v;
            }
            if let Some(v) = pick("style", &STYLES)? {
                f.style = v;
            }
            if let Some(v) = pick("axis", &AXES)? {
                f.axis = v;
            }
            if let Some(v) = pick("unit", &[])? {
                f.unit = v;
            }
        }
        out.push(f);
    }
    // New fields take the colours nobody uses yet, like the window's withFields.
    for i in 0..out.len() {
        if out[i].colour.is_empty() {
            let used: Vec<String> = out.iter().map(|f| f.colour.clone()).collect();
            out[i].colour = COLOURS.iter().find(|c| !used.iter().any(|u| u == *c)).unwrap_or(&COLOURS[i % COLOURS.len()]).to_string();
        }
    }
    let paths: Vec<String> = out.iter().map(|f| f.path.clone()).collect();
    let saved = update_prefs(&c, "connection", &topic, |t| t.chart_fields = Some(out))?;
    let text = if paths.is_empty() {
        c.t("charts.topicChartCleared", &[("topic", json!(topic))])
    } else {
        c.t("charts.topicChartSet", &[("topic", json!(topic)), ("fields", json!(paths.join(", ")))])
    };
    Ok(Outcome::new(text, json!({ "connection": saved.id, "topic": topic, "fields": saved.topics.get(&topic).and_then(|t| t.chart_fields.clone()).unwrap_or_default() })).changed("profiles"))
}

async fn set_sparkline(c: Call) -> Result<Outcome, Fail> {
    let topic = c.str("topic")?;
    let off = c.opt_bool("off").unwrap_or(false);
    let path = if off {
        None
    } else {
        let raw = c.opt_str("path").ok_or_else(|| Fail::args("give path, or off true"))?;
        Some(field_path(&raw).ok_or_else(|| Fail::args("path is not a field path like $.temperature"))?)
    };
    let saved = update_prefs(&c, "connection", &topic, |t| {
        if off {
            t.sparkline_off = true;
        } else {
            t.sparkline_path = path.clone();
            t.sparkline_off = false;
        }
    })?;
    let text = match &path {
        None => c.t("charts.sparklineOff", &[("topic", json!(topic))]),
        Some(p) => c.t("charts.sparklineSet", &[("topic", json!(topic)), ("path", json!(p))]),
    };
    Ok(Outcome::new(text, json!({ "connection": saved.id, "topic": topic, "path": path, "off": off })).changed("profiles"))
}

// ---------------------------------------------------------------- export

async fn export_chart(c: Call) -> Result<Outcome, Fail> {
    use base64::Engine as _;
    let ch = chart(&c, "id")?;
    let format = c.opt_enum("format", &["csv", "png"])?.unwrap_or_else(|| "csv".into());
    let (default_name, data) = if format == "png" {
        let a = c.ask(Ask { request: "chartPng".into(), name: ch.name.clone(), chart_id: Some(ch.id.clone()), ..Default::default() }).await?;
        let b64 = a.data.filter(|d| !d.is_empty()).ok_or_else(Fail::cancelled)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| Fail::args("the window sent no image"))?;
        let base = if ch.name.trim().is_empty() { "chart".to_string() } else { ch.name.clone() };
        (format!("{base}.png"), bytes)
    } else {
        let refs: Vec<ChartSeriesRef> = ch.series.iter().map(|s| ChartSeriesRef { name: s.name.clone(), topic: s.topic.clone(), path: s.path.clone() }).collect();
        let (name, csv) = cmd::chart_csv_on(c.state(), &ch.conn_id, &refs, ch.since(crate::types::now_ms()))?;
        (name, csv.into_bytes())
    };
    let path = c.save_target(&default_name, &ch.name).await?;
    crate::files::write_file(&path, &data)?;
    let text = c.t("charts.exported", &[("name", json!(ch.name)), ("file", json!(shown(&path)))]);
    Ok(Outcome::new(text, json!({ "id": ch.id, "format": format, "file": shown(&path), "bytes": data.len() })))
}

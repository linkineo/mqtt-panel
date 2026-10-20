//! Prompts (guided flows the host can offer) and the text of two resources, docs/MCP.md section 4: "Connect me to a
//! broker", "Why can't I connect?", "Set up the built-in broker for my devices" and "Chart this value".

use super::text::t;
use super::{AppLink, LogEntry};
use mqttpanel_client::catalog::Lang;
use serde_json::{json, Value};

pub struct PromptDef {
    pub name: &'static str,
    /// Catalog key under `mcp.prompt.`: `<key>.title`, `<key>.description`, `<key>.body`.
    pub key: &'static str,
    /// (argument name, catalog key of its description)
    pub arguments: &'static [(&'static str, &'static str)],
}

pub fn prompts() -> Vec<PromptDef> {
    vec![
        PromptDef { name: "connect_to_broker", key: "connect", arguments: &[("address", "connect.address")] },
        PromptDef { name: "why_cant_i_connect", key: "why", arguments: &[("connection", "why.connection")] },
        PromptDef { name: "setup_builtin_broker", key: "broker", arguments: &[("devices", "broker.devices"), ("security", "broker.security")] },
        PromptDef { name: "chart_this_value", key: "chart", arguments: &[("value", "chart.value"), ("connection", "chart.connection")] },
    ]
}

/// The text of a prompt: the steps for the host, in the app language, with the current state where it helps.
pub fn prompt_text(link: &dyn AppLink, name: &str, args: &serde_json::Map<String, Value>) -> Option<(String, String)> {
    let lang = link.state().assistant.lang();
    let def = prompts().into_iter().find(|p| p.name == name)?;
    let arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let mut body = t(lang, &format!("prompt.{}.body", def.key), &[]);
    match def.key {
        "connect" => {
            let a = arg("address");
            if !a.is_empty() {
                body.push_str("\n\n");
                body.push_str(&t(lang, "prompt.connect.given", &[("address", json!(a))]));
            }
        }
        "broker" => {
            for (k, given) in [("devices", "prompt.broker.givenDevices"), ("security", "prompt.broker.givenSecurity")] {
                let a = arg(k);
                if !a.is_empty() {
                    body.push_str("\n\n");
                    body.push_str(&t(lang, given, &[(k, json!(a))]));
                }
            }
            body.push_str("\n\n");
            body.push_str(&t(lang, "prompt.broker.state", &[]));
            body.push_str("\n```json\n");
            let state = super::tools::broker::status_json(link.state(), lang).unwrap_or(Value::Null);
            body.push_str(&serde_json::to_string_pretty(&state).unwrap_or_default());
            body.push_str("\n```");
        }
        "chart" => {
            let v = arg("value");
            if !v.is_empty() {
                body.push_str("\n\n");
                body.push_str(&t(lang, "prompt.chart.given", &[("value", json!(v))]));
            }
            let c = arg("connection");
            if !c.is_empty() {
                body.push_str("\n\n");
                body.push_str(&t(lang, "prompt.chart.givenConnection", &[("connection", json!(c))]));
            }
            let open: Vec<Value> = connections_json(link).as_array().cloned().unwrap_or_default().into_iter().filter(|r| r["state"] == "connected").map(|r| json!({ "id": r["id"], "name": r["name"] })).collect();
            body.push_str("\n\n");
            body.push_str(&t(lang, "prompt.chart.state", &[]));
            body.push_str("\n```json\n");
            let charts = super::tools::charts::charts_json(link.state());
            body.push_str(&serde_json::to_string_pretty(&json!({ "connected": open, "charts": charts["charts"] })).unwrap_or_default());
            body.push_str("\n```");
        }
        _ => {
            let c = arg("connection");
            let profiles = link.state().store.profiles();
            let picked: Vec<_> = profiles.iter().filter(|p| c.is_empty() || p.id == c || p.name.eq_ignore_ascii_case(&c)).collect();
            let states = connections_json(link);
            let rows: Vec<Value> = states.as_array().cloned().unwrap_or_default().into_iter().filter(|r| picked.iter().any(|p| r["id"] == json!(p.id))).collect();
            body.push_str("\n\n");
            body.push_str(&t(lang, "prompt.why.state", &[]));
            body.push_str("\n```json\n");
            body.push_str(&serde_json::to_string_pretty(&rows).unwrap_or_default());
            body.push_str("\n```");
        }
    }
    Some((t(lang, &format!("prompt.{}.description", def.key), &[]), body))
}

/// `mqttpanel://connections`: the profiles with their state (same fields as list_connections, without secrets).
pub fn connections_json(link: &dyn AppLink) -> Value {
    let st = link.state();
    let lang = st.assistant.lang();
    let now = crate::types::now_ms();
    let conns = crate::state::lock(&st.conns).clone();
    let rows: Vec<Value> = st
        .store
        .profiles()
        .into_iter()
        .map(|p| {
            let status = conns.get(&p.id).map(|c| c.status(now));
            let state = status.as_ref().map(|s| if s.state == "disconnected" { "idle".to_string() } else { s.state.clone() }).unwrap_or_else(|| "idle".into());
            let error = status.as_ref().and_then(|s| s.error.as_ref()).map(|e| e.render(lang));
            let tls = p.scheme == "mqtts" || p.scheme == "wss";
            json!({
                "id": p.id,
                "name": p.name,
                "state": state,
                "lastError": error,
                "host": p.host,
                "port": p.port,
                "protocol": p.scheme,
                "tls": tls,
                "skipVerify": p.tls.skip_verify,
                "hasPassword": p.has_password,
                "hasClientCertificate": p.tls.client_cert.is_some(),
                "isLocalBroker": p.is_local_broker,
                "lastTest": p.last_test.as_ref().map(|lt| json!({ "ok": lt.get("ok"), "at": lt.get("at"), "error": lt.get("error").and_then(|e| serde_json::from_value::<mqttpanel_client::UserMessage>(e.clone()).ok()).map(|e| e.render(lang)) })),
            })
        })
        .collect();
    Value::Array(rows)
}

/// `mqttpanel://assistant-log` and "Export…": one line per row, tab-separated.
pub fn log_text(lang: Lang, rows: &[LogEntry]) -> String {
    let mut out = String::new();
    out.push_str(&t(lang, "logHeader", &[]));
    out.push('\n');
    for r in rows {
        let at = utc(r.at);
        let note = r.note.as_deref().map(|n| format!(" ({n})")).unwrap_or_default();
        out.push_str(&format!("{at}\t{}\t{}\t{}\t{}\t{}{note}\n", r.host, r.tool, r.arguments, r.outcome, r.detail));
    }
    out
}

/// "2026-10-20 14:03:07 UTC" for a time in ms since the epoch.
pub fn utc(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map(|d| format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", d.year(), u8::from(d.month()), d.day(), d.hour(), d.minute(), d.second()))
        .unwrap_or_default()
}

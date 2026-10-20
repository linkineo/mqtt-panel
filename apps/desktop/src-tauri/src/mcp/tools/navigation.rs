//! Navigation group (docs/MCP.md section 4): bring the window forward and open a place; never change data. The window
//! is raised without taking the focus from another app, and a toast says the assistant moved the view.

use super::{id_prop, schema, BoxFuture, Call, Fail, Outcome, ToolDef, Wait};
use crate::mcp::AssistantEvent;
use serde_json::{json, Value};

macro_rules! h {
    ($f:path) => {
        |c: Call| -> BoxFuture<Result<Outcome, Fail>> { Box::pin($f(c)) }
    };
}

/// The Settings pages a host may open (`show_settings(page)`).
pub const SETTINGS_PAGES: [&str; 9] = ["appearance", "language", "data", "security", "assistant", "updates", "shortcuts", "privacy", "about"];

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "show_connection",
            description: "Show a connection in MQTT Panel's window (Connections screen, its form). Changes nothing.",
            schema: || schema(json!({ "id": id_prop() }), &["id"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(show_connection),
        },
        ToolDef {
            name: "show_topic",
            description: "Show a topic in the Explorer of an open connection. Changes nothing.",
            schema: || schema(json!({ "connection": id_prop(), "topic": { "type": "string" } }), &["connection", "topic"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(show_topic),
        },
        ToolDef {
            name: "show_chart",
            description: "Show the Charts screen, with the chart of this id when given. Changes nothing.",
            schema: || schema(json!({ "id": { "type": "string", "description": "Chart id; omit for the Charts screen." } }), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(show_chart),
        },
        ToolDef {
            name: "show_broker",
            description: "Show the built-in broker screen. Changes nothing.",
            schema: || schema(json!({}), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(show_broker),
        },
        ToolDef {
            name: "show_settings",
            description: "Show the Settings screen, at a page when given (assistant shows the acknowledgement, the host buttons and the Assistant log). Changes nothing.",
            schema: || schema(json!({ "page": { "type": "string", "enum": SETTINGS_PAGES } }), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(show_settings),
        },
    ]
}

/// Sends the navigation event, raises the window, and answers in one sentence.
fn go(c: &Call, ev: AssistantEvent, place: String, data: Value) -> Outcome {
    c.link.emit(ev);
    c.link.raise_window();
    Outcome::new(c.t("shown", &[("place", json!(place))]), data)
}

fn navigate(screen: &str) -> AssistantEvent {
    AssistantEvent::Navigate { screen: screen.into(), profile_id: None, topic: None, chart_id: None, page: None }
}

async fn show_connection(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?.profile;
    let mut ev = navigate("connect");
    if let AssistantEvent::Navigate { profile_id, .. } = &mut ev {
        *profile_id = Some(p.id.clone());
    }
    let place = c.t("place.connection", &[("name", json!(p.name))]);
    Ok(go(&c, ev, place, json!({ "screen": "connect", "id": p.id })))
}

async fn show_topic(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("connection")?.profile;
    let topic = c.str("topic")?;
    if !crate::state::lock(&c.state().conns).contains_key(&p.id) {
        return Err(Fail::from(mqttpanel_client::msg!("error.notConnected")));
    }
    let mut ev = navigate("explore");
    if let AssistantEvent::Navigate { profile_id, topic: t, .. } = &mut ev {
        *profile_id = Some(p.id.clone());
        *t = Some(topic.clone());
    }
    let place = c.t("place.topic", &[("topic", json!(topic)), ("name", json!(p.name))]);
    Ok(go(&c, ev, place, json!({ "screen": "explore", "connection": p.id, "topic": topic })))
}

async fn show_chart(c: Call) -> Result<Outcome, Fail> {
    let id = c.opt_str("id").filter(|s| !s.is_empty());
    let mut ev = navigate("charts");
    if let AssistantEvent::Navigate { chart_id, .. } = &mut ev {
        *chart_id = id.clone();
    }
    let place = c.t("place.charts", &[]);
    Ok(go(&c, ev, place, json!({ "screen": "charts", "chart": id })))
}

async fn show_broker(c: Call) -> Result<Outcome, Fail> {
    let place = c.t("place.broker", &[]);
    Ok(go(&c, navigate("broker"), place, json!({ "screen": "broker" })))
}

async fn show_settings(c: Call) -> Result<Outcome, Fail> {
    let page = c.opt_str("page").filter(|p| !p.is_empty());
    if let Some(p) = &page {
        if !SETTINGS_PAGES.contains(&p.as_str()) {
            return Err(Fail::args("unknown settings page"));
        }
    }
    let mut ev = navigate("settings");
    if let AssistantEvent::Navigate { page: pg, .. } = &mut ev {
        *pg = page.clone();
    }
    let place = c.t("place.settings", &[]);
    Ok(go(&c, ev, place, json!({ "screen": "settings", "page": page })))
}

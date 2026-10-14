//! Import of connection settings: MQTT Panel exports, MQTTX exports and the predecessor tool's settings file.
//! Secrets from other tools are never imported silently: passwords are skipped and listed in the summary.

use crate::model::{LastWill, Profile, StoredProfile, Subscription};
use crate::store::{decrypt_secrets, ExportFile, EXPORT_FORMAT};
use mqttpanel_client::{msg, UserMessage};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ImportResult {
    pub profiles: Vec<StoredProfile>,
    pub secrets: HashMap<String, String>,
    pub skipped: Vec<(String, UserMessage)>,
    pub notes: Vec<UserMessage>,
}

/// Parses an import file. `passphrase` unlocks secrets in an MQTT Panel export.
pub fn parse_import(bytes: &[u8], passphrase: Option<&str>) -> Result<ImportResult, UserMessage> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| msg!("file.unknownImport"))?;
    if v.get("format").and_then(Value::as_str) == Some(EXPORT_FORMAT) {
        let file: ExportFile = serde_json::from_value(v).map_err(|_| msg!("file.unknownImport"))?;
        let mut r = ImportResult { profiles: file.profiles, ..Default::default() };
        if let Some(enc) = &file.secrets {
            match passphrase.and_then(|p| decrypt_secrets(enc, p)) {
                Some(map) => r.secrets = map,
                None => r.notes.push(msg!("import.passwordsSkipped")),
            }
        }
        return Ok(r);
    }
    if let Some(map) = v.get("ConnectionManager_connections").and_then(Value::as_object) {
        return Ok(from_predecessor(map.values()));
    }
    let list: Option<&Vec<Value>> = v.as_array().or_else(|| v.get("connections").and_then(Value::as_array));
    if let Some(list) = list {
        if list.iter().any(|c| c.get("host").is_some()) {
            return Ok(from_mqttx(list));
        }
    }
    Err(msg!("file.unknownImport"))
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn port_of(v: &Value, default: u16) -> u16 {
    match v.get("port") {
        Some(Value::Number(n)) => n.as_u64().map(|p| p as u16).unwrap_or(default),
        Some(Value::String(t)) => t.parse().unwrap_or(default),
        _ => default,
    }
}

fn subs_of(v: Option<&Value>) -> Vec<Subscription> {
    let mut out = vec![];
    if let Some(Value::Array(list)) = v {
        for item in list {
            match item {
                Value::String(t) => out.push(Subscription { filter: t.clone(), qos: 0 }),
                Value::Object(_) => {
                    let filter = s(item, "topic");
                    if !filter.is_empty() {
                        out.push(Subscription { filter, qos: item.get("qos").and_then(Value::as_u64).unwrap_or(0).min(2) as u8 });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn note_password(r: &mut ImportResult, had_password: bool) {
    if had_password && !r.notes.iter().any(|n| n.key == "import.passwordsSkipped") {
        r.notes.push(msg!("import.passwordsSkipped"));
    }
}

/// The predecessor tool stores connections under "ConnectionManager_connections" (id -> connection).
fn from_predecessor<'a>(conns: impl Iterator<Item = &'a Value>) -> ImportResult {
    let mut r = ImportResult::default();
    for c in conns {
        let host = s(c, "host");
        let name = if s(c, "name").is_empty() { host.clone() } else { s(c, "name") };
        if host.is_empty() {
            r.skipped.push((name, msg!("import.unsupportedField", detail = "no host")));
            continue;
        }
        let ws = s(c, "protocol") == "ws";
        let tls = c.get("encryption").and_then(Value::as_bool).unwrap_or(false);
        let mut p = Profile::new_default();
        p.name = name;
        p.host = host;
        p.scheme = match (ws, tls) {
            (false, false) => "mqtt",
            (false, true) => "mqtts",
            (true, false) => "ws",
            (true, true) => "wss",
        }
        .into();
        p.port = port_of(c, if tls { 8883 } else { 1883 });
        let base = s(c, "basePath");
        if !base.is_empty() {
            p.ws_path = if base.starts_with('/') { base } else { format!("/{base}") };
        }
        if !s(c, "clientId").is_empty() {
            p.client_id = s(c, "clientId");
        }
        p.username = s(c, "username");
        p.tls.skip_verify = !c.get("certValidation").and_then(Value::as_bool).unwrap_or(true);
        let subs = subs_of(c.get("subscriptions"));
        if !subs.is_empty() {
            p.subscriptions = subs;
        }
        note_password(&mut r, !s(c, "password").is_empty());
        if c.get("selfSignedCertificate").is_some() || c.get("clientCertificate").is_some() {
            r.skipped.push((p.name.clone(), msg!("import.unsupportedField", detail = "certificate files; choose them again in the profile")));
        }
        r.profiles.push(StoredProfile { profile: p, blobs: Default::default() });
    }
    r
}

/// MQTTX exports a list of connections.
fn from_mqttx(list: &[Value]) -> ImportResult {
    let mut r = ImportResult::default();
    for c in list {
        let host = s(c, "host");
        let name = if s(c, "name").is_empty() { host.clone() } else { s(c, "name") };
        if host.is_empty() {
            r.skipped.push((name, msg!("import.unsupportedField", detail = "no host")));
            continue;
        }
        let mut p = Profile::new_default();
        p.name = name;
        p.host = host;
        let ssl = c.get("ssl").and_then(Value::as_bool).unwrap_or(false);
        let proto = s(c, "protocol");
        p.scheme = match proto.as_str() {
            "ws" if ssl => "wss",
            "ws" => "ws",
            "wss" => "wss",
            "mqtts" => "mqtts",
            _ if ssl => "mqtts",
            _ => "mqtt",
        }
        .into();
        p.port = port_of(c, 1883);
        let path = s(c, "path");
        if !path.is_empty() {
            p.ws_path = path;
        }
        if !s(c, "clientId").is_empty() {
            p.client_id = s(c, "clientId");
        }
        p.username = s(c, "username");
        p.mqtt_version = match s(c, "mqttVersion").as_str() {
            "5.0" | "5" => "5",
            "3.1.1" => "3.1.1",
            _ => "auto",
        }
        .into();
        if let Some(k) = c.get("keepalive").and_then(Value::as_u64) {
            p.keep_alive = k.min(65535) as u16;
        }
        if let Some(clean) = c.get("clean").and_then(Value::as_bool) {
            p.clean_start = clean;
        }
        p.tls.skip_verify = !c.get("rejectUnauthorized").and_then(Value::as_bool).unwrap_or(true);
        let subs = subs_of(c.get("subscriptions"));
        if !subs.is_empty() {
            p.subscriptions = subs;
        }
        if let Some(w) = c.get("will") {
            let topic = s(w, "lastWillTopic");
            if !topic.is_empty() {
                p.last_will = Some(LastWill {
                    topic,
                    payload: s(w, "lastWillPayload"),
                    qos: w.get("lastWillQos").and_then(Value::as_u64).unwrap_or(0).min(2) as u8,
                    retain: w.get("lastWillRetain").and_then(Value::as_bool).unwrap_or(false),
                });
            }
        }
        note_password(&mut r, !s(c, "password").is_empty());
        if !s(c, "ca").is_empty() || !s(c, "cert").is_empty() || !s(c, "key").is_empty() {
            r.skipped.push((p.name.clone(), msg!("import.unsupportedField", detail = "certificate files; choose them again in the profile")));
        }
        r.profiles.push(StoredProfile { profile: p, blobs: Default::default() });
    }
    r
}

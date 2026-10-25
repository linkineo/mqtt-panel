//! Embedded broker group (docs/MCP.md section 4): status, start and stop, listeners, reachability, Anonymous, users,
//! client certificates, the local CA and the server certificate, the log, and the "Built-in broker" flow of the
//! Connections screen. Every change applies at once like in the window (no restart), and the profile "Local broker"
//! follows it (`crate::broker::*_on`).

use super::{ask_prop, folder_prop, need, schema, shown, BoxFuture, Call, Fail, Outcome, ToolDef, Wait};
use crate::broker as b;
use crate::mcp::{text, Ask};
use crate::state::AppState;
use mqttpanel_broker::{AddListenerRequest, Bind, BrokerState, CertFormat, IssueCertRequest, Listener, ListenerKind, LogLevel, Preset, VerifyAgainst};
use mqttpanel_client::catalog::Lang;
use serde_json::{json, Value};

macro_rules! h {
    ($f:path) => {
        |c: Call| -> BoxFuture<Result<Outcome, Fail>> { Box::pin($f(c)) }
    };
}

fn listener_id_prop() -> Value {
    json!({ "type": "string", "description": "The listener id from broker_status (for example tcp, tls, wss-2)." })
}

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "broker_status",
            description: "The built-in broker: running or stopped, its listeners (kind, port, on or off, up, error), reachability, Anonymous, users (names only), the client certificate rule, connected clients and retained messages. Read-only.",
            schema: || schema(json!({}), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_status),
        },
        ToolDef {
            name: "broker_start",
            description: "Start the built-in broker (nothing happens when it runs; a listener that could not start is tried again).",
            schema: || schema(json!({}), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_start),
        },
        ToolDef {
            name: "broker_stop",
            description: "Stop the built-in broker; every client connected to it is disconnected.",
            schema: || schema(json!({}), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_stop),
        },
        ToolDef {
            name: "broker_add_listener",
            description: "Add a listener to the built-in broker; it starts at once when the broker runs. preset: plain (no encryption, tcp or ws), encrypted (TLS), passwords (TLS, clients log in with a broker user), mtls (TLS, every client needs a client certificate; this makes client certificates required on every encrypted listener). An encrypted preset turns tcp into tls and ws into wss.",
            schema: || {
                schema(
                    json!({
                        "kind": { "type": "string", "enum": ["tcp", "tls", "ws", "wss"] },
                        "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
                        "preset": { "type": "string", "enum": ["plain", "encrypted", "passwords", "mtls"] }
                    }),
                    &["kind", "port", "preset"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_add_listener),
        },
        ToolDef {
            name: "broker_update_listener",
            description: "Switch a listener on or off, or change its port; applies at once (only that listener's clients reconnect).",
            schema: || schema(json!({ "id": listener_id_prop(), "enabled": { "type": "boolean" }, "port": { "type": "integer", "minimum": 1, "maximum": 65535 } }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_update_listener),
        },
        ToolDef {
            name: "broker_remove_listener",
            description: "Remove a listener from the built-in broker; its clients are disconnected. Confirm with the user first.",
            schema: || schema(json!({ "id": listener_id_prop() }), &["id"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_remove_listener),
        },
        ToolDef {
            name: "broker_set_reachability",
            description: "Who can reach every listener of the built-in broker: this_computer (only apps on this computer) or whole_network (devices on the network too).",
            schema: || schema(json!({ "reach": { "type": "string", "enum": ["this_computer", "whole_network"] } }), &["reach"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_set_reachability),
        },
        ToolDef {
            name: "broker_set_anonymous",
            description: "Anonymous access on (clients connect without a user or a certificate; fine for a test bench, unsafe on a network) or off.",
            schema: || schema(json!({ "on": { "type": "boolean" } }), &["on"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_set_anonymous),
        },
        ToolDef {
            name: "broker_add_user",
            description: "Add a user of the built-in broker (shared by every listener), or change the password of an existing one. Recommended: ask_in_app true (the default), MQTT Panel asks the user for the password in its window, so it never passes through the chat. The direct way (password argument) sends the password through the chat transcript and the model provider; use it only when the user insists. When the user has already written the password in the chat, use the direct way with it: it has already passed through the chat, and asking again in the window only makes them type it twice.",
            schema: || {
                schema(
                    json!({
                        "username": { "type": "string" },
                        "ask_in_app": ask_prop(),
                        "password": { "type": "string", "description": "Direct way, not recommended: this value travels through the chat and the model provider." }
                    }),
                    &["username"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &["password"],
            wait: Wait::MaybeUser,
            handler: h!(broker_add_user),
        },
        ToolDef {
            name: "broker_remove_user",
            description: "Remove a user of the built-in broker; clients logged in with it are disconnected. Confirm with the user first.",
            schema: || schema(json!({ "username": { "type": "string" } }), &["username"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_remove_user),
        },
        ToolDef {
            name: "broker_client_certs",
            description: "The client certificates issued by the built-in broker's CA: name, expiry, key type, last seen, revoked. Read-only.",
            schema: || schema(json!({}), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_client_certs),
        },
        ToolDef {
            name: "broker_issue_client_cert",
            description: "Issue a client certificate for a device from the built-in broker's CA. The files (certificate and key, or one PKCS#12 bundle, plus CA.pem by default) are written to a folder the user picks in MQTT Panel; the result gives their paths, never the key. A PKCS#12 bundle needs a passphrase: recommended, leave it out and MQTT Panel asks the user for it together with the folder. The direct way (passphrase argument) sends the passphrase through the chat transcript and the model provider. When the user has already written the passphrase in the chat, use the direct way with it: it has already passed through the chat, and asking again in the window only makes them type it twice.",
            schema: || {
                schema(
                    json!({
                        "name": { "type": "string", "description": "The device name, also the certificate's common name (letters, digits, dot, dash, underscore)." },
                        "format": { "type": "string", "enum": ["pem", "pkcs12"], "default": "pem" },
                        "valid_years": { "type": "integer", "enum": [1, 2, 5], "default": 2 },
                        "key_type": { "type": "string", "enum": ["ecdsa", "rsa"], "default": "ecdsa" },
                        "include_ca": { "type": "boolean", "default": true, "description": "Also write CA.pem, which the device needs to trust the broker." },
                        "ask_in_app": ask_prop(),
                        "passphrase": { "type": "string", "description": "Direct way, not recommended: passphrase of the bundle or of the PEM key; travels through the chat." }
                    }),
                    &["name"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &["passphrase"],
            wait: Wait::User,
            handler: h!(broker_issue_client_cert),
        },
        ToolDef {
            name: "broker_revoke_client_cert",
            description: "Revoke a client certificate: the broker refuses it from now on and disconnects the device using it. Cannot be undone; confirm with the user first.",
            schema: || schema(json!({ "name": { "type": "string" } }), &["name"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_revoke_client_cert),
        },
        ToolDef {
            name: "broker_set_client_cert_rule",
            description: "Client certificates on every encrypted listener (TLS, WSS): required or optional, and which CA verifies them (local: the broker's own CA, imported: a CA imported on the Broker screen). Plain listeners are never affected. Required disconnects the clients without a certificate.",
            schema: || schema(json!({ "rule": { "type": "string", "enum": ["optional", "required"] }, "verify_against": { "type": "string", "enum": ["local", "imported"] } }), &["rule"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_set_client_cert_rule),
        },
        ToolDef {
            name: "broker_export_ca",
            description: "Save the built-in broker's CA certificate (CA.pem, public) for devices that must trust the broker. Recommended: ask_in_app true (the default), the user picks the folder in MQTT Panel; or give an absolute folder path. Returns the file path.",
            schema: || schema(json!({ "ask_in_app": ask_prop(), "folder": folder_prop() }), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(broker_export_ca),
        },
        ToolDef {
            name: "broker_export_server_cert",
            description: "Save the certificate chain the built-in broker serves (server-chain.pem, public; devices can pin it). Recommended: ask_in_app true (the default), the user picks the folder in MQTT Panel; or give an absolute folder path. Returns the file path.",
            schema: || schema(json!({ "ask_in_app": ask_prop(), "folder": folder_prop() }), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(broker_export_server_cert),
        },
        ToolDef {
            name: "broker_regenerate_server_cert",
            description: "Generate a new server certificate from the local CA with the automatic names (localhost, this computer's name and addresses) plus extra_names (host names or IP addresses devices use to reach the broker; replaces the kept list; omit to keep it). Devices that trust CA.pem keep working.",
            schema: || schema(json!({ "extra_names": { "type": "array", "items": { "type": "string" } }, "key_type": { "type": "string", "enum": ["ecdsa", "rsa"] } }), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_regenerate_server_cert),
        },
        ToolDef {
            name: "broker_rotate_ca",
            description: "Replace the built-in broker's local CA and server certificate. Every client certificate issued before stops working and every device needs the new CA.pem. Confirm with the user first.",
            schema: || schema(json!({}), &[]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_rotate_ca),
        },
        ToolDef {
            name: "broker_log",
            description: "The last lines of the built-in broker's log (connections, refusals, listener errors). level: debug, info (default), warn or error; limit: at most 500 (default 100). Read-only.",
            schema: || schema(json!({ "level": { "type": "string", "enum": ["debug", "info", "warn", "error"], "default": "info" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 100 } }), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(broker_log),
        },
        ToolDef {
            name: "connect_builtin_broker",
            description: "Connect MQTT Panel itself to its built-in broker, like \"New connection, Built-in broker\" in the window: it creates the broker user mqtt-panel or the client certificate mqttpanel-desktop when the listener needs one, trusts the broker's CA, starts the broker and connects the single profile \"Local broker\". listener: a listener id or kind (tcp, tls, ws, wss); omit for the most secure listener switched on. Returns the profile id.",
            schema: || schema(json!({ "listener": { "type": "string", "description": "Listener id or kind; the most secure one switched on when omitted." } }), &[]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Connect,
            handler: h!(connect_builtin_broker),
        },
    ]
}

// ---------------------------------------------------------------- words

fn tt(lang: Lang, key: &str, params: &[(&str, Value)]) -> String {
    text::t(lang, key, params)
}

fn state_word(s: BrokerState) -> &'static str {
    match s {
        BrokerState::Running => "running",
        BrokerState::Starting => "starting",
        BrokerState::Failed => "failed",
        BrokerState::Stopped => "stopped",
    }
}

fn kind_of(s: &str) -> Option<ListenerKind> {
    match s.trim().to_ascii_lowercase().as_str() {
        "tcp" | "mqtt" => Some(ListenerKind::Tcp),
        "tls" | "mqtts" => Some(ListenerKind::Tls),
        "ws" => Some(ListenerKind::Ws),
        "wss" => Some(ListenerKind::Wss),
        _ => None,
    }
}

fn preset_word(p: Preset) -> &'static str {
    match p {
        Preset::Plain => "plain",
        Preset::Encrypted => "encrypted",
        Preset::Passwords => "passwords",
        Preset::Mtls => "mtls",
    }
}

/// "TLS 8883 (up), TCP 1883 (off)".
fn listeners_text(lang: Lang, st: &AppState) -> String {
    let Ok(m) = b::mgr(st) else { return String::new() };
    let cfg = m.config();
    let status = m.status();
    let running = m.is_running();
    if cfg.listeners.is_empty() {
        return tt(lang, "broker.noListeners", &[]);
    }
    cfg.listeners
        .iter()
        .map(|l| {
            let ls = status.listeners.iter().find(|x| x.id == l.id);
            let word = if !l.enabled {
                tt(lang, "broker.listener.off", &[])
            } else if let Some(e) = ls.and_then(|x| x.error.clone()) {
                tt(lang, "broker.listener.error", &[("error", json!(text::m(lang, &b::um(e))))])
            } else if running && ls.is_some_and(|x| x.up) {
                tt(lang, "broker.listener.up", &[])
            } else {
                tt(lang, "broker.listener.down", &[])
            };
            format!("{} {} ({word})", l.kind.label(), l.port)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// broker_status and `mqttpanel://broker`: no secret (user names only).
pub fn status_json(st: &AppState, lang: Lang) -> Result<Value, mqttpanel_client::UserMessage> {
    let m = b::mgr(st)?;
    let cfg = m.config();
    let s = m.status();
    let running = m.is_running();
    let listeners: Vec<Value> = cfg
        .listeners
        .iter()
        .map(|l| {
            let ls = s.listeners.iter().find(|x| x.id == l.id);
            json!({
                "id": l.id,
                "kind": l.kind,
                "port": l.port,
                "enabled": l.enabled,
                "preset": l.preset,
                "up": running && ls.is_some_and(|x| x.up),
                "error": ls.and_then(|x| x.error.clone()).map(|e| text::m(lang, &b::um(e))),
            })
        })
        .collect();
    let certs = m.issued_certs();
    Ok(json!({
        "state": state_word(s.state),
        "error": s.error.clone().map(|e| text::m(lang, &b::um(e))),
        "listeners": listeners,
        "reachability": match cfg.bind { Bind::Network => "whole_network", Bind::Localhost => "this_computer" },
        "anonymous": cfg.anonymous,
        "users": cfg.users.iter().map(|u| json!({ "username": u.username, "system": u.system })).collect::<Vec<_>>(),
        "clientCertRule": if cfg.require_client_cert { "required" } else { "optional" },
        "verifyAgainst": cfg.verify_against,
        "extraNames": cfg.extra_sans,
        "clients": s.clients,
        "maxClients": s.max_clients,
        "retained": s.retained,
        "messagesInPerSecond": s.msg_in_rate,
        "messagesOutPerSecond": s.msg_out_rate,
        "clientCertificates": certs.iter().filter(|c| !c.revoked).count(),
    }))
}

fn status_text(c: &Call) -> String {
    let lang = c.lang();
    let st = c.state();
    let Ok(m) = b::mgr(st) else { return String::new() };
    let cfg = m.config();
    let s = m.status();
    let reach = tt(lang, if cfg.bind == Bind::Localhost { "broker.reach.localhost" } else { "broker.reach.network" }, &[]);
    let anon = tt(lang, if cfg.anonymous { "broker.on" } else { "broker.off" }, &[]);
    let rule = tt(lang, if cfg.require_client_cert { "broker.rule.required" } else { "broker.rule.optional" }, &[]);
    tt(
        lang,
        "broker.status",
        &[
            ("state", json!(tt(lang, &format!("broker.state.{}", state_word(s.state)), &[]))),
            ("listeners", json!(listeners_text(lang, st))),
            ("clients", json!(s.clients)),
            ("retained", json!(s.retained)),
            ("reach", json!(reach)),
            ("anonymous", json!(anon)),
            ("rule", json!(rule)),
        ],
    )
}

fn changed(text: String, data: Value) -> Outcome {
    // The broker screen and the profile "Local broker" (which follows every change) refresh.
    Outcome::new(text, data).changed("broker").changed("profiles")
}

fn listener(c: &Call, id: &str) -> Result<Listener, Fail> {
    let m = b::mgr(c.state())?;
    m.config().listeners.into_iter().find(|l| l.id == id).ok_or_else(|| Fail::from(text::um("broker.noListener", &[("id", json!(id))])))
}

fn port_arg(c: &Call) -> Result<Option<u16>, Fail> {
    match c.opt_u64("port")? {
        None => Ok(None),
        Some(p) => u16::try_from(p).ok().filter(|p| *p > 0).map(Some).ok_or_else(|| Fail::args("port must be between 1 and 65535")),
    }
}

// ---------------------------------------------------------------- status, start, stop

async fn broker_status(c: Call) -> Result<Outcome, Fail> {
    let data = status_json(c.state(), c.lang())?;
    Ok(Outcome::new(status_text(&c), data))
}

async fn broker_start(c: Call) -> Result<Outcome, Fail> {
    let was = b::broker_running(c.state());
    b::broker_start_on(c.state()).await?;
    let lang = c.lang();
    let key = if was { "broker.alreadyRunning" } else { "broker.started" };
    let text = tt(lang, key, &[("listeners", json!(listeners_text(lang, c.state())))]);
    Ok(changed(text, status_json(c.state(), lang)?).changed("connections"))
}

async fn broker_stop(c: Call) -> Result<Outcome, Fail> {
    let was = b::broker_running(c.state());
    b::broker_stop_on(c.state()).await?;
    let key = if was { "broker.stopped" } else { "broker.notRunning" };
    Ok(changed(c.t(key, &[]), status_json(c.state(), c.lang())?).changed("connections"))
}

// ---------------------------------------------------------------- listeners and reachability

async fn broker_add_listener(c: Call) -> Result<Outcome, Fail> {
    let kind = kind_of(&c.str("kind")?).ok_or_else(|| Fail::args("kind must be tcp, tls, ws or wss"))?;
    let port = port_arg(&c)?.ok_or_else(|| Fail::args("port is required"))?;
    let preset = match c.str("preset")?.as_str() {
        "plain" => Preset::Plain,
        "encrypted" => Preset::Encrypted,
        "passwords" => Preset::Passwords,
        "mtls" => Preset::Mtls,
        _ => return Err(Fail::args("preset must be plain, encrypted, passwords or mtls")),
    };
    let before: Vec<String> = b::mgr(c.state())?.config().listeners.into_iter().map(|l| l.id).collect();
    let cfg = b::add_listener_on(c.state(), AddListenerRequest { preset, kind, port: u32::from(port) }).await?;
    let added = cfg.listeners.iter().find(|l| !before.contains(&l.id)).cloned().ok_or_else(|| Fail::args("the listener was not added"))?;
    let lang = c.lang();
    let text = tt(
        lang,
        "broker.listenerAdded",
        &[("kind", json!(added.kind.label())), ("port", json!(added.port)), ("preset", json!(tt(lang, &format!("broker.preset.{}", preset_word(added.preset)), &[])))],
    );
    let mut data = status_json(c.state(), lang)?;
    data["added"] = json!(added.id);
    Ok(changed(text, data))
}

async fn broker_update_listener(c: Call) -> Result<Outcome, Fail> {
    let id = c.str("id")?;
    let mut l = listener(&c, &id)?;
    if let Some(on) = c.opt_bool("enabled") {
        l.enabled = on;
    }
    if let Some(p) = port_arg(&c)? {
        l.port = p;
    }
    b::update_listener_on(c.state(), l.clone()).await?;
    let lang = c.lang();
    let word = tt(lang, if l.enabled { "broker.on" } else { "broker.off" }, &[]);
    let text = tt(lang, "broker.listenerUpdated", &[("kind", json!(l.kind.label())), ("port", json!(l.port)), ("state", json!(word))]);
    Ok(changed(text, status_json(c.state(), lang)?).changed("connections"))
}

async fn broker_remove_listener(c: Call) -> Result<Outcome, Fail> {
    let id = c.str("id")?;
    let l = listener(&c, &id)?;
    b::remove_listener_on(c.state(), &id).await?;
    let text = c.t("broker.listenerRemoved", &[("kind", json!(l.kind.label())), ("port", json!(l.port))]);
    Ok(changed(text, status_json(c.state(), c.lang())?).changed("connections"))
}

async fn broker_set_reachability(c: Call) -> Result<Outcome, Fail> {
    let (bind, key) = match c.str("reach")?.as_str() {
        "this_computer" => (Bind::Localhost, "broker.reach.localhost"),
        "whole_network" => (Bind::Network, "broker.reach.network"),
        _ => return Err(Fail::args("reach must be this_computer or whole_network")),
    };
    b::set_broker_bind_on(c.state(), bind).await?;
    let lang = c.lang();
    let text = tt(lang, "broker.reachSet", &[("reach", json!(tt(lang, key, &[])))]);
    Ok(changed(text, status_json(c.state(), lang)?))
}

async fn broker_set_anonymous(c: Call) -> Result<Outcome, Fail> {
    let on = c.opt_bool("on").ok_or_else(|| Fail::args("on is required (true or false)"))?;
    b::set_anonymous_on(c.state(), on).await?;
    let text = c.t(if on { "broker.anonOn" } else { "broker.anonOff" }, &[]);
    Ok(changed(text, status_json(c.state(), c.lang())?).changed("connections"))
}

// ---------------------------------------------------------------- users

async fn broker_add_user(c: Call) -> Result<Outcome, Fail> {
    let username = c.str("username")?.trim().to_string();
    let m = b::mgr(c.state())?;
    let exists = m.config().users.iter().any(|u| u.username == username);
    let password = match c.opt_str("password").filter(|p| !p.is_empty()) {
        Some(pw) => pw,
        None if c.ask_in_app() => {
            let a = c.ask(Ask { request: "brokerPassword".into(), name: username.clone(), ..Default::default() }).await?;
            a.password.filter(|p| !p.is_empty()).ok_or_else(Fail::cancelled)?
        }
        None => return Err(need("password")),
    };
    let key = if exists {
        b::set_user_password_on(c.state(), &username, &password).await?;
        "broker.userPassword"
    } else {
        b::add_user_on(c.state(), &username, &password)?;
        "broker.userAdded"
    };
    Ok(changed(c.t(key, &[("name", json!(username))]), json!({ "username": username, "created": !exists })))
}

async fn broker_remove_user(c: Call) -> Result<Outcome, Fail> {
    let username = c.str("username")?;
    if !b::mgr(c.state())?.config().users.iter().any(|u| u.username == username) {
        return Err(Fail::from(text::um("broker.noUser", &[("name", json!(username))])));
    }
    b::remove_user_on(c.state(), &username)?;
    Ok(changed(c.t("broker.userRemoved", &[("name", json!(username))]), json!({ "username": username, "removed": true })))
}

// ---------------------------------------------------------------- client certificates

fn certs_json(st: &AppState) -> Result<Value, Fail> {
    let certs = b::mgr(st)?.issued_certs();
    Ok(serde_json::to_value(&certs).unwrap_or_default())
}

async fn broker_client_certs(c: Call) -> Result<Outcome, Fail> {
    let certs = b::mgr(c.state())?.issued_certs();
    let text = if certs.is_empty() {
        c.t("broker.noCerts", &[])
    } else {
        let list: Vec<String> = certs.iter().map(|x| format!("{}{}", x.name, if x.revoked { " (revoked)" } else { "" })).collect();
        c.t("broker.certs", &[("count", json!(certs.len())), ("list", json!(list.join(", ")))])
    };
    Ok(Outcome::new(text, json!({ "certificates": certs_json(c.state())? })))
}

async fn broker_issue_client_cert(c: Call) -> Result<Outcome, Fail> {
    let name = c.str("name")?.trim().to_string();
    let format = match c.opt_enum("format", &["pem", "pkcs12"])?.as_deref() {
        Some("pkcs12") => CertFormat::Pkcs12,
        _ => CertFormat::Pem,
    };
    let years = match c.opt_u64("valid_years")? {
        None => 2,
        Some(y @ (1 | 2 | 5)) => y as u8,
        Some(_) => return Err(Fail::args("valid_years must be 1, 2 or 5")),
    };
    let key_type = match c.opt_enum("key_type", &["ecdsa", "rsa"])?.as_deref() {
        Some("rsa") => mqttpanel_tls::kit::KeyAlg::Rsa,
        _ => mqttpanel_tls::kit::KeyAlg::Ecdsa,
    };
    let include_ca = c.opt_bool("include_ca").unwrap_or(true);
    let direct = c.opt_str("passphrase").filter(|p| !p.is_empty());
    if direct.is_none() && !c.ask_in_app() && format == CertFormat::Pkcs12 {
        return Err(need("passphrase"));
    }
    // The folder always comes from the app (the key file leaves the app only to a place the user picked).
    let fmt = if format == CertFormat::Pkcs12 { "pkcs12" } else { "pem" };
    let a = c
        .ask(Ask { request: "issueCert".into(), name: name.clone(), format: Some(fmt.into()), with_passphrase: Some(direct.is_none() && c.ask_in_app()), ..Default::default() })
        .await?;
    let folder = a.path.filter(|p| !p.is_empty()).map(std::path::PathBuf::from).ok_or_else(Fail::cancelled)?;
    let passphrase = direct.or(a.passphrase).unwrap_or_default();
    if format == CertFormat::Pkcs12 && passphrase.is_empty() {
        return Err(Fail::args("a PKCS#12 bundle needs a passphrase"));
    }
    let files = b::issue_client_cert_on(c.state(), IssueCertRequest { name: name.clone(), valid_years: years, key_type, format, passphrase, save_ca: include_ca })?;
    let paths = crate::files::write_files_in(&folder, &files)?;
    let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let text = c.t("broker.certIssued", &[("name", json!(name)), ("files", json!(names.join(", "))), ("folder", json!(shown(&folder)))]);
    let toast = c.t("broker.certIssuedToast", &[("name", json!(name))]);
    let mut out = changed(text, json!({ "name": name, "files": paths.iter().map(|p| shown(p)).collect::<Vec<_>>(), "certificates": certs_json(c.state())? }));
    out.toast = Some(toast);
    Ok(out)
}

async fn broker_revoke_client_cert(c: Call) -> Result<Outcome, Fail> {
    let name = c.str("name")?;
    if !b::mgr(c.state())?.issued_certs().iter().any(|x| x.name == name && !x.revoked) {
        return Err(Fail::from(text::um("broker.noCert", &[("name", json!(name))])));
    }
    b::revoke_client_cert_on(c.state(), &name)?;
    Ok(changed(c.t("broker.certRevoked", &[("name", json!(name))]), json!({ "name": name, "certificates": certs_json(c.state())? })))
}

async fn broker_set_client_cert_rule(c: Call) -> Result<Outcome, Fail> {
    let required = match c.str("rule")?.as_str() {
        "required" => true,
        "optional" => false,
        _ => return Err(Fail::args("rule must be optional or required")),
    };
    if let Some(v) = c.opt_enum("verify_against", &["local", "imported"])? {
        let source = if v == "imported" { VerifyAgainst::Imported } else { VerifyAgainst::Local };
        b::set_verify_against_on(c.state(), source).await?;
    }
    let cfg = b::set_require_client_cert_on(c.state(), required).await?;
    let lang = c.lang();
    let rule = tt(lang, if required { "broker.rule.required" } else { "broker.rule.optional" }, &[]);
    let against = tt(lang, if cfg.verify_against == VerifyAgainst::Imported { "broker.against.imported" } else { "broker.against.local" }, &[]);
    let text = tt(lang, "broker.ruleSet", &[("rule", json!(rule)), ("against", json!(against))]);
    Ok(changed(text, status_json(c.state(), lang)?).changed("connections"))
}

// ---------------------------------------------------------------- CA and server certificate

async fn export_one(c: &Call, file: (String, Vec<u8>), key: &str) -> Result<Outcome, Fail> {
    let (name, data) = file;
    let folder = c.folder_target(&name, &name).await?;
    let path = folder.join(&name);
    crate::files::write_file(&path, &data)?;
    Ok(Outcome::new(c.t(key, &[("file", json!(shown(&path)))]), json!({ "file": shown(&path) })))
}

async fn broker_export_ca(c: Call) -> Result<Outcome, Fail> {
    let f = b::mgr(c.state())?.export_ca();
    export_one(&c, f, "broker.caExported").await
}

async fn broker_export_server_cert(c: Call) -> Result<Outcome, Fail> {
    let f = b::mgr(c.state())?.export_server_chain();
    export_one(&c, f, "broker.serverExported").await
}

async fn broker_regenerate_server_cert(c: Call) -> Result<Outcome, Fail> {
    let m = b::mgr(c.state())?;
    let extra: Vec<String> = match c.args.get("extra_names") {
        None | Some(Value::Null) => m.config().extra_sans,
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Some(_) => return Err(Fail::args("extra_names must be a list of host names or addresses")),
    };
    let key_type = match c.opt_enum("key_type", &["ecdsa", "rsa"])?.as_deref() {
        Some("rsa") => mqttpanel_tls::kit::KeyAlg::Rsa,
        Some(_) => mqttpanel_tls::kit::KeyAlg::Ecdsa,
        None => {
            let current = m.broker_certs().map(|x| x.server.key_type).unwrap_or_default();
            if current.starts_with("RSA") {
                mqttpanel_tls::kit::KeyAlg::Rsa
            } else {
                mqttpanel_tls::kit::KeyAlg::Ecdsa
            }
        }
    };
    let certs = b::regenerate_broker_certs_on(c.state(), &extra, key_type).await?;
    let names = certs.server.san.join(", ");
    let text = c.t("broker.serverRegenerated", &[("names", json!(names))]);
    Ok(changed(text, json!({ "names": certs.server.san, "extraNames": certs.extra_sans, "notAfter": certs.server.not_after, "fingerprint": certs.server.fingerprint })).changed("connections"))
}

async fn broker_rotate_ca(c: Call) -> Result<Outcome, Fail> {
    let certs = b::rotate_ca_on(c.state()).await?;
    let text = c.t("broker.caRotated", &[]);
    Ok(changed(text, json!({ "ca": { "subject": certs.ca.subject, "notAfter": certs.ca.not_after, "fingerprint": certs.ca.fingerprint } })).changed("connections"))
}

// ---------------------------------------------------------------- log

async fn broker_log(c: Call) -> Result<Outcome, Fail> {
    let level = match c.opt_enum("level", &["debug", "info", "warn", "error"])?.as_deref() {
        Some("debug") => LogLevel::Debug,
        Some("warn") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        _ => LogLevel::Info,
    };
    let limit = c.opt_u64("limit")?.unwrap_or(100).clamp(1, 500) as usize;
    let lines = b::mgr(c.state())?.log(level, 0);
    let start = lines.len().saturating_sub(limit);
    let tail = &lines[start..];
    let text = if tail.is_empty() {
        c.t("broker.logEmpty", &[])
    } else {
        let body: Vec<String> = tail.iter().map(|l| format!("{} {:?} {}", crate::mcp::prompts::utc(l.at), l.level, l.text)).collect();
        format!("{}\n{}", c.t("broker.log", &[("count", json!(tail.len()))]), body.join("\n"))
    };
    Ok(Outcome::new(text, json!({ "lines": tail, "more": start > 0 })))
}

// ---------------------------------------------------------------- the "Built-in broker" flow

async fn connect_builtin_broker(c: Call) -> Result<Outcome, Fail> {
    let st = c.state();
    let m = b::mgr(st)?;
    let cfg = m.config();
    let l = match c.opt_str("listener").filter(|s| !s.trim().is_empty()) {
        Some(x) => match cfg.listeners.iter().find(|l| l.id == x) {
            Some(l) => l.clone(),
            None => {
                let kind = kind_of(&x).ok_or_else(|| Fail::from(text::um("broker.noListener", &[("id", json!(x))])))?;
                let req = cfg.require_client_cert;
                cfg.listeners
                    .iter()
                    .filter(|l| l.kind == kind)
                    .max_by_key(|l| (l.enabled, mqttpanel_broker::BrokerManager::listener_rank(l, req)))
                    .cloned()
                    .ok_or_else(|| Fail::from(text::um("broker.noListener", &[("id", json!(x))])))?
            }
        },
        None => m.preferred_listener().ok_or_else(|| Fail::from(text::um("broker.noEnabledListener", &[])))?,
    };
    if !l.enabled {
        let mut on = l.clone();
        on.enabled = true;
        b::update_listener_on(st, on).await?;
    }
    let lang = c.lang();
    let mut done = vec![];
    let mut outcome = None;
    let mut steps = serde_json::Map::new();
    for step in ["user", "cert", "trust", "broker", "connect"] {
        let r = b::app_broker_step_on(st, &l.id, step, "Local broker").await?;
        let sentence = match (step, r.result.as_str()) {
            ("user", "created") => Some("broker.step.user"),
            ("cert", "issued") => Some("broker.step.cert"),
            ("trust", "trusted") => Some("broker.step.trust"),
            ("broker", "started") => Some("broker.step.started"),
            _ => None,
        };
        if let Some(k) = sentence {
            done.push(tt(lang, k, &[]));
        }
        steps.insert(step.into(), json!(r.result));
        outcome = r.outcome;
    }
    let profile = st.store.list().into_iter().find(|p| p.profile.is_local_broker).ok_or_else(|| Fail::from(text::um("broker.noEnabledListener", &[])))?;
    let name = profile.profile.name.clone();
    let listener_text = format!("{} {}", l.kind.label(), l.port);
    match outcome {
        Some(crate::types::ConnectOutcome::Connected { status }) => {
            let mut text = tt(lang, "broker.builtinConnected", &[("name", json!(name)), ("listener", json!(listener_text))]);
            if !done.is_empty() {
                text = format!("{text} {}", done.join(" "));
            }
            let toast = tt(lang, "broker.builtinToast", &[("name", json!(name))]);
            let mut out = changed(
                text,
                json!({ "id": profile.profile.id, "name": name, "listener": l.id, "protocol": profile.profile.scheme, "port": profile.profile.port, "steps": steps, "state": status.state, "mqttVersion": status.negotiated_version }),
            )
            .changed("connections");
            out.toast = Some(toast);
            Ok(out)
        }
        Some(crate::types::ConnectOutcome::Failed { error }) => Err(Fail::from(text::um("connectFailed", &[("name", json!(name)), ("error", json!(text::m(lang, &error)))]))),
        _ => Err(Fail::from(text::um("connectFailed", &[("name", json!(name)), ("error", json!(text::m(lang, &mqttpanel_client::msg!("tls.untrusted"))))]))),
    }
}

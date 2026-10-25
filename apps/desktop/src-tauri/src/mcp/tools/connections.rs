//! Connections group (docs/MCP.md section 4): profiles, their secrets and certificates, test, trust, connect.

use super::{ask_prop, id_prop, need, schema, BoxFuture, Call, Fail, Outcome, ToolDef, Wait};
use crate::commands as cmd;
use crate::mcp::{text, Ask};
use crate::state::lock;
use crate::types::{ConnectOutcome, TestOutcome, TrustPrompt};
use mqttpanel_profiles::{Profile, StoredProfile, Subscription};
use serde_json::{json, Value};

macro_rules! h {
    ($f:path) => {
        |c: Call| -> BoxFuture<Result<Outcome, Fail>> { Box::pin($f(c)) }
    };
}

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_connections",
            description: "List the saved connections (profiles) of MQTT Panel with their state (connected, idle, failed), host, port, protocol, security and the last error. Read-only.",
            schema: || schema(json!({}), &[]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(list_connections),
        },
        ToolDef {
            name: "create_connection",
            description: "Create a connection profile. The name follows the host when omitted (\"broker.example.com\" gives \"broker\"). Protocol: mqtt (plain TCP), mqtts (TLS), ws or wss (WebSocket). The port defaults to the protocol's usual port. Set the password with set_connection_password, never here.",
            schema: || schema(profile_props(false), &["host", "protocol"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(create_connection),
        },
        ToolDef {
            name: "update_connection",
            description: "Change fields of a connection profile; fields left out stay as they are. subscriptions replaces the whole list of topic filters.",
            schema: || {
                let mut p = profile_props(true);
                p["id"] = id_prop();
                schema(p, &["id"])
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(update_connection),
        },
        ToolDef {
            name: "set_connection_password",
            description: "Set the password of a connection. Recommended: ask_in_app true (the default), MQTT Panel asks the user in its window and the password never passes through the chat. The direct way (password argument) sends the password through the chat transcript and the model provider; use it only when the user insists. When the user has already written the password in the chat, use the direct way with it: it has already passed through the chat, and asking again in the window only makes them type it twice. An empty password removes it.",
            schema: || schema(json!({ "id": id_prop(), "ask_in_app": ask_prop(), "password": { "type": "string", "description": "Direct way, not recommended: this value travels through the chat and the model provider." } }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &["password"],
            wait: Wait::MaybeUser,
            handler: h!(set_connection_password),
        },
        ToolDef {
            name: "set_ca_certificate",
            description: "Set the CA certificate a connection uses to verify its broker (PEM, CRT or DER). Recommended: ask_in_app true (the default), MQTT Panel opens its file picker. The direct way takes the file path of a file on this computer.",
            schema: || schema(json!({ "id": id_prop(), "ask_in_app": ask_prop(), "file": { "type": "string", "description": "Direct way: path of the certificate file on this computer." } }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(set_ca_certificate),
        },
        ToolDef {
            name: "set_client_certificate",
            description: "Set the client certificate and key a connection logs in with (PEM, CRT, DER, or one PKCS#12 bundle .p12/.pfx; key PEM or KEY). Recommended: ask_in_app true (the default), MQTT Panel opens its pickers and asks for a passphrase itself. The direct way (cert_file, key_file, passphrase) sends the paths and the passphrase through the chat transcript and the model provider. When the user has already written the file paths or the passphrase in the chat, use the direct way with them: they have already passed through the chat, and asking again in the window only makes them pick them twice. The key goes to MQTT Panel's secret store and is never returned.",
            schema: || {
                schema(
                    json!({
                        "id": id_prop(),
                        "ask_in_app": ask_prop(),
                        "cert_file": { "type": "string", "description": "Direct way, not recommended: path of the certificate or PKCS#12 bundle." },
                        "key_file": { "type": "string", "description": "Direct way, not recommended: path of the private key (not needed with PKCS#12)." },
                        "passphrase": { "type": "string", "description": "Direct way, not recommended: passphrase of the key or the bundle; travels through the chat." }
                    }),
                    &["id"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &["cert_file", "key_file", "passphrase"],
            wait: Wait::MaybeUser,
            handler: h!(set_client_certificate),
        },
        ToolDef {
            name: "generate_client_key",
            description: "Generate a new private key for a connection's client certificate (ECDSA P-256 by default, or RSA 2048). The key stays in MQTT Panel's secret store; the result gives its public fingerprint. Then get_client_csr, have the request signed, and import_signed_certificate.",
            schema: || schema(json!({ "id": id_prop(), "algorithm": { "type": "string", "enum": ["ecdsa", "rsa"], "default": "ecdsa" } }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(generate_client_key),
        },
        ToolDef {
            name: "get_client_csr",
            description: "The certificate signing request (PEM text) for the key generated with generate_client_key. A CSR is not secret. common_name defaults to the connection's client ID.",
            schema: || schema(json!({ "id": id_prop(), "common_name": { "type": "string" } }), &["id"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(get_client_csr),
        },
        ToolDef {
            name: "import_signed_certificate",
            description: "Import the certificate signed for the generated key (PEM, CRT or DER, with its chain if any). Recommended: ask_in_app true (the default), MQTT Panel opens its file picker. The direct way takes the file path.",
            schema: || schema(json!({ "id": id_prop(), "ask_in_app": ask_prop(), "file": { "type": "string", "description": "Direct way: path of the signed certificate on this computer." } }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(import_signed_certificate),
        },
        ToolDef {
            name: "test_connection",
            description: "Try a connection once without keeping it open. Returns the result in plain words; when the broker certificate cannot be verified, its details and the trust choices for trust_certificate. Starts the built-in broker first for the connection \"Local broker\".",
            schema: || schema(json!({ "id": id_prop() }), &["id"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(test_connection),
        },
        ToolDef {
            name: "trust_certificate",
            description: "Answer the certificate question after test_connection or connect reported a certificate MQTT Panel cannot verify: pin (trust this exact certificate), ca (trust the CA that signed it, when offered), skip_once (connect once without verification), skip_always (never verify this connection; unsafe). Ask the user which one; never pick a skip mode on your own.",
            schema: || schema(json!({ "id": id_prop(), "mode": { "type": "string", "enum": ["pin", "ca", "skip_once", "skip_always"] } }), &["id", "mode"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Connect,
            handler: h!(trust_certificate),
        },
        ToolDef {
            name: "connect",
            description: "Connect a saved connection and keep it open (the window shows it). Starts the built-in broker first for the connection \"Local broker\". Reports a certificate question like test_connection.",
            schema: || schema(json!({ "id": id_prop() }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Connect,
            handler: h!(connect),
        },
        ToolDef {
            name: "disconnect",
            description: "Disconnect an open connection; its topics and history stay until it is closed.",
            schema: || schema(json!({ "id": id_prop() }), &["id"]),
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(disconnect),
        },
        ToolDef {
            name: "delete_connection",
            description: "Delete a connection profile with its stored password and keys. Cannot be undone; confirm with the user first.",
            schema: || schema(json!({ "id": id_prop() }), &["id"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(delete_connection),
        },
    ]
}

fn profile_props(update: bool) -> Value {
    let mut p = json!({
        "name": { "type": "string", "description": "Shown in the app; follows the host when omitted." },
        "host": { "type": "string", "description": "Host name or IP address, without scheme or port." },
        "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
        "protocol": { "type": "string", "enum": ["mqtt", "mqtts", "ws", "wss"], "description": "mqtt: plain TCP, mqtts: TLS, ws/wss: WebSocket (wss with TLS)." },
        "mqtt_version": { "type": "string", "enum": ["auto", "5", "3.1.1"], "default": "auto" },
        "username": { "type": "string" },
        "client_id": { "type": "string", "description": "Random when omitted." }
    });
    if update {
        p["ws_path"] = json!({ "type": "string", "description": "WebSocket path, for ws and wss (default /mqtt)." });
        p["keep_alive"] = json!({ "type": "integer", "minimum": 0, "maximum": 65535, "description": "Seconds." });
        p["clean_start"] = json!({ "type": "boolean" });
        p["session_expiry"] = json!({ "type": "integer", "minimum": 0, "description": "Seconds (MQTT 5)." });
        p["subscriptions"] = json!({ "type": "array", "items": { "type": "object", "properties": { "filter": { "type": "string" }, "qos": { "type": "integer", "enum": [0, 1, 2] } }, "required": ["filter"] } });
    }
    p
}

// ---------------------------------------------------------------- shared helpers

fn default_port(scheme: &str) -> u16 {
    match scheme {
        "mqtts" => 8883,
        "ws" => 8083,
        "wss" => 8084,
        _ => 1883,
    }
}

fn scheme_of(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "mqtt" | "tcp" => Some("mqtt"),
        "mqtts" | "ssl" | "tls" => Some("mqtts"),
        "ws" => Some("ws"),
        "wss" => Some("wss"),
        _ => None,
    }
}

/// Like the UI's nameFromHost: the first label of a host name, an IP address as it is.
pub fn name_from_host(host: &str) -> String {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    if h.is_empty() || h.contains(':') || h.split('.').count() == 4 && h.split('.').all(|x| !x.is_empty() && x.len() <= 3 && x.chars().all(|c| c.is_ascii_digit())) {
        return h.to_string();
    }
    h.split('.').find(|x| !x.is_empty()).unwrap_or(h).to_string()
}

fn is_tls(scheme: &str) -> bool {
    scheme == "mqtts" || scheme == "wss"
}

/// The state word of a profile: connected, connecting, reconnecting, failed or idle.
fn state_of(c: &Call, id: &str) -> (String, Option<String>) {
    let conn = lock(&c.state().conns).get(id).cloned();
    match conn {
        Some(conn) => {
            let st = conn.status(crate::types::now_ms());
            let word = if st.state == "disconnected" { "idle".to_string() } else { st.state.clone() };
            (word, st.error.map(|e| text::m(c.lang(), &e)))
        }
        None => ("idle".into(), None),
    }
}

/// The security chip: plain, tls, tls with client certificate, or unverified.
fn security_of(p: &Profile) -> &'static str {
    if !is_tls(&p.scheme) {
        "plain"
    } else if p.tls.skip_verify {
        "unverified"
    } else if p.tls.client_cert.is_some() || p.tls.client_key.is_some() {
        "tlsClientCert"
    } else {
        "tls"
    }
}

/// What a tool says about a profile: never a secret, only whether one is stored.
fn profile_json(c: &Call, p: &Profile) -> Value {
    let (state, error) = state_of(c, &p.id);
    let last_test = p.last_test.as_ref().map(|t| {
        let mut t = t.clone();
        if let Some(e) = t.get("error").and_then(|e| serde_json::from_value::<mqttpanel_client::UserMessage>(e.clone()).ok()) {
            t["error"] = json!(text::m(c.lang(), &e));
        }
        t
    });
    json!({
        "id": p.id,
        "name": p.name,
        "state": state,
        "lastError": error,
        "host": p.host,
        "port": p.port,
        "protocol": p.scheme,
        "wsPath": if p.scheme.starts_with("ws") { Some(&p.ws_path) } else { None },
        "mqttVersion": p.mqtt_version,
        "clientId": p.client_id,
        "username": p.username,
        "hasPassword": p.has_password,
        "security": security_of(p),
        "caCertificate": p.tls.ca_cert.as_ref().map(|f| &f.file_name),
        "clientCertificate": p.tls.client_cert.as_ref().map(|f| &f.file_name),
        "clientKey": p.tls.client_key.as_ref().map(|f| json!({ "stored": f.in_keychain, "generated": p.tls.client_key_generated, "fingerprint": f.summary.as_ref().and_then(|s| s.fingerprint.clone()) })),
        "skipVerify": p.tls.skip_verify,
        "pinnedFingerprints": p.tls.trusted_fingerprints,
        "subscriptions": p.subscriptions,
        "keepAlive": p.keep_alive,
        "cleanStart": p.clean_start,
        "sessionExpiry": p.session_expiry,
        "isLocalBroker": p.is_local_broker,
        "lastTest": last_test,
    })
}

fn name_json(s: &str) -> Value {
    json!(s)
}

// ---------------------------------------------------------------- list, create, update

async fn list_connections(c: Call) -> Result<Outcome, Fail> {
    let profiles = c.state().store.profiles();
    let items: Vec<Value> = profiles.iter().map(|p| profile_json(&c, p)).collect();
    let text = if profiles.is_empty() {
        c.t("noConnections", &[])
    } else {
        let list: Vec<String> = items
            .iter()
            .map(|i| {
                let state = c.t(&format!("state.{}", i["state"].as_str().unwrap_or("idle")), &[]);
                format!("{} ({})", i["name"].as_str().unwrap_or_default(), state)
            })
            .collect();
        c.t("connections", &[("count", json!(profiles.len())), ("list", json!(list.join(", ")))])
    };
    Ok(Outcome::new(text, json!({ "connections": items })))
}

/// Applies the profile fields of a create or update call; returns the names of the changed fields.
fn apply_fields(c: &Call, p: &mut Profile, creating: bool) -> Result<Vec<&'static str>, Fail> {
    let mut changed = vec![];
    let old_scheme = p.scheme.clone();
    if let Some(proto) = c.opt_str("protocol") {
        let s = scheme_of(&proto).ok_or_else(|| Fail::args("protocol must be mqtt, mqtts, ws or wss"))?;
        if s != p.scheme {
            p.scheme = s.into();
            changed.push("protocol");
        }
    }
    if let Some(h) = c.opt_str("host") {
        let h = h.trim().trim_start_matches('[').trim_end_matches(']').to_string();
        if h.is_empty() || h.contains('/') || h.contains(' ') {
            return Err(Fail::args("host must be a host name or an IP address, without scheme or port"));
        }
        if h != p.host {
            p.host = h;
            changed.push("host");
        }
    }
    match c.opt_u64("port")? {
        Some(port) => {
            let port = u16::try_from(port).ok().filter(|p| *p > 0).ok_or_else(|| Fail::args("port must be between 1 and 65535"))?;
            if port != p.port {
                p.port = port;
                changed.push("port");
            }
        }
        None => {
            // A new connection, or a protocol change from the usual port: the usual port of the new protocol.
            if creating || (old_scheme != p.scheme && p.port == default_port(&old_scheme)) {
                p.port = default_port(&p.scheme);
            }
        }
    }
    if let Some(v) = c.opt_str("mqtt_version") {
        if !["auto", "5", "3.1.1"].contains(&v.as_str()) {
            return Err(Fail::args("mqtt_version must be auto, 5 or 3.1.1"));
        }
        if v != p.mqtt_version {
            p.mqtt_version = v;
            changed.push("mqtt_version");
        }
    }
    if let Some(v) = c.opt_str("username") {
        if v != p.username {
            p.username = v;
            changed.push("username");
        }
    }
    if let Some(v) = c.opt_str("client_id").filter(|v| !v.trim().is_empty()) {
        if v != p.client_id {
            p.client_id = v;
            changed.push("client_id");
        }
    }
    if let Some(v) = c.opt_str("ws_path").filter(|v| !v.is_empty()) {
        p.ws_path = if v.starts_with('/') { v } else { format!("/{v}") };
        changed.push("ws_path");
    }
    if let Some(v) = c.opt_u64("keep_alive")? {
        p.keep_alive = u16::try_from(v).map_err(|_| Fail::args("keep_alive must be at most 65535"))?;
        changed.push("keep_alive");
    }
    if let Some(v) = c.opt_bool("clean_start") {
        p.clean_start = v;
        changed.push("clean_start");
    }
    if let Some(v) = c.opt_u64("session_expiry")? {
        p.session_expiry = u32::try_from(v).map_err(|_| Fail::args("session_expiry is too large"))?;
        changed.push("session_expiry");
    }
    if let Some(subs) = c.args.get("subscriptions") {
        let arr = subs.as_array().ok_or_else(|| Fail::args("subscriptions must be a list"))?;
        let mut out = vec![];
        for s in arr {
            let filter = s.get("filter").and_then(Value::as_str).unwrap_or_default().trim().to_string();
            if filter.is_empty() {
                return Err(Fail::args("every subscription needs a filter"));
            }
            let qos = s.get("qos").and_then(Value::as_u64).unwrap_or(0);
            if qos > 2 {
                return Err(Fail::args("qos must be 0, 1 or 2"));
            }
            out.push(Subscription { filter, qos: qos as u8 });
        }
        p.subscriptions = out;
        changed.push("subscriptions");
    }
    match c.opt_str("name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        Some(n) => {
            if n != p.name {
                p.name = n;
                changed.push("name");
            }
        }
        None if creating => p.name = name_from_host(&p.host),
        None => {}
    }
    Ok(changed)
}

async fn create_connection(c: Call) -> Result<Outcome, Fail> {
    c.str("host")?;
    c.str("protocol")?;
    let mut p = Profile::new_default();
    apply_fields(&c, &mut p, true)?;
    let saved = cmd::save_profile_on(c.state(), p)?;
    let text = c.t("created", &[("name", name_json(&saved.name))]);
    Ok(Outcome::new(text, json!({ "id": saved.id, "connection": profile_json(&c, &saved) })).changed("profiles"))
}

async fn update_connection(c: Call) -> Result<Outcome, Fail> {
    let mut p = c.profile("id")?.profile;
    let changed = apply_fields(&c, &mut p, false)?;
    let saved = cmd::save_profile_on(c.state(), p)?;
    let text = if changed.is_empty() {
        c.t("unchanged", &[("name", name_json(&saved.name))])
    } else {
        c.t("updated", &[("name", name_json(&saved.name)), ("fields", json!(changed.join(", ")))])
    };
    Ok(Outcome::new(text, json!({ "changed": changed, "connection": profile_json(&c, &saved) })).changed("profiles"))
}

// ---------------------------------------------------------------- secrets and certificates

/// Asks the user in the app about a connection.
async fn ask(c: &Call, request: &str, p: &StoredProfile) -> Result<crate::mcp::AssistantAnswer, Fail> {
    c.ask(Ask { request: request.into(), profile_id: p.profile.id.clone(), name: p.profile.name.clone(), ..Default::default() }).await
}

async fn set_connection_password(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let password = match c.opt_str("password") {
        Some(pw) => pw,
        None if c.ask_in_app() => ask(&c, "password", &p).await?.password.unwrap_or_default(),
        None => return Err(need("password")),
    };
    let saved = c.state().store.set_password(&p.profile.id, &password)?;
    let key = if password.is_empty() { "passwordRemoved" } else { "passwordSet" };
    let text = c.t(key, &[("name", name_json(&saved.name))]);
    Ok(Outcome::new(text, json!({ "id": saved.id, "hasPassword": saved.has_password })).changed("profiles"))
}

fn file_name(path: &str) -> String {
    crate::files::file_name(std::path::Path::new(path))
}

async fn set_ca_certificate(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let path = match c.opt_str("file").filter(|f| !f.is_empty()) {
        Some(f) => f,
        None if c.ask_in_app() => ask(&c, "caCert", &p).await?.path.ok_or_else(Fail::cancelled)?,
        None => return Err(need("file")),
    };
    let saved = cmd::set_profile_file_on(c.state(), p.profile.id.clone(), "caCert".into(), Some(path.clone()), None)?;
    let text = c.t("caSet", &[("name", name_json(&saved.name)), ("file", json!(file_name(&path)))]);
    Ok(Outcome::new(text, json!({ "connection": profile_json(&c, &saved) })).changed("profiles"))
}

async fn set_client_certificate(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let direct = c.opt_str("cert_file").filter(|f| !f.is_empty());
    let (cert, key, pass) = match direct {
        Some(cert) => (cert, c.opt_str("key_file").filter(|f| !f.is_empty()), c.opt_str("passphrase").filter(|f| !f.is_empty())),
        None if c.ask_in_app() => {
            let a = ask(&c, "clientCert", &p).await?;
            (a.path.ok_or_else(Fail::cancelled)?, a.key_path.filter(|k| !k.is_empty()), a.passphrase.filter(|k| !k.is_empty()))
        }
        None => return Err(need("cert_file")),
    };
    let st = c.state();
    let id = p.profile.id.clone();
    if p.profile.tls.client_key_generated {
        cmd::client_cert_way(st, &id, false)?;
    }
    let mut saved = cmd::set_profile_file_on(st, id.clone(), "clientCert".into(), Some(cert.clone()), pass.clone())?;
    if let Some(k) = &key {
        saved = cmd::set_profile_file_on(st, id.clone(), "clientKey".into(), Some(k.clone()), pass.clone())?;
    }
    let text = c.t("clientCertSet", &[("name", name_json(&saved.name)), ("file", json!(file_name(&cert)))]);
    Ok(Outcome::new(text, json!({ "connection": profile_json(&c, &saved) })).changed("profiles"))
}

async fn generate_client_key(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let alg = match c.opt_str("algorithm").as_deref() {
        None | Some("ecdsa") => mqttpanel_tls::kit::KeyAlg::Ecdsa,
        Some("rsa") => mqttpanel_tls::kit::KeyAlg::Rsa,
        Some(_) => return Err(Fail::args("algorithm must be ecdsa or rsa")),
    };
    let saved = cmd::generate_key_for(c.state(), &p.profile.id, alg)?;
    let summary = saved.tls.client_key.as_ref().and_then(|f| f.summary.clone());
    let fp = summary.as_ref().and_then(|s| s.fingerprint.clone()).unwrap_or_default();
    let alg_name = summary.and_then(|s| s.key_type).unwrap_or_default();
    let text = c.t("keyGenerated", &[("name", name_json(&saved.name)), ("algorithm", json!(alg_name)), ("fingerprint", json!(fp))]);
    Ok(Outcome::new(text, json!({ "id": saved.id, "algorithm": alg_name, "fingerprint": fp })).changed("profiles"))
}

async fn get_client_csr(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let cn = c.opt_str("common_name").filter(|s| !s.trim().is_empty()).unwrap_or_else(|| p.profile.client_id.clone());
    let pem = cmd::client_csr_pem(c.state(), &p.profile.id, &cn)?;
    let text = c.t("csr", &[("name", name_json(&p.profile.name)), ("cn", json!(cn))]);
    Ok(Outcome::new(text, json!({ "commonName": cn, "csr": pem })))
}

async fn import_signed_certificate(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let path = match c.opt_str("file").filter(|f| !f.is_empty()) {
        Some(f) => f,
        None if c.ask_in_app() => ask(&c, "signedCert", &p).await?.path.ok_or_else(Fail::cancelled)?,
        None => return Err(need("file")),
    };
    let name = file_name(&path);
    let bytes = std::fs::read(&path).map_err(|_| mqttpanel_client::msg!("file.unreadable", file = name.clone()))?;
    let saved = cmd::import_signed_cert_bytes(c.state(), &p.profile.id, &name, &bytes)?;
    let text = c.t("signedImported", &[("name", name_json(&saved.name)), ("file", json!(name))]);
    Ok(Outcome::new(text, json!({ "connection": profile_json(&c, &saved) })).changed("profiles"))
}

// ---------------------------------------------------------------- test, trust, connect

/// The certificate question in plain words, with the details and the choices for trust_certificate.
fn trust_outcome(c: &Call, name: &str, t: &TrustPrompt) -> Outcome {
    let mut modes = vec!["pin"];
    if t.info.ca_offer {
        modes.push("ca");
    }
    modes.extend(["skip_once", "skip_always"]);
    let reason = text::m(c.lang(), &t.info.reason_text);
    let leaf = &t.info.leaf;
    let text = c.t(
        "needsTrust",
        &[("name", name_json(name)), ("reason", json!(reason)), ("subject", json!(leaf.subject)), ("issuer", json!(leaf.issuer)), ("modes", json!(modes.join(", ")))],
    );
    Outcome::new(
        text,
        json!({
            "kind": "needsTrust",
            "reason": t.info.reason,
            "reasonText": reason,
            "certificate": { "subject": leaf.subject, "issuer": leaf.issuer, "names": leaf.san, "notBefore": leaf.not_before, "notAfter": leaf.not_after, "fingerprint": leaf.fingerprint, "selfSigned": leaf.self_signed },
            "chain": t.info.chain.iter().map(|d| json!({ "subject": d.subject, "issuer": d.issuer, "fingerprint": d.fingerprint })).collect::<Vec<_>>(),
            "previousFingerprint": t.info.previous_fingerprint,
            "choices": modes,
        }),
    )
}

/// "Local broker" needs the built-in broker: start it when it is stopped (like Connect and Test in the window).
async fn start_local_broker(c: &Call, p: &StoredProfile) -> Result<bool, Fail> {
    if !p.profile.is_local_broker || crate::broker::broker_running(c.state()) {
        return Ok(false);
    }
    crate::broker::broker_start_on(c.state()).await?;
    Ok(true)
}

async fn test_connection(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let started = start_local_broker(&c, &p).await?;
    let name = p.profile.name.clone();
    let mut out = match cmd::test_profile_on(c.state(), &p.profile.id, false).await? {
        TestOutcome::NeedsTrust { trust } => trust_outcome(&c, &name, &trust),
        TestOutcome::Tested { result } => {
            let text = if result.ok {
                c.t("testOk", &[("name", name_json(&name)), ("ms", json!(result.latency_ms.unwrap_or(0))), ("version", json!(result.negotiated_version.clone().unwrap_or_default()))])
            } else {
                let e = result.error.as_ref().map(|e| text::m(c.lang(), e)).unwrap_or_default();
                c.t("testFailed", &[("name", name_json(&name)), ("error", json!(e))])
            };
            let mut data = serde_json::to_value(&result).unwrap_or_default();
            if let Some(e) = &result.error {
                data["error"] = json!(text::m(c.lang(), e));
            }
            data["kind"] = json!("tested");
            Outcome::new(text, data)
        }
    };
    if started {
        out.text = format!("{} {}", c.t("brokerStarted", &[]), out.text);
        out = out.changed("broker");
    }
    Ok(out.changed("profiles"))
}

/// `tls`: the profile uses TLS (mqtts, wss); without TLS there is no certificate, so `certificateVerified` is null
/// (a plain connection reported `true`, found by the prompts lab, docs/MCP_PROMPTS.md).
fn connect_outcome(c: &Call, name: &str, tls: bool, o: ConnectOutcome) -> Result<Outcome, Fail> {
    match o {
        ConnectOutcome::Connected { status } => {
            let text = c.t("connected", &[("name", name_json(name)), ("version", json!(status.negotiated_version.clone().unwrap_or_default()))]);
            let verified = if tls { json!(!status.skip_verify) } else { Value::Null };
            Ok(Outcome::new(text, json!({ "kind": "connected", "state": status.state, "mqttVersion": status.negotiated_version, "certificateVerified": verified })).changed("connections"))
        }
        ConnectOutcome::NeedsTrust { trust } => Ok(trust_outcome(c, name, &trust)),
        ConnectOutcome::Failed { error } => {
            let e = text::m(c.lang(), &error);
            Err(Fail::from(text::um("connectFailed", &[("name", name_json(name)), ("error", json!(e))])))
        }
    }
}

fn uses_tls(scheme: &str) -> bool {
    matches!(scheme, "mqtts" | "wss")
}

async fn connect(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let started = start_local_broker(&c, &p).await?;
    let name = p.profile.name.clone();
    let tls = uses_tls(&p.profile.scheme);
    let o = cmd::connect_stored(c.state(), p, false).await;
    let mut out = connect_outcome(&c, &name, tls, o)?;
    if started {
        out.text = format!("{} {}", c.t("brokerStarted", &[]), out.text);
        out = out.changed("broker");
    }
    Ok(out)
}

async fn trust_certificate(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let id = p.profile.id.clone();
    let mode = match c.str("mode")?.as_str() {
        "pin" => "certificate",
        "ca" => "ca",
        "skip_once" => "skipOnce",
        "skip_always" => "skipAlways",
        _ => return Err(Fail::args("mode must be pin, ca, skip_once or skip_always")),
    };
    if !lock(&c.state().pending_trust).contains_key(&id) {
        return Err(Fail::from(text::um("noTrustPending", &[("name", name_json(&p.profile.name))])));
    }
    if mode == "ca" && !lock(&c.state().pending_trust).get(&id).is_some_and(|t| t.ca_offer) {
        return Err(Fail::args("this certificate offers no CA to trust; choose pin, skip_once or skip_always"));
    }
    let saved = cmd::trust_certificate_on(c.state(), &id, mode)?;
    let key = match mode {
        "certificate" => "trustedPin",
        "ca" => "trustedCa",
        "skipAlways" => "trustedSkipAlways",
        _ => "trustedSkipOnce",
    };
    let mut out = Outcome::new(c.t(key, &[("name", name_json(&saved.name))]), json!({ "mode": c.str("mode")?, "connection": profile_json(&c, &saved) })).changed("profiles");
    if mode == "skipOnce" {
        // Nothing is saved: connect once without verification right away (as the window does).
        let stored = c.profile("id")?;
        let tls = uses_tls(&stored.profile.scheme);
        let o = connect_outcome(&c, &saved.name, tls, cmd::connect_stored(c.state(), stored, true).await)?;
        out.text = format!("{} {}", out.text, o.text);
        out.data["connect"] = o.data;
        out = out.changed("connections");
    }
    Ok(out)
}

async fn disconnect(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    let open = lock(&c.state().conns).get(&p.profile.id).is_some_and(|x| x.is_open());
    cmd::disconnect_on(c.state(), &p.profile.id).await?;
    let key = if open { "disconnected" } else { "notOpen" };
    Ok(Outcome::new(c.t(key, &[("name", name_json(&p.profile.name))]), json!({ "id": p.profile.id, "state": "idle" })).changed("connections"))
}

async fn delete_connection(c: Call) -> Result<Outcome, Fail> {
    let p = c.profile("id")?;
    cmd::delete_profile_on(c.state(), &p.profile.id).await?;
    Ok(Outcome::new(c.t("deleted", &[("name", name_json(&p.profile.name))]), json!({ "id": p.profile.id, "deleted": true })).changed("profiles").changed("connections"))
}

#[cfg(test)]
mod tests {
    use super::name_from_host;

    #[test]
    fn names_follow_the_host_like_the_ui() {
        assert_eq!(name_from_host("broker.example.com"), "broker");
        assert_eq!(name_from_host("10.0.0.5"), "10.0.0.5");
        assert_eq!(name_from_host("[::1]"), "::1");
        assert_eq!(name_from_host("plant"), "plant");
    }
}

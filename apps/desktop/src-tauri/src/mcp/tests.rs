//! Assistant interface tests (docs/TESTPLAN.md section 13, M-01 to M-08): an rmcp client over the app's socket against
//! the real engine, client and embedded broker, on a temporary data folder with in-memory secrets.

use super::server::listen;
use super::tools::registry;
use super::{AppLink, AssistantAnswer, AssistantEvent, Limits};
use crate::state::{lock, AppState};
use mqttpanel_profiles::model::secret_keys;
use mqttpanel_profiles::{MemorySecrets, SecretStore, Settings, Store};
use rmcp::model::*;
use rmcp::service::{RoleClient, RunningService};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct TestLink {
    state: AppState,
    events: Mutex<Vec<AssistantEvent>>,
    raised: Mutex<u32>,
    /// What the app's prompt or picker answers (None: nobody answers).
    answer: Mutex<Option<AssistantAnswer>>,
}

impl AppLink for TestLink {
    fn state(&self) -> &AppState {
        &self.state
    }
    fn emit(&self, event: AssistantEvent) {
        if let AssistantEvent::Request { id, .. } = &event {
            if let Some(a) = lock(&self.answer).clone() {
                self.state.assistant.answer(id, a);
            }
        }
        lock(&self.events).push(event);
    }
    fn raise_window(&self) {
        *lock(&self.raised) += 1;
    }
}

impl TestLink {
    fn events(&self) -> Vec<AssistantEvent> {
        lock(&self.events).clone()
    }
    fn clear(&self) {
        lock(&self.events).clear();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Env {
    link: Arc<TestLink>,
    dir: tempfile::TempDir,
    _listening: super::server::Listening,
}

/// A temporary home with memory secrets and a localhost broker (TCP and TLS listeners on free ports, Anonymous on).
async fn env() -> Env {
    use mqttpanel_broker::*;
    let dir = tempfile::tempdir().unwrap();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::new());
    let store = Store::new(dir.path().join("profiles"), secrets.clone());
    let m = BrokerManager::open(&dir.path().join("broker"), secrets).unwrap();
    m.set_bind(Bind::Localhost).await.unwrap();
    for mut l in m.config().listeners {
        l.port = free_port();
        l.enabled = matches!(l.kind, ListenerKind::Tcp | ListenerKind::Tls);
        m.update_listener(l).await.unwrap();
    }
    m.set_anonymous(true).unwrap();
    let state = AppState {
        store,
        settings: Mutex::new(Settings::default()),
        conns: Mutex::new(Default::default()),
        pending_trust: Mutex::new(Default::default()),
        broker: crate::broker::BrokerState::with_manager(m),
        assistant: Default::default(),
        charts: Default::default(),
    };
    state.assistant.set_lang("en");
    *lock(&state.assistant.limits) = Limits { call: Duration::from_secs(20), connect: Duration::from_secs(15), user: Duration::from_millis(800), ack: Duration::from_millis(400) };
    let link = Arc::new(TestLink { state, events: Mutex::new(vec![]), raised: Mutex::new(0), answer: Mutex::new(None) });
    let dyn_link: Arc<dyn AppLink> = link.clone();
    let listening = listen(dyn_link, dir.path()).await.expect("listen");
    Env { link, dir, _listening: listening }
}

type Client = RunningService<RoleClient, ClientConfig>;

async fn client(e: &Env, name: &str) -> Client {
    let stream = tokio::net::UnixStream::connect(mqttpanel_shim::endpoint_in(e.dir.path())).await.unwrap();
    let mut info = Implementation::new(name, "1.0");
    info.title = Some(name.to_string());
    let cfg = ClientConfig::new(ClientCapabilities::default(), info);
    cfg.serve(stream).await.expect("initialize")
}

/// (is_error, all texts joined, the whole result as JSON for the secret scan)
async fn call(c: &Client, name: &str, args: Value) -> (bool, String, String) {
    let mut p = CallToolRequestParams::new(name.to_string());
    if let Value::Object(o) = args {
        p = p.with_arguments(o);
    }
    let r = c.call_tool(p).await.unwrap_or_else(|e| panic!("{name}: {e}"));
    let text: Vec<String> = r.content.iter().filter_map(|b| b.as_text().map(|t| t.text.clone())).collect();
    (r.is_error.unwrap_or(false), text.join("\n"), serde_json::to_string(&r).unwrap())
}

/// The JSON block of a successful result.
fn data(text: &str) -> Value {
    let start = text.find("```json\n").expect("json block") + 8;
    let end = text[start..].find("\n```").unwrap() + start;
    serde_json::from_str(&text[start..end]).unwrap()
}

fn accept(e: &Env) {
    super::accept(&e.link.state).unwrap();
}

async fn local_broker_profile(e: &Env) -> String {
    for step in ["user", "cert", "trust", "broker", "connect"] {
        crate::broker::app_broker_step_on(&e.link.state, "tcp", step, "Local broker").await.unwrap_or_else(|m| panic!("{step}: {}", m.text));
    }
    let p = e.link.state.store.list().into_iter().find(|p| p.profile.is_local_broker).unwrap();
    crate::commands::close(&e.link.state, &p.profile.id).await;
    p.profile.id
}

/// The tools of phases 1 and 2 (Connections and Navigation).
const PHASE_1_2: [&str; 19] = [
    "list_connections",
    "create_connection",
    "update_connection",
    "set_connection_password",
    "set_ca_certificate",
    "set_client_certificate",
    "generate_client_key",
    "get_client_csr",
    "import_signed_certificate",
    "test_connection",
    "trust_certificate",
    "connect",
    "disconnect",
    "delete_connection",
    "show_connection",
    "show_topic",
    "show_chart",
    "show_broker",
    "show_settings",
];

/// M-01: list, create (name from the host, usual port), update, connect to the embedded broker, disconnect; toasts,
/// refreshes and log rows for the writing calls.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m01_connections_flow_against_the_embedded_broker() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let tools = c.list_all_tools().await.unwrap();
    for name in ["list_connections", "create_connection", "update_connection", "set_connection_password", "set_ca_certificate", "set_client_certificate", "generate_client_key", "get_client_csr", "import_signed_certificate", "test_connection", "trust_certificate", "connect", "disconnect", "delete_connection", "show_connection", "show_topic", "show_chart", "show_broker", "show_settings"] {
        assert!(tools.iter().any(|t| t.name == name), "missing tool {name}");
    }
    let (err, text, _) = call(&c, "list_connections", json!({})).await;
    assert!(!err && text.contains("no connections"), "{text}");

    let (err, text, _) = call(&c, "create_connection", json!({ "host": "broker.example.com", "protocol": "mqtts" })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!(d["connection"]["name"], "broker");
    assert_eq!(d["connection"]["port"], 8883);
    let id = d["id"].as_str().unwrap().to_string();
    let (err, text, _) = call(&c, "update_connection", json!({ "id": "broker", "protocol": "mqtt", "mqtt_version": "5", "subscriptions": [{ "filter": "plant/#", "qos": 1 }] })).await;
    assert!(!err, "{text}");
    let p = e.link.state.store.get(&id).unwrap().profile;
    assert_eq!((p.scheme.as_str(), p.port, p.mqtt_version.as_str()), ("mqtt", 1883, "5"));
    assert_eq!(p.subscriptions[0].filter, "plant/#");

    let local = local_broker_profile(&e).await;
    let (err, text, _) = call(&c, "connect", json!({ "id": local })).await;
    assert!(!err, "{text}");
    assert_eq!(data(&text)["kind"], "connected");
    // Plain TCP listener: no certificate, so nothing to call verified (prompts lab, docs/MCP_PROMPTS.md).
    assert!(data(&text)["certificateVerified"].is_null(), "{text}");
    let (_, text, _) = call(&c, "list_connections", json!({})).await;
    assert!(text.contains("Local broker (connected)"), "{text}");
    let (err, text, _) = call(&c, "disconnect", json!({ "id": "Local broker" })).await;
    assert!(!err, "{text}");
    let (err, text, _) = call(&c, "delete_connection", json!({ "id": id })).await;
    assert!(!err, "{text}");
    assert!(e.link.state.store.get(&id).is_none());

    let ev = e.link.events();
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Toast { text } if text.starts_with("Assistant: Connected Local broker"))), "{ev:?}");
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Changed { what } if what == "profiles")));
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Session { connected: true, host: Some(h) } if h == "Test host")));
    let log = e.link.state.assistant.log();
    assert_eq!(log.iter().map(|r| r.tool.as_str()).collect::<Vec<_>>(), ["create_connection", "update_connection", "connect", "disconnect", "delete_connection"]);
    assert!(log.iter().all(|r| r.host == "Test host" && r.outcome == "ok"));
    // resources
    let res = c.list_all_resources().await.unwrap();
    assert!(res.iter().any(|r| r.uri == "mqttpanel://connections") && res.iter().any(|r| r.uri == "mqttpanel://assistant-log"));
    let r = c.read_resource(ReadResourceRequestParams::new("mqttpanel://assistant-log")).await.unwrap();
    assert!(serde_json::to_string(&r).unwrap().contains("delete_connection"));
    c.cancel().await.unwrap();
}

/// M-02: before the acknowledgement, writing tools are refused with the sentence of the spec (the dialog is shown and
/// the call waits for it); reading tools work; accepting while a call waits lets it go on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m02_refusal_before_the_acknowledgement() {
    let e = env().await;
    let c = client(&e, "Test host").await;
    assert!(e.link.events().contains(&AssistantEvent::Acknowledge), "dialog on connect");
    let (err, text, _) = call(&c, "create_connection", json!({ "host": "h", "protocol": "mqtt" })).await;
    assert!(err);
    assert_eq!(text, "Not yet allowed: accept the acknowledgement in MQTT Panel, Settings, Assistant.");
    assert!(e.link.state.store.profiles().is_empty());
    assert_eq!(e.link.state.assistant.log()[0].outcome, "refused");
    let (err, _, _) = call(&c, "list_connections", json!({})).await;
    assert!(!err);
    // "Not now" refuses at once
    *lock(&e.link.state.assistant.limits) = Limits { ack: Duration::from_secs(10), ..e.link.state.assistant.limits() };
    let hub_link = e.link.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        hub_link.state.assistant.notify_ack(false);
    });
    let t0 = std::time::Instant::now();
    let (err, _, _) = call(&c, "create_connection", json!({ "host": "h", "protocol": "mqtt" })).await;
    assert!(err && t0.elapsed() < Duration::from_secs(5));
    // accepting while the call waits
    let l2 = e.link.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        super::accept(&l2.state).unwrap();
    });
    let (err, text, _) = call(&c, "create_connection", json!({ "host": "h", "protocol": "mqtt" })).await;
    assert!(!err, "{text}");
    assert_eq!(lock(&e.link.state.settings).assistant_acknowledged.as_deref(), Some(super::today().as_str()));
    // persisted
    assert!(e.link.state.store.settings().assistant_acknowledged.is_some());
    c.cancel().await.unwrap();
}

/// M-03: a direct secret works, its result carries the warning once per session, the log masks it and notes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m03_direct_secret_warning_and_masked_log() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let (_, text, _) = call(&c, "create_connection", json!({ "host": "plant.example.com", "protocol": "mqtt" })).await;
    let id = data(&text)["id"].as_str().unwrap().to_string();
    let (err, text, raw) = call(&c, "set_connection_password", json!({ "id": id, "password": "Pa55-W0rd-Direct" })).await;
    assert!(!err, "{text}");
    assert!(text.contains("travelled through the chat"), "{text}");
    assert!(!raw.contains("Pa55-W0rd-Direct"));
    assert_eq!(e.link.state.store.password(&id).as_deref(), Some("Pa55-W0rd-Direct"));
    let (_, text, _) = call(&c, "set_connection_password", json!({ "id": id, "password": "Another-Pa55" })).await;
    assert!(!text.contains("travelled through the chat"), "warning only once: {text}");
    let log = e.link.state.assistant.log();
    let row = log.iter().find(|r| r.tool == "set_connection_password").unwrap();
    assert_eq!(row.arguments["password"], "•••");
    assert_eq!(row.note.as_deref(), Some("secret given through the assistant"));
    assert!(!serde_json::to_string(&log).unwrap().contains("Pa55"));
    c.cancel().await.unwrap();
    // a new session warns again
    let c = client(&e, "Test host").await;
    let (_, text, _) = call(&c, "set_connection_password", json!({ "id": id, "password": "Third-Pa55" })).await;
    assert!(text.contains("travelled through the chat"));
    c.cancel().await.unwrap();
}

/// M-04: ask_in_app opens the app's prompt and waits; a cancel and no answer are reported; no warning, no log note.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m04_ask_in_app() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let (_, text, _) = call(&c, "create_connection", json!({ "host": "plant.example.com", "protocol": "mqtt" })).await;
    let id = data(&text)["id"].as_str().unwrap().to_string();
    *lock(&e.link.answer) = Some(AssistantAnswer { password: Some("Typed-In-App".into()), ..Default::default() });
    let (err, text, raw) = call(&c, "set_connection_password", json!({ "id": id })).await;
    assert!(!err, "{text}");
    assert!(!text.contains("travelled"));
    assert!(!raw.contains("Typed-In-App"));
    assert_eq!(e.link.state.store.password(&id).as_deref(), Some("Typed-In-App"));
    let ev = e.link.events();
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Request { request, profile_id, .. } if request == "password" && *profile_id == id)));
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::RequestDone { .. })));
    *lock(&e.link.answer) = Some(AssistantAnswer { cancelled: true, ..Default::default() });
    let (err, text, _) = call(&c, "set_connection_password", json!({ "id": id })).await;
    assert!(err && text.contains("cancelled"), "{text}");
    *lock(&e.link.answer) = None;
    let (err, text, _) = call(&c, "set_ca_certificate", json!({ "id": id })).await;
    assert!(err && text.contains("Nobody answered"), "{text}");
    let (err, text, _) = call(&c, "set_connection_password", json!({ "id": id, "ask_in_app": false })).await;
    assert!(err && text.contains("ask_in_app"), "{text}");
    let outcomes: Vec<_> = e.link.state.assistant.log().iter().filter(|r| r.tool != "create_connection").map(|r| r.outcome.clone()).collect();
    assert_eq!(outcomes, ["ok", "cancelled", "timeout", "failed"]);
    assert!(e.link.state.assistant.log().iter().all(|r| r.note.is_none()));
    c.cancel().await.unwrap();
}

/// M-05: no tool result, resource or prompt ever contains a password, a private key or a passphrase.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m05_no_secret_in_any_result() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let st = &e.link.state;
    let password = "Secret-Pa55word-91";
    let passphrase = "Bundle-Pa55phrase-27";
    let (_, text, _) = call(&c, "create_connection", json!({ "host": "127.0.0.1", "protocol": "mqtt", "name": "Plant" })).await;
    let id = data(&text)["id"].as_str().unwrap().to_string();
    let mut results = vec![];
    let mut r = |x: (bool, String, String)| results.push(x.2);
    r(call(&c, "set_connection_password", json!({ "id": id, "password": password })).await);
    // a client certificate and an encrypted key from files, with a passphrase
    use mqttpanel_tls::kit::{Ca, KeyAlg, Usage, Validity};
    let ca = Ca::new_root("Test CA", KeyAlg::Ecdsa, Validity::years(1)).unwrap();
    let leaf = ca.issue("plant-client", &[], Usage::Client, KeyAlg::Ecdsa, Validity::years(1)).unwrap();
    let encrypted_key = mqttpanel_tls::kit::key_pem_with_passphrase(&leaf.key_der, passphrase).unwrap();
    let cert_path = e.dir.path().join("client.pem");
    let key_path = e.dir.path().join("client.key");
    let ca_path = e.dir.path().join("ca.pem");
    std::fs::write(&cert_path, &leaf.cert_pem).unwrap();
    std::fs::write(&key_path, &encrypted_key).unwrap();
    std::fs::write(&ca_path, &ca.cert_pem).unwrap();
    r(call(&c, "set_ca_certificate", json!({ "id": id, "file": ca_path })).await);
    r(call(&c, "set_client_certificate", json!({ "id": id, "cert_file": cert_path, "key_file": key_path, "passphrase": passphrase })).await);
    let stored_key = st.store.secrets().get(&secret_keys::client_key(&id)).unwrap().expect("key stored");
    r(call(&c, "generate_client_key", json!({ "id": id })).await);
    let generated_key = st.store.secrets().get(&secret_keys::client_key(&id)).unwrap().expect("generated key");
    r(call(&c, "get_client_csr", json!({ "id": id })).await);
    // every other tool of phases 1 and 2, with plausible arguments (M-15 covers the broker, data and chart tools)
    for t in registry().into_iter().filter(|t| PHASE_1_2.contains(&t.name)) {
        let args = match t.name {
            "show_topic" => json!({ "connection": id, "topic": "a/b" }),
            "show_settings" => json!({ "page": "assistant" }),
            "show_chart" | "show_broker" | "list_connections" => json!({}),
            "trust_certificate" => json!({ "id": id, "mode": "pin" }),
            "create_connection" => json!({ "host": "x.example.com", "protocol": "mqtts" }),
            "delete_connection" | "set_connection_password" | "set_ca_certificate" | "set_client_certificate" | "import_signed_certificate" => continue,
            _ => json!({ "id": id }),
        };
        r(call(&c, t.name, args).await);
    }
    for uri in ["mqttpanel://connections", "mqttpanel://assistant-log"] {
        results.push(serde_json::to_string(&c.read_resource(ReadResourceRequestParams::new(uri)).await.unwrap()).unwrap());
    }
    for p in ["connect_to_broker", "why_cant_i_connect"] {
        results.push(serde_json::to_string(&c.get_prompt(GetPromptRequestParams::new(p)).await.unwrap()).unwrap());
    }
    results.push(serde_json::to_string(&c.list_all_tools().await.unwrap()).unwrap());
    let key_body = |pem: &str| pem.lines().filter(|l| !l.starts_with("-----")).collect::<String>();
    let secrets = [password.to_string(), passphrase.to_string(), key_body(&stored_key), key_body(&generated_key), key_body(&leaf.key_pem)];
    for (i, res) in results.iter().enumerate() {
        let flat = res.replace("\\n", "");
        for s in &secrets {
            assert!(!flat.contains(&s[..s.len().min(40)]), "result {i} contains a secret: {res}");
        }
    }
    c.cancel().await.unwrap();
}

/// M-06: the certificate question and the four trust modes against the broker's TLS listener.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m06_trust_modes() {
    let e = env().await;
    accept(&e);
    let m = crate::broker::test_manager(&e.link.state.broker);
    assert_eq!(m.start().await.state, mqttpanel_broker::BrokerState::Running);
    let tls_port = m.config().listeners.iter().find(|l| l.kind == mqttpanel_broker::ListenerKind::Tls).unwrap().port;
    let c = client(&e, "Test host").await;
    let mut ids = vec![];
    for n in ["pin", "ca", "once", "always"] {
        let (_, text, _) = call(&c, "create_connection", json!({ "name": n, "host": "localhost", "port": tls_port, "protocol": "mqtts" })).await;
        ids.push(data(&text)["id"].as_str().unwrap().to_string());
    }
    // no prompt yet
    let (err, text, _) = call(&c, "trust_certificate", json!({ "id": ids[0], "mode": "pin" })).await;
    assert!(err && text.contains("test_connection"), "{text}");
    for id in &ids {
        let (err, text, _) = call(&c, "test_connection", json!({ "id": id })).await;
        assert!(!err, "{text}");
        let d = data(&text);
        assert_eq!(d["kind"], "needsTrust", "{text}");
        assert!(d["choices"].as_array().unwrap().contains(&json!("skip_once")));
        assert!(d["certificate"]["fingerprint"].as_str().unwrap().contains(':'));
    }
    let (err, text, _) = call(&c, "trust_certificate", json!({ "id": ids[0], "mode": "pin" })).await;
    assert!(!err, "{text}");
    assert_eq!(e.link.state.store.get(&ids[0]).unwrap().profile.tls.trusted_fingerprints.len(), 1);
    let (_, text, _) = call(&c, "test_connection", json!({ "id": ids[0] })).await;
    assert_eq!(data(&text)["kind"], "tested", "{text}");
    assert_eq!(data(&text)["ok"], true, "{text}");
    let (err, text, _) = call(&c, "trust_certificate", json!({ "id": ids[1], "mode": "ca" })).await;
    assert!(!err, "{text}");
    assert!(e.link.state.store.get(&ids[1]).unwrap().blobs.ca_pem.is_some());
    let (_, text, _) = call(&c, "connect", json!({ "id": ids[1] })).await;
    assert_eq!(data(&text)["certificateVerified"], true, "{text}");
    let (err, text, _) = call(&c, "trust_certificate", json!({ "id": ids[2], "mode": "skip_once" })).await;
    assert!(!err, "{text}");
    assert_eq!(data(&text)["connect"]["kind"], "connected", "{text}");
    assert_eq!(data(&text)["connect"]["certificateVerified"], false);
    assert!(!e.link.state.store.get(&ids[2]).unwrap().profile.tls.skip_verify);
    let (err, text, _) = call(&c, "trust_certificate", json!({ "id": ids[3], "mode": "skip_always" })).await;
    assert!(!err, "{text}");
    assert!(e.link.state.store.get(&ids[3]).unwrap().profile.tls.skip_verify);
    c.cancel().await.unwrap();
    m.stop().await;
}

/// M-07: one host session at a time; a second host gets "another assistant is connected"; after the first leaves, a
/// new one is served.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m07_second_host_refused() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let e = env().await;
    let c = client(&e, "First host").await;
    let mut s = tokio::net::UnixStream::connect(mqttpanel_shim::endpoint_in(e.dir.path())).await.unwrap();
    s.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"second\",\"version\":\"1\"}}}\n").await.unwrap();
    let mut line = String::new();
    tokio::io::BufReader::new(&mut s).read_line(&mut line).await.unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["id"], 7);
    assert!(v["error"]["message"].as_str().unwrap().contains("Another assistant is connected"), "{line}");
    c.cancel().await.unwrap();
    for _ in 0..50 {
        if e.link.state.assistant.session().is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let c2 = client(&e, "Second host").await;
    let (err, _, _) = call(&c2, "list_connections", json!({})).await;
    assert!(!err);
    assert_eq!(e.link.state.assistant.host(), "Second host");
    c2.cancel().await.unwrap();
}

/// M-08: navigation sends the place to the window and raises it; nothing changes; texts follow the app language.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m08_navigation_and_language() {
    let e = env().await;
    let c = client(&e, "Test host").await;
    e.link.clear();
    let raised = *lock(&e.link.raised);
    let (err, text, _) = call(&c, "show_settings", json!({ "page": "assistant" })).await;
    assert!(!err, "{text}");
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Navigate { screen, page: Some(p), .. } if screen == "settings" && p == "assistant")));
    let (err, _, _) = call(&c, "show_broker", json!({})).await;
    assert!(!err);
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Navigate { screen, .. } if screen == "broker")));
    assert_eq!(*lock(&e.link.raised), raised + 2);
    let (err, _, _) = call(&c, "show_connection", json!({ "id": "nothing" })).await;
    assert!(err);
    // nothing written, nothing logged, no toast (navigation is not a writing call)
    assert!(e.link.state.assistant.log().is_empty());
    assert!(!e.link.events().iter().any(|x| matches!(x, AssistantEvent::Toast { .. })));
    e.link.state.assistant.set_lang("fr");
    let (_, text, _) = call(&c, "list_connections", json!({})).await;
    assert!(text.contains("Aucune connexion"), "{text}");
    e.link.state.assistant.set_lang("de");
    let (_, text, _) = call(&c, "list_connections", json!({})).await;
    assert!(text.contains("Noch keine Verbindung"), "{text}");
    c.cancel().await.unwrap();
}

/// The registry: every writing tool says so, every secret field is documented as travelling through the chat.
#[test]
fn registry_rules() {
    let all = registry();
    let names: std::collections::HashSet<_> = all.iter().map(|t| t.name).collect();
    assert_eq!(names.len(), all.len(), "duplicate tool names");
    for t in &all {
        let schema = (t.schema)();
        assert_eq!(schema["type"], "object", "{}", t.name);
        for f in t.secret_fields {
            assert!(schema["properties"][*f].is_object(), "{}: {f}", t.name);
            assert!(t.touches_secret());
            assert!(t.description.contains("chat"), "{} must say the value travels through the chat", t.name);
            // Lab finding (docs/MCP_PROMPTS.md): with the value already in the user's message, the model sometimes
            // asked in the window anyway; the description says to use it directly then.
            assert!(t.description.contains("already written"), "{} must say what to do with a value already in the chat", t.name);
        }
        if schema["properties"]["ask_in_app"].is_object() {
            assert!(matches!(t.wait, super::tools::Wait::MaybeUser | super::tools::Wait::User), "{}", t.name);
        }
    }
    for w in ["create_connection", "update_connection", "connect", "disconnect", "delete_connection", "trust_certificate"] {
        assert!(all.iter().find(|t| t.name == w).unwrap().writes, "{w}");
    }
    for r in ["list_connections", "get_client_csr", "test_connection", "show_connection", "show_settings"] {
        assert!(!all.iter().find(|t| t.name == r).unwrap().writes, "{r}");
    }
}

// ---------------------------------------------------------------- phases 3 and 4 (M-11 onwards)

fn answer(e: &Env, a: AssistantAnswer) {
    *lock(&e.link.answer) = Some(a);
}

fn folder_answer(p: &std::path::Path) -> AssistantAnswer {
    AssistantAnswer { path: Some(p.to_string_lossy().to_string()), ..Default::default() }
}

/// Polls until `f` holds (up to 5 s).
async fn until(mut f: impl FnMut() -> bool) {
    for _ in 0..250 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(f(), "condition not reached in 5 s");
}

/// Anonymous off and a mutual-TLS listener: client certificates are required on every encrypted listener.
async fn require_client_certs(c: &Client) -> String {
    let (err, text, _) = call(c, "broker_set_anonymous", json!({ "on": false })).await;
    assert!(!err, "{text}");
    let (err, text, _) = call(c, "broker_add_listener", json!({ "kind": "tls", "port": free_port(), "preset": "mtls" })).await;
    assert!(!err, "{text}");
    data(&text)["added"].as_str().unwrap().to_string()
}

/// connect_builtin_broker with the defaults; the profile id.
async fn builtin(c: &Client) -> String {
    let (err, text, _) = call(c, "connect_builtin_broker", json!({})).await;
    assert!(!err, "{text}");
    data(&text)["id"].as_str().unwrap().to_string()
}

fn conn_of(e: &Env, id: &str) -> Arc<crate::state::Conn> {
    lock(&e.link.state.conns).get(id).cloned().expect("open connection")
}

/// M-11: every broker tool against the in-app broker manager: start and stop, listeners (live, no restart),
/// reachability, Anonymous, users (in the app and direct), the client certificate rule, issuing (PEM and PKCS#12 into
/// a folder picked in the app), revoking, the CA and server exports, regenerating, rotating, the log.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m11_broker_tools() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let st = &e.link.state;
    let m = crate::broker::test_manager(&st.broker);
    let (err, text, _) = call(&c, "broker_status", json!({})).await;
    assert!(!err && text.contains("stopped"), "{text}");
    assert_eq!(data(&text)["state"], "stopped");
    let (err, text, _) = call(&c, "broker_start", json!({})).await;
    assert!(!err && text.starts_with("Started the built-in broker"), "{text}");
    assert!(m.is_running());
    let started_at = m.status().started_at;
    let (_, text, _) = call(&c, "broker_status", json!({})).await;
    let d = data(&text);
    assert_eq!(d["state"], "running");
    assert!(d["listeners"].as_array().unwrap().iter().any(|l| l["kind"] == "tcp" && l["up"] == true), "{text}");
    // listeners, live
    let port = free_port();
    let (err, text, _) = call(&c, "broker_add_listener", json!({ "kind": "ws", "port": port, "preset": "plain" })).await;
    assert!(!err, "{text}");
    let ws = data(&text)["added"].as_str().unwrap().to_string();
    until(|| m.running_port(&ws) == Some(port)).await;
    let port2 = free_port();
    let (err, text, _) = call(&c, "broker_update_listener", json!({ "id": ws, "port": port2 })).await;
    assert!(!err, "{text}");
    until(|| m.running_port(&ws) == Some(port2)).await;
    let (err, text, _) = call(&c, "broker_update_listener", json!({ "id": ws, "enabled": false })).await;
    assert!(!err && text.contains("is now off"), "{text}");
    let (err, text, _) = call(&c, "broker_remove_listener", json!({ "id": ws })).await;
    assert!(!err, "{text}");
    assert!(!m.config().listeners.iter().any(|l| l.id == ws));
    let (err, text, _) = call(&c, "broker_remove_listener", json!({ "id": "nothing" })).await;
    assert!(err && text.contains("broker_status"), "{text}");
    let (err, text, _) = call(&c, "broker_set_reachability", json!({ "reach": "this_computer" })).await;
    assert!(!err && text.contains("this computer only"), "{text}");
    let (err, _, _) = call(&c, "broker_set_anonymous", json!({ "on": false })).await;
    assert!(!err);
    assert!(!m.anonymous());
    // users: in the app, then direct (the password changes)
    answer(&e, AssistantAnswer { password: Some("Typed-For-Sensor-1".into()), ..Default::default() });
    let (err, text, raw) = call(&c, "broker_add_user", json!({ "username": "sensor-1" })).await;
    assert!(!err && text.contains("Added the broker user sensor-1"), "{text}");
    assert!(!raw.contains("Typed-For-Sensor-1"));
    assert_eq!(m.user_password("sensor-1").unwrap().as_deref(), Some("Typed-For-Sensor-1"));
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Request { request, profile_name, .. } if request == "brokerPassword" && profile_name == "sensor-1")));
    let (err, text, _) = call(&c, "broker_add_user", json!({ "username": "sensor-1", "password": "Direct-For-Sensor-1" })).await;
    assert!(!err && text.contains("Changed the password") && text.contains("travelled through the chat"), "{text}");
    assert_eq!(m.user_password("sensor-1").unwrap().as_deref(), Some("Direct-For-Sensor-1"));
    let (err, text, _) = call(&c, "broker_remove_user", json!({ "username": "sensor-1" })).await;
    assert!(!err, "{text}");
    assert!(!m.config().users.iter().any(|u| u.username == "sensor-1"));
    // the client certificate rule
    let (err, text, _) = call(&c, "broker_set_client_cert_rule", json!({ "rule": "required", "verify_against": "local" })).await;
    assert!(!err && text.contains("required"), "{text}");
    assert!(m.config().require_client_cert);
    let (err, text, _) = call(&c, "broker_set_client_cert_rule", json!({ "rule": "optional", "verify_against": "imported" })).await;
    assert!(err, "no imported CA: {text}");
    let (err, _, _) = call(&c, "broker_set_client_cert_rule", json!({ "rule": "optional" })).await;
    assert!(!err);
    // issuing into a folder picked in the app
    let out = tempfile::tempdir().unwrap();
    answer(&e, folder_answer(out.path()));
    let (err, text, raw) = call(&c, "broker_issue_client_cert", json!({ "name": "dev-1" })).await;
    assert!(!err, "{text}");
    let files: Vec<String> = data(&text)["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
    assert_eq!(files.len(), 3, "{files:?}");
    for f in ["dev-1.pem", "dev-1.key", "CA.pem"] {
        assert!(out.path().join(f).is_file(), "{f}");
    }
    let key = std::fs::read_to_string(out.path().join("dev-1.key")).unwrap();
    let body: String = key.lines().filter(|l| !l.starts_with("-----")).collect();
    assert!(!raw.replace("\\n", "").contains(&body[..40]), "the key never leaves through the result");
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Request { request, format: Some(f), with_passphrase: Some(true), .. } if request == "issueCert" && f == "pem")));
    // PKCS#12 with the passphrase typed in the app
    answer(&e, AssistantAnswer { path: Some(out.path().to_string_lossy().to_string()), passphrase: Some("Bundle-Typed-1".into()), ..Default::default() });
    let (err, text, raw) = call(&c, "broker_issue_client_cert", json!({ "name": "dev-2", "format": "pkcs12", "include_ca": false })).await;
    assert!(!err, "{text}");
    assert!(!raw.contains("Bundle-Typed-1"));
    let p12 = std::fs::read(out.path().join("dev-2.p12")).unwrap();
    assert!(mqttpanel_tls::inspect(&p12, mqttpanel_tls::FileExpect::CertificateOrPkcs12, Some("Bundle-Typed-1")).is_ok());
    // PKCS#12 without a passphrase and without the app
    let (err, text, _) = call(&c, "broker_issue_client_cert", json!({ "name": "dev-3", "format": "pkcs12", "ask_in_app": false })).await;
    assert!(err && text.contains("passphrase"), "{text}");
    // cancelled in the app: nothing issued
    answer(&e, AssistantAnswer { cancelled: true, ..Default::default() });
    let (err, _, _) = call(&c, "broker_issue_client_cert", json!({ "name": "dev-4" })).await;
    assert!(err);
    let (err, text, _) = call(&c, "broker_client_certs", json!({})).await;
    assert!(!err, "{text}");
    let names: Vec<String> = data(&text)["certificates"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap().to_string()).collect();
    assert!(names.contains(&"dev-1".to_string()) && names.contains(&"dev-2".to_string()) && !names.contains(&"dev-4".to_string()), "{names:?}");
    let (err, text, _) = call(&c, "broker_revoke_client_cert", json!({ "name": "dev-1" })).await;
    assert!(!err, "{text}");
    assert!(m.issued_certs().iter().any(|x| x.name == "dev-1" && x.revoked));
    // exports: a folder given directly, then one picked in the app
    let ca_dir = tempfile::tempdir().unwrap();
    let (err, text, _) = call(&c, "broker_export_ca", json!({ "folder": ca_dir.path() })).await;
    assert!(!err, "{text}");
    assert_eq!(std::fs::read_to_string(ca_dir.path().join("CA.pem")).unwrap(), m.ca_pem());
    let (err, text, _) = call(&c, "broker_export_ca", json!({ "folder": "relative/folder" })).await;
    assert!(err && text.contains("absolute"), "{text}");
    answer(&e, folder_answer(ca_dir.path()));
    let (err, text, _) = call(&c, "broker_export_server_cert", json!({})).await;
    assert!(!err, "{text}");
    assert!(std::fs::read_to_string(ca_dir.path().join("server-chain.pem")).unwrap().contains("BEGIN CERTIFICATE"));
    // regenerate with extra names, rotate the CA
    let (err, text, _) = call(&c, "broker_regenerate_server_cert", json!({ "extra_names": ["plant-gw.example.com", "10.20.30.40"] })).await;
    assert!(!err, "{text}");
    assert!(data(&text)["names"].as_array().unwrap().iter().any(|n| n == "plant-gw.example.com"), "{text}");
    assert_eq!(m.config().extra_sans, ["plant-gw.example.com", "10.20.30.40"]);
    let old_ca = m.ca_pem();
    let (err, text, _) = call(&c, "broker_rotate_ca", json!({})).await;
    assert!(!err && text.contains("issued again"), "{text}");
    assert_ne!(m.ca_pem(), old_ca);
    let (err, text, _) = call(&c, "broker_log", json!({ "limit": 500 })).await;
    assert!(!err && text.contains("client certificate dev-1 revoked") && text.contains("CA rotated"), "{text}");
    let (_, text, _) = call(&c, "broker_log", json!({ "level": "error", "limit": 3 })).await;
    assert!(data(&text)["lines"].as_array().unwrap().len() <= 3);
    // never restarted
    assert_eq!(m.status().started_at, started_at);
    let (err, text, _) = call(&c, "broker_stop", json!({})).await;
    assert!(!err && text.contains("Stopped"), "{text}");
    assert!(!m.is_running());
    // toasts, refreshes, log rows
    let ev = e.link.events();
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Toast { text } if text == "Assistant: issued the client certificate dev-1")), "{ev:?}");
    assert!(ev.iter().any(|x| matches!(x, AssistantEvent::Changed { what } if what == "broker")));
    let log = st.assistant.log();
    assert!(log.iter().all(|r| r.tool.starts_with("broker_") && r.tool != "broker_status" && r.tool != "broker_log" && r.tool != "broker_client_certs"), "reads are not logged");
    let direct = log.iter().find(|r| r.tool == "broker_add_user" && r.note.is_some()).unwrap();
    assert_eq!(direct.arguments["password"], "•••");
    c.cancel().await.unwrap();
}

/// M-12: connect_builtin_broker end to end with client certificates required (Anonymous off, a mutual-TLS listener):
/// the user, the desktop certificate, the trust, the start and the connection, one profile "Local broker".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m12_connect_builtin_broker_with_client_certificates_required() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let st = &e.link.state;
    let m = crate::broker::test_manager(&st.broker);
    let mtls = require_client_certs(&c).await;
    assert!(m.config().require_client_cert && !m.anonymous());
    let (err, text, _) = call(&c, "connect_builtin_broker", json!({ "listener": mtls })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!(d["listener"], json!(mtls));
    assert_eq!(d["protocol"], "mqtts");
    assert_eq!(d["steps"]["user"], "created");
    assert_eq!(d["steps"]["cert"], "issued");
    assert_eq!(d["steps"]["trust"], "trusted");
    assert_eq!(d["steps"]["broker"], "started");
    assert_eq!(d["steps"]["connect"], "connected");
    assert!(text.contains("Issued its client certificate mqttpanel-desktop") && text.contains("Started the built-in broker"), "{text}");
    let id = d["id"].as_str().unwrap().to_string();
    let p = st.store.get(&id).unwrap();
    assert!(p.profile.is_local_broker && p.profile.tls.client_cert.is_some() && p.blobs.ca_pem.is_some());
    assert!(conn_of(&e, &id).is_open());
    let clients = m.clients();
    assert!(clients.iter().any(|x| x.cert_cn.as_deref() == Some("mqttpanel-desktop")), "{clients:?}");
    // again: everything kept, still one profile
    let (err, text, _) = call(&c, "connect_builtin_broker", json!({ "listener": "tls" })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!((d["steps"]["user"].as_str(), d["steps"]["cert"].as_str(), d["steps"]["broker"].as_str()), (Some("kept"), Some("kept"), Some("running")));
    assert_eq!(st.store.profiles().iter().filter(|p| p.is_local_broker).count(), 1);
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Toast { text } if text == "Assistant: connected Local broker to the built-in broker")));
    // no listener switched on
    for l in m.config().listeners {
        let (err, _, _) = call(&c, "broker_update_listener", json!({ "id": l.id, "enabled": false })).await;
        assert!(!err);
    }
    let (err, text, _) = call(&c, "connect_builtin_broker", json!({})).await;
    assert!(err && text.contains("broker_add_listener"), "{text}");
    c.cancel().await.unwrap();
    m.stop().await;
}

/// M-13: Explorer and data tools on the connection to the built-in broker: publish, list (filter, subtopics), get,
/// search, history with its cap of 500 and the cursor, delete retained (one, subtree), exports to temporary files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m13_data_tools_and_bounds() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let id = builtin(&c).await;
    let conn = conn_of(&e, &id);
    let (err, text, _) = call(&c, "publish", json!({ "connection": id, "topic": "plant/r204/temp", "payload": "{\"t\":21.5,\"h\":40}", "format": "json", "retain": true, "qos": 1 })).await;
    assert!(!err && text.contains("retained"), "{text}");
    let (err, _, _) = call(&c, "publish", json!({ "connection": id, "topic": "plant/r204/state", "payload": "on", "retain": true })).await;
    assert!(!err);
    let (err, text, _) = call(&c, "publish", json!({ "connection": id, "topic": "plant/x", "payload": "{bad", "format": "json" })).await;
    assert!(err, "invalid JSON refused: {text}");
    let (err, text, _) = call(&c, "publish", json!({ "connection": id, "topic": "plant/#", "payload": "x" })).await;
    assert!(err, "{text}");
    until(|| lock(&conn.engine).topic_count() >= 2).await;
    // Retained messages arrive with the retained flag on a new subscription: reconnect (the topics stay).
    let (err, text, _) = call(&c, "disconnect", json!({ "id": id })).await;
    assert!(!err, "{text}");
    let (err, text, _) = call(&c, "connect", json!({ "id": id })).await;
    assert!(!err, "{text}");
    let conn = conn_of(&e, &id);
    until(|| lock(&conn.engine).retained_under("plant").len() == 2).await;
    let (err, text, _) = call(&c, "list_topics", json!({ "connection": "Local broker" })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!(d["total"], 2, "{text}");
    let (_, text, _) = call(&c, "list_topics", json!({ "connection": id, "filter": "plant/+/temp" })).await;
    let d = data(&text);
    assert_eq!(d["topics"][0]["topic"], "plant/r204/temp", "{text}");
    assert_eq!(d["topics"][0]["retained"], true);
    assert_eq!(lock(&conn.engine).subtopic_count("plant"), 2);
    let (err, text, _) = call(&c, "get_topic", json!({ "connection": id, "topic": "plant/r204/temp" })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!(d["payload"], json!({ "t": 21.5, "h": 40 }));
    assert!(d["fields"].as_array().unwrap().iter().any(|f| f["path"] == "$.t" && f["plottable"] == true), "{text}");
    let (err, text, _) = call(&c, "get_topic", json!({ "connection": id, "topic": "no/such" })).await;
    assert!(err, "{text}");
    let (err, text, _) = call(&c, "search_values", json!({ "connection": id, "text": "ON" })).await;
    assert!(!err, "{text}");
    assert_eq!(data(&text)["hits"][0]["topic"], "plant/r204/state");
    // history: 620 messages in a buffer of 1000
    lock(&conn.engine).set_topic_buffer("bulk/x", 1000);
    for i in 0..620 {
        let req = crate::types::PublishRequest { topic: "bulk/x".into(), payload: format!("{{\"n\":{i}}}"), format: "json".into(), qos: 0, retain: false, props: None, request: None };
        crate::commands::do_publish(&conn, &req, "pending", false).unwrap();
    }
    until(|| lock(&conn.engine).history("bulk/x").len() == 620).await;
    let (err, text, _) = call(&c, "get_history", json!({ "connection": id, "topic": "bulk/x", "limit": 5000 })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    let rows = d["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 500, "capped at 500");
    assert_eq!(rows[0]["payload"], json!({ "n": 619 }), "newest first");
    assert_eq!(d["more"], true);
    let before = d["nextBefore"].as_u64().unwrap();
    assert!(text.contains(&format!("before {before}")), "{text}");
    let (_, text, _) = call(&c, "get_history", json!({ "connection": id, "topic": "bulk/x", "limit": 500, "before": before })).await;
    let d = data(&text);
    assert_eq!(d["rows"].as_array().unwrap().len(), 120);
    assert_eq!(d["rows"][0]["payload"], json!({ "n": 119 }));
    assert_eq!(d["rows"][119]["payload"], json!({ "n": 0 }));
    assert_eq!(d["more"], false);
    assert!(d["nextBefore"].is_null());
    let (_, text, _) = call(&c, "get_history", json!({ "connection": id, "topic": "bulk/x" })).await;
    assert_eq!(data(&text)["rows"].as_array().unwrap().len(), 200, "default 200");
    // exports: a file given directly, one picked in the app, a relative path refused
    let out = tempfile::tempdir().unwrap();
    let csv = out.path().join("bulk.csv");
    let (err, text, _) = call(&c, "export_data", json!({ "connection": id, "topic": "bulk/x", "format": "csv", "file": csv })).await;
    assert!(!err, "{text}");
    assert_eq!(data(&text)["messages"], 620);
    assert_eq!(std::fs::read_to_string(&csv).unwrap().lines().count(), 621, "header plus 620 rows");
    let nd = out.path().join("plant.ndjson");
    answer(&e, folder_answer(&nd));
    let (err, text, _) = call(&c, "export_data", json!({ "connection": id, "topic": "plant", "scope": "subtree", "format": "ndjson" })).await;
    assert!(!err, "{text}");
    assert_eq!(std::fs::read_to_string(&nd).unwrap().lines().count(), 4, "two messages each (published, then retained on the reconnect)");
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Request { request, default_name: Some(n), .. } if request == "saveFile" && n.ends_with(".ndjson"))));
    let (err, text, _) = call(&c, "export_data", json!({ "connection": id, "topic": "bulk/x", "file": "bulk.csv" })).await;
    assert!(err && text.contains("absolute"), "{text}");
    let (err, text, _) = call(&c, "export_data", json!({ "connection": id, "topic": "nothing/here", "file": csv })).await;
    assert!(err && text.contains("no message"), "{text}");
    // delete retained: one, then the subtree
    let (err, text, _) = call(&c, "delete_retained", json!({ "connection": id, "topic": "plant/r204/state" })).await;
    assert!(!err, "{text}");
    // the broker's count (the Explorer keeps the retained mark until the next retained delivery, as in the window)
    let m = crate::broker::test_manager(&e.link.state.broker);
    until(|| m.status().retained == 1).await;
    let (err, text, _) = call(&c, "delete_retained", json!({ "connection": id, "topic": "plant", "subtree": true })).await;
    assert!(!err && text.contains("Deleted 2 retained"), "{text}");
    until(|| m.status().retained == 0).await;
    let (err, text, _) = call(&c, "delete_retained", json!({ "connection": id, "topic": "nothing", "subtree": true })).await;
    assert!(err && text.contains("no retained message"), "{text}");
    // bounds of list_topics and search_values
    for i in 0..250 {
        let req = crate::types::PublishRequest { topic: format!("many/t{i:03}"), payload: format!("match {i}"), format: "text".into(), qos: 0, retain: false, props: None, request: None };
        crate::commands::do_publish(&conn, &req, "pending", false).unwrap();
    }
    until(|| lock(&conn.engine).known_topics("many/", 1000).len() == 250).await;
    let (_, text, _) = call(&c, "list_topics", json!({ "connection": id, "filter": "many/#", "limit": 1000 })).await;
    let d = data(&text);
    assert_eq!((d["shown"].as_u64(), d["total"].as_u64(), d["truncated"].as_bool()), (Some(200), Some(250), Some(true)), "{text}");
    let (_, text, _) = call(&c, "search_values", json!({ "connection": id, "text": "match", "limit": 1000 })).await;
    let d = data(&text);
    assert_eq!(d["hits"].as_array().unwrap().len(), 100);
    assert_eq!(d["total"], 250);
    // a connection that is not open
    let (_, text, _) = call(&c, "create_connection", json!({ "host": "idle.example.com", "protocol": "mqtt" })).await;
    let idle = data(&text)["id"].as_str().unwrap().to_string();
    let (err, text, _) = call(&c, "list_topics", json!({ "connection": idle })).await;
    assert!(err && text.contains("not connected"), "{text}");
    c.cancel().await.unwrap();
    crate::broker::test_manager(&e.link.state.broker).stop().await;
}

/// A 1×1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

/// M-14: chart tools (dashboard charts, topic charts, sparklines, CSV and PNG exports), the new resources and prompts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m14_chart_tools_resources_and_prompts() {
    use base64::Engine as _;
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let st = &e.link.state;
    let id = builtin(&c).await;
    let conn = conn_of(&e, &id);
    for v in [20.5, 21.0, 21.5] {
        let req = crate::types::PublishRequest { topic: "plant/r204/temp".into(), payload: format!("{{\"t\":{v},\"h\":40}}"), format: "json".into(), qos: 0, retain: false, props: None, request: None };
        crate::commands::do_publish(&conn, &req, "pending", false).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    until(|| lock(&conn.engine).history("plant/r204/temp").len() == 3).await;
    let (err, text, _) = call(&c, "list_charts", json!({})).await;
    assert!(!err && text.contains("no chart"), "{text}");
    let (err, text, _) = call(&c, "create_chart", json!({ "name": "Room 204" })).await;
    assert!(!err, "{text}");
    let chart = data(&text)["id"].as_str().unwrap().to_string();
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Charts { charts } if charts.iter().any(|ch| ch.id == chart && ch.conn_id == id))));
    let (err, text, _) = call(&c, "add_series", json!({ "chart": chart, "topic": "plant/r204/temp", "field_path": "t", "unit": "°C" })).await;
    assert!(!err, "{text}");
    let d = data(&text);
    assert_eq!(d["points"], 3, "{text}");
    assert_eq!(d["seriesData"]["path"], "$.t");
    assert_eq!(d["seriesData"]["name"], "t");
    let series = d["series"].as_str().unwrap().to_string();
    let (_, text, _) = call(&c, "add_series", json!({ "chart": "Room 204", "topic": "plant/r204/temp", "field_path": "$.t" })).await;
    assert_eq!(data(&text)["added"], false, "{text}");
    let (err, text, _) = call(&c, "add_series", json!({ "chart": chart, "topic": "plant/r204/temp", "field_path": "h", "colour": "slate" })).await;
    assert!(!err, "{text}");
    let h = data(&text)["series"].as_str().unwrap().to_string();
    let (err, text, _) = call(&c, "update_series", json!({ "chart": chart, "series": series, "colour": "clay", "style": "step", "axis": "right" })).await;
    assert!(!err, "{text}");
    let (err, text, _) = call(&c, "update_series", json!({ "chart": chart, "series": series, "colour": "purple" })).await;
    assert!(err, "{text}");
    {
        let b = lock(&st.charts);
        let s = &b.charts[0].series[0];
        assert_eq!((s.color.as_str(), s.style.as_str(), s.axis.as_str(), s.unit.as_str()), ("clay", "step", "right", "°C"));
        assert_eq!(b.charts[0].series[1].color, "slate");
    }
    let (err, text, _) = call(&c, "rename_chart", json!({ "id": chart, "name": "Room 204 climate" })).await;
    assert!(!err, "{text}");
    let (_, text, _) = call(&c, "list_charts", json!({})).await;
    assert!(text.contains("Room 204 climate (2)"), "{text}");
    // exports
    let out = tempfile::tempdir().unwrap();
    let csv = out.path().join("room.csv");
    let (err, text, _) = call(&c, "export_chart", json!({ "id": chart, "format": "csv", "file": csv })).await;
    assert!(!err, "{text}");
    let body = std::fs::read_to_string(&csv).unwrap();
    assert!(body.starts_with("time,t,h\n") && body.lines().count() == 4, "{body}");
    let png = out.path().join("room.png");
    answer(&e, AssistantAnswer { data: Some(PNG.into()), path: Some(png.to_string_lossy().to_string()), ..Default::default() });
    let (err, text, _) = call(&c, "export_chart", json!({ "id": chart, "format": "png" })).await;
    assert!(!err, "{text}");
    assert_eq!(std::fs::read(&png).unwrap(), base64::engine::general_purpose::STANDARD.decode(PNG).unwrap());
    assert!(e.link.events().iter().any(|x| matches!(x, AssistantEvent::Request { request, chart_id: Some(ch), .. } if request == "chartPng" && *ch == chart)));
    // topic chart and sparkline, saved with the profile
    let (err, text, _) = call(&c, "set_topic_chart", json!({ "connection": id, "topic": "plant/r204/temp", "fields": ["t", { "path": "$.h", "unit": "%", "style": "points" }] })).await;
    assert!(!err, "{text}");
    let prefs = st.store.get(&id).unwrap().profile.topics["plant/r204/temp"].clone();
    let fields = prefs.chart_fields.unwrap();
    assert_eq!(fields.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["$.t", "$.h"]);
    assert_ne!(fields[0].colour, fields[1].colour);
    assert_eq!((fields[1].unit.as_str(), fields[1].style.as_str()), ("%", "points"));
    let (err, _, _) = call(&c, "set_topic_chart", json!({ "connection": id, "topic": "plant/r204/temp", "fields": ["$.h"] })).await;
    assert!(!err);
    let kept = st.store.get(&id).unwrap().profile.topics["plant/r204/temp"].chart_fields.clone().unwrap();
    assert_eq!(kept[0].unit, "%", "a field kept keeps its settings");
    let (err, text, _) = call(&c, "set_sparkline", json!({ "connection": id, "topic": "plant/r204/temp", "path": "t" })).await;
    assert!(!err, "{text}");
    assert_eq!(st.store.get(&id).unwrap().profile.topics["plant/r204/temp"].sparkline_path.as_deref(), Some("$.t"));
    let (err, _, _) = call(&c, "set_sparkline", json!({ "connection": id, "topic": "plant/r204/temp", "off": true })).await;
    assert!(!err);
    assert!(st.store.get(&id).unwrap().profile.topics["plant/r204/temp"].sparkline_off);
    let (err, _, _) = call(&c, "set_topic_chart", json!({ "connection": id, "topic": "plant/r204/temp", "fields": [] })).await;
    assert!(!err);
    assert!(st.store.get(&id).unwrap().profile.topics["plant/r204/temp"].chart_fields.is_none());
    let (_, text, _) = call(&c, "list_charts", json!({})).await;
    assert_eq!(data(&text)["sparklines"][0]["off"], true, "{text}");
    // resources
    let res = c.list_all_resources().await.unwrap();
    for uri in ["mqttpanel://broker", "mqttpanel://charts", &format!("mqttpanel://topics/{id}")] {
        assert!(res.iter().any(|r| r.uri == *uri), "{uri}");
        let r = serde_json::to_string(&c.read_resource(ReadResourceRequestParams::new(uri)).await.unwrap()).unwrap();
        assert!(r.len() > 20, "{uri}: {r}");
    }
    let r = serde_json::to_string(&c.read_resource(ReadResourceRequestParams::new("mqttpanel://topics/Local broker")).await.unwrap()).unwrap();
    assert!(r.contains("plant/r204/temp"), "{r}");
    assert!(c.read_resource(ReadResourceRequestParams::new("mqttpanel://topics/nothing")).await.is_err());
    let templates = c.list_all_resource_templates().await.unwrap();
    assert!(templates.iter().any(|t| t.uri_template == "mqttpanel://topics/{connection}"));
    // prompts
    let prompts = c.list_all_prompts().await.unwrap();
    assert!(prompts.iter().any(|p| p.name == "setup_builtin_broker") && prompts.iter().any(|p| p.name == "chart_this_value"));
    let mut args = serde_json::Map::new();
    args.insert("devices".into(), json!("12 room sensors with TLS"));
    let p = c.get_prompt(GetPromptRequestParams::new("setup_builtin_broker").with_arguments(args)).await.unwrap();
    let body = serde_json::to_string(&p).unwrap();
    assert!(body.contains("connect_builtin_broker") && body.contains("12 room sensors") && body.contains("\\\"state\\\": \\\"running\\\""), "{body}");
    let p = c.get_prompt(GetPromptRequestParams::new("chart_this_value")).await.unwrap();
    assert!(serde_json::to_string(&p).unwrap().contains("Room 204 climate"));
    // remove
    let (err, _, _) = call(&c, "remove_series", json!({ "chart": chart, "series": h })).await;
    assert!(!err);
    let (err, text, _) = call(&c, "delete_chart", json!({ "id": chart })).await;
    assert!(!err, "{text}");
    assert!(lock(&st.charts).charts.is_empty());
    let (err, text, _) = call(&c, "show_chart", json!({ "id": chart })).await;
    assert!(!err, "{text}");
    // French
    st.assistant.set_lang("fr");
    let (_, text, _) = call(&c, "list_charts", json!({})).await;
    assert!(text.contains("Graphiques"), "{text}");
    c.cancel().await.unwrap();
    crate::broker::test_manager(&st.broker).stop().await;
}

/// M-15: no result, resource or prompt of the broker, data and chart tools contains a broker user password, the
/// generated password of mqtt-panel, a PKCS#12 passphrase, an issued key, the desktop key or the CA key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m15_no_secret_in_broker_data_and_chart_results() {
    let e = env().await;
    accept(&e);
    let c = client(&e, "Test host").await;
    let st = &e.link.state;
    let m = crate::broker::test_manager(&st.broker);
    let mut results: Vec<String> = vec![];
    require_client_certs(&c).await;
    let (_, text, raw) = call(&c, "connect_builtin_broker", json!({})).await;
    results.push(raw);
    let id = data(&text)["id"].as_str().unwrap().to_string();
    let user_password = "Broker-Pa55-Direct-77";
    let passphrase = "P12-Pa55phrase-33";
    results.push(call(&c, "broker_add_user", json!({ "username": "line-3", "password": user_password })).await.2);
    let out = tempfile::tempdir().unwrap();
    answer(&e, folder_answer(out.path()));
    results.push(call(&c, "broker_issue_client_cert", json!({ "name": "dev-p12", "format": "pkcs12", "passphrase": passphrase })).await.2);
    results.push(call(&c, "broker_issue_client_cert", json!({ "name": "dev-pem" })).await.2);
    results.push(call(&c, "publish", json!({ "connection": id, "topic": "plant/a", "payload": "{\"v\":1}", "format": "json" })).await.2);
    until(|| lock(&conn_of(&e, &id).engine).topic_count() >= 1).await;
    let (_, text, raw) = call(&c, "create_chart", json!({ "name": "Scan" })).await;
    results.push(raw);
    let chart = data(&text)["id"].as_str().unwrap().to_string();
    let (_, text, raw) = call(&c, "add_series", json!({ "chart": chart, "topic": "plant/a", "field_path": "v" })).await;
    results.push(raw);
    let series = data(&text)["series"].as_str().unwrap().to_string();
    for t in registry().into_iter().filter(|t| !PHASE_1_2.contains(&t.name)) {
        if t.name == "broker_issue_client_cert" {
            answer(&e, folder_answer(out.path()));
        } else {
            answer(&e, AssistantAnswer { data: Some(PNG.into()), path: Some(out.path().join("x.png").to_string_lossy().to_string()), ..Default::default() });
        }
        let args = match t.name {
            "broker_add_listener" => json!({ "kind": "tcp", "port": free_port(), "preset": "plain" }),
            "broker_update_listener" => json!({ "id": "tcp", "enabled": true }),
            "broker_remove_listener" => json!({ "id": "nothing" }),
            "broker_set_reachability" => json!({ "reach": "this_computer" }),
            "broker_set_anonymous" => json!({ "on": false }),
            "broker_add_user" => json!({ "username": "line-4", "password": "Other-Pa55-88" }),
            "broker_remove_user" => json!({ "username": "line-4" }),
            "broker_issue_client_cert" => json!({ "name": "dev-scan" }),
            "broker_revoke_client_cert" => json!({ "name": "dev-scan" }),
            "broker_set_client_cert_rule" => json!({ "rule": "required" }),
            "broker_export_ca" | "broker_export_server_cert" => json!({ "folder": out.path() }),
            "broker_regenerate_server_cert" => json!({ "extra_names": ["gw.example.com"] }),
            "broker_log" => json!({ "level": "debug", "limit": 500 }),
            // connect_builtin_broker ran above (it starts a fresh connection); stop and rotate run last.
            "broker_stop" | "broker_rotate_ca" | "connect_builtin_broker" => continue,
            "list_topics" | "list_charts" | "broker_status" | "broker_start" | "broker_client_certs" => json!({ "connection": id }),
            "get_topic" | "get_history" => json!({ "connection": id, "topic": "plant/a" }),
            "search_values" => json!({ "connection": id, "text": "1" }),
            "publish" => json!({ "connection": id, "topic": "plant/b", "payload": "2" }),
            "delete_retained" => json!({ "connection": id, "topic": "plant/b" }),
            "export_data" => json!({ "connection": id, "topic": "plant/a", "file": out.path().join("a.json"), "format": "json" }),
            "create_chart" => json!({ "name": "Other" }),
            "rename_chart" => json!({ "id": chart, "name": "Scan 2" }),
            "delete_chart" => json!({ "id": "Other" }),
            "add_series" => json!({ "chart": chart, "topic": "plant/a" }),
            "update_series" => json!({ "chart": chart, "series": series, "unit": "x" }),
            "remove_series" => json!({ "chart": chart, "series": "nothing" }),
            "set_topic_chart" => json!({ "connection": id, "topic": "plant/a", "fields": ["v"] }),
            "set_sparkline" => json!({ "connection": id, "topic": "plant/a", "path": "v" }),
            "export_chart" => json!({ "id": chart, "format": "png" }),
            other => panic!("no arguments for {other}"),
        };
        let x = call(&c, t.name, args.clone()).await;
        assert!(!x.0 || ["broker_remove_listener", "remove_series"].contains(&t.name), "{}: {}", t.name, x.1);
        results.push(x.2);
    }
    results.push(call(&c, "broker_rotate_ca", json!({})).await.2);
    for uri in ["mqttpanel://broker", "mqttpanel://charts", &format!("mqttpanel://topics/{id}"), "mqttpanel://assistant-log", "mqttpanel://connections"] {
        results.push(serde_json::to_string(&c.read_resource(ReadResourceRequestParams::new(uri)).await.unwrap()).unwrap());
    }
    for p in ["setup_builtin_broker", "chart_this_value"] {
        results.push(serde_json::to_string(&c.get_prompt(GetPromptRequestParams::new(p)).await.unwrap()).unwrap());
    }
    results.push(serde_json::to_string(&st.assistant.log()).unwrap());
    let secret = |k: &str| st.store.secrets().get(k).unwrap().unwrap_or_else(|| panic!("secret {k}"));
    let key_body = |pem: &str| pem.lines().filter(|l| !l.starts_with("-----")).collect::<String>();
    let desktop_key = secret(&secret_keys::client_key(&id));
    let pem_key = std::fs::read_to_string(out.path().join("dev-pem.key")).unwrap();
    let secrets = [
        user_password.to_string(),
        passphrase.to_string(),
        "Other-Pa55-88".to_string(),
        m.user_password("mqtt-panel").unwrap().unwrap(),
        key_body(&desktop_key),
        key_body(&pem_key),
        key_body(&secret("broker/ca-key")),
    ];
    for (i, res) in results.iter().enumerate() {
        let flat = res.replace("\\n", "");
        for s in &secrets {
            assert!(!flat.contains(&s[..s.len().min(40)]), "result {i} contains a secret: {res}");
        }
    }
    c.cancel().await.unwrap();
    m.stop().await;
}

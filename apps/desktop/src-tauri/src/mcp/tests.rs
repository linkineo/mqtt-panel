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
    // every other tool, with plausible arguments
    for t in registry() {
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
        }
        if schema["properties"]["ask_in_app"].is_object() {
            assert_eq!(t.wait, super::tools::Wait::MaybeUser, "{}", t.name);
        }
    }
    for w in ["create_connection", "update_connection", "connect", "disconnect", "delete_connection", "trust_certificate"] {
        assert!(all.iter().find(|t| t.name == w).unwrap().writes, "{w}");
    }
    for r in ["list_connections", "get_client_csr", "test_connection", "show_connection", "show_settings"] {
        assert!(!all.iter().find(|t| t.name == r).unwrap().writes, "{r}");
    }
}

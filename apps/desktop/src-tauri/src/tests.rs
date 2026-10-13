//! Adapter tests against a local Mosquitto (F-03 harness; skipped when Mosquitto is not installed).

use crate::commands::{do_publish, start_repeat_on};
use crate::state::*;
use crate::types::*;
use mqttpanel_client::{ConnectOptions, Outgoing, Props5};
use mqttpanel_profiles::{Profile, Settings, StoredProfile};
use std::sync::Arc;
use std::time::Duration;

struct Mosquitto {
    child: std::process::Child,
    port: u16,
    _dir: tempfile::TempDir,
}

impl Drop for Mosquitto {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn mosquitto_bin() -> Option<std::path::PathBuf> {
    ["/opt/homebrew/opt/mosquitto/sbin/mosquitto", "/usr/local/opt/mosquitto/sbin/mosquitto", "/usr/sbin/mosquitto"]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
}

async fn mosquitto() -> Option<Mosquitto> {
    let Some(bin) = mosquitto_bin() else {
        eprintln!("SKIP: mosquitto not found");
        return None;
    };
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
        l.local_addr().ok()?.port()
    };
    let dir = tempfile::tempdir().ok()?;
    let conf = dir.path().join("m.conf");
    std::fs::write(&conf, format!("listener {port} 127.0.0.1\nallow_anonymous true\n")).ok()?;
    let child = std::process::Command::new(bin).arg("-c").arg(&conf).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Some(Mosquitto { child, port, _dir: dir });
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

async fn open(port: u16) -> Arc<Conn> {
    let mut p = Profile::new_default();
    p.host = "127.0.0.1".into();
    p.port = port;
    p.mqtt_version = "5".into();
    let stored = StoredProfile { profile: p, blobs: Default::default() };
    let c = Arc::new(Conn::new(&stored, &Settings::default()));
    let mut o = ConnectOptions::new("127.0.0.1", port);
    o.version = mqttpanel_client::VersionPref::V5;
    o.subscriptions = vec![("#".into(), 0)];
    o.reconnect = false;
    let (h, info, rx) = mqttpanel_client::connect(o).await.expect("connect");
    on_connected(&c, &info);
    *lock(&c.handle) = Some(h);
    tokio::spawn(pump(c.clone(), rx));
    c
}

fn req(topic: &str, payload: &str) -> PublishRequest {
    PublishRequest { topic: topic.into(), payload: payload.into(), format: "json".into(), qos: 1, retain: false, props: None, request: None }
}

async fn wait_for<F: Fn() -> bool>(f: F, secs: u64) -> bool {
    for _ in 0..(secs * 20) {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    f()
}

/// P-05: the reply on the response topic with the right correlation is shown with its round-trip time;
/// a reply with another correlation is ignored.
#[tokio::test]
async fn p05_request_response() {
    let Some(m) = mosquitto().await else { return };
    let c = open(m.port).await;
    // Responder: echoes requests with the correlation data, and also sends one wrong reply first.
    let mut o = ConnectOptions::new("127.0.0.1", m.port);
    o.version = mqttpanel_client::VersionPref::V5;
    o.subscriptions = vec![("dev/cmd".into(), 1)];
    o.reconnect = false;
    let (rh, _, mut rrx) = mqttpanel_client::connect(o).await.unwrap();
    tokio::spawn(async move {
        while let Some(ev) = rrx.recv().await {
            if let mqttpanel_client::ClientEvent::Message(msg) = ev {
                let p = msg.props.unwrap_or_default();
                let topic = p.response_topic.clone().unwrap_or_default();
                let wrong = Props5 { correlation_data: Some(b"c-9999".to_vec()), ..Default::default() };
                rh.publish(Outgoing { topic: topic.clone(), payload: b"{\"ok\":false}".to_vec(), qos: 1, retain: false, props: Some(wrong) });
                let right = Props5 { correlation_data: p.correlation_data.clone(), ..Default::default() };
                rh.publish(Outgoing { topic, payload: b"{\"ok\":true}".to_vec(), qos: 1, retain: false, props: Some(right) });
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut r = req("dev/cmd", "{\"cmd\":\"restart\"}");
    r.request = Some(RequestSpec { response_topic: "dev/cmd/ack".into(), correlation_data: String::new(), wait_s: 5.0 });
    let entry = do_publish(&c, &r, "pending", true).unwrap();
    let corr = entry.request.as_ref().unwrap().correlation_data.clone();
    assert!(corr.starts_with("c-"));
    let id = entry.id.clone();
    let got = wait_for(|| lock(&c.sent).iter().any(|e| e.id == id && e.reply.is_some()), 5).await;
    assert!(got, "no reply");
    let e = lock(&c.sent).iter().find(|e| e.id == id).cloned().unwrap();
    let reply = e.reply.unwrap();
    assert_eq!(reply.payload_text, "{\"ok\":true}");
    assert_eq!(reply.correlation_data, corr);
    assert!(reply.rtt_ms >= 0);
    assert_eq!(e.status, "acked");
}

/// P-01 through the adapter: acked status for QoS 1, sent for QoS 0; the message lands in the engine.
#[tokio::test]
async fn p01_adapter_statuses_and_engine() {
    let Some(m) = mosquitto().await else { return };
    let c = open(m.port).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let a = do_publish(&c, &req("building/f2/r204/setpoint", "{\"value\":21.5}"), "pending", true).unwrap();
    let mut q0 = req("building/f2/r204/temperature", "22.4");
    q0.qos = 0;
    q0.format = "text".into();
    let b = do_publish(&c, &q0, "pending", true).unwrap();
    assert!(wait_for(|| lock(&c.sent).iter().any(|e| e.id == a.id && e.status == "acked"), 3).await);
    assert!(wait_for(|| lock(&c.sent).iter().any(|e| e.id == b.id && e.status == "sent"), 3).await);
    assert!(wait_for(|| lock(&c.engine).topic_count() >= 2, 3).await);
    let d = lock(&c.engine).topic_detail("building/f2/r204/setpoint");
    assert!(d.exists);
}

/// P-04: repeat every 0.2 s with a count; and a repeat stopped by a disconnect.
#[tokio::test]
async fn p04_repeat_count_and_stop_on_disconnect() {
    let Some(m) = mosquitto().await else { return };
    let c = open(m.port).await;
    let mut r = req("sim/room/temperature", "21.5");
    r.format = "text".into();
    r.qos = 0;
    start_repeat_on(c.clone(), r.clone(), 0.2, Some(3)).unwrap();
    assert!(wait_for(|| lock(&c.repeat).as_ref().is_some_and(|x| !x.status.running), 5).await);
    assert_eq!(lock(&c.repeat).as_ref().unwrap().status.sent, 3);
    // Unlimited, then the connection drops.
    start_repeat_on(c.clone(), r, 0.2, None).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(m);
    assert!(wait_for(|| lock(&c.repeat).as_ref().is_some_and(|x| !x.status.running), 5).await);
    let st = lock(&c.repeat).as_ref().unwrap().status.clone();
    assert_eq!(st.stopped_reason.unwrap().key, "repeat.stoppedDisconnect");
}

/// The EngineClient contract (client.ts) and the registered Tauri commands agree on names and parameters.
#[test]
fn contract_ts_methods_match_rust_commands() {
    let ts = include_str!("../../../../packages/engine-client/src/client.ts");
    let body = &ts[ts.find("export interface EngineClient {").unwrap()..ts.find("export interface ContextMenuItem").unwrap()];
    let main_rs = include_str!("main.rs");
    let sources = [include_str!("commands.rs"), include_str!("broker.rs")].concat();
    let snake = |s: &str| {
        let mut o = String::new();
        for c in s.chars() {
            if c.is_ascii_uppercase() {
                o.push('_');
                o.push(c.to_ascii_lowercase());
            } else {
                o.push(c);
            }
        }
        o
    };
    let mut checked = 0;
    for line in body.lines() {
        let l = line.trim();
        let Some(open) = l.find('(') else { continue };
        if !l.contains("): Promise<") || l.starts_with("//") || l.starts_with('*') {
            continue;
        }
        let name = &l[..open];
        if name == "onEvent" || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let close = l.find("): Promise<").unwrap();
        let params: Vec<String> = l[open + 1..close].split(", ").filter(|p| !p.is_empty()).map(|p| snake(p.split(':').next().unwrap().trim())).collect();
        let cmd = snake(name);
        assert!(main_rs.contains(&format!("::{cmd},")), "command {cmd} not registered in main.rs");
        let sig_start = sources
            .find(&format!("pub fn {cmd}("))
            .or_else(|| sources.find(&format!("pub async fn {cmd}(")))
            .or_else(|| sources.find(&format!("    {cmd}(")))
            .unwrap_or_else(|| panic!("no Rust function for {cmd}"));
        let sig = &sources[sig_start..];
        let sig = &sig[sig.find('(').unwrap() + 1..sig.find(')').unwrap()];
        let mut parts = vec![];
        let (mut depth, mut cur) = (0i32, String::new());
        for ch in sig.chars() {
            match ch {
                '<' | '(' => depth += 1,
                '>' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(std::mem::take(&mut cur));
                    continue;
                }
                _ => {}
            }
            cur.push(ch);
        }
        parts.push(cur);
        let rust_params: Vec<String> = parts
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| p.split(':').next().unwrap().trim().trim_start_matches("mut ").to_string())
            .filter(|p| p != "state" && p != "app")
            .collect();
        assert_eq!(rust_params, params, "parameters of {cmd}");
        checked += 1;
    }
    assert!(checked >= 85, "only {checked} methods checked");
}

/// Q-02 (pipeline part): 5 000 msg/s for 10 s over 2 000 topics through Mosquitto, the client and the adapter pump
/// into the engine. Prints the delivered count and the lag after the last send. Opt-in (performance, nightly).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "performance; run with --ignored --nocapture"]
async fn q02_pipeline_5k_msgs_per_second() {
    let Some(m) = mosquitto().await else { return };
    let c = open(m.port).await;
    let mut o = ConnectOptions::new("127.0.0.1", m.port);
    o.reconnect = false;
    let (ph, _, mut prx) = mqttpanel_client::connect(o).await.unwrap();
    tokio::spawn(async move { while prx.recv().await.is_some() {} });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let total = 50_000u64;
    let start = std::time::Instant::now();
    for i in 0..total {
        let topic = format!("building/f{}/r{}/sensor{}", i % 10, (i / 10) % 20, i % 2000 / 200);
        let payload = format!("{{\"value\":{}.{},\"unit\":\"C\"}}", 20 + i % 5, i % 10).into_bytes();
        ph.publish(Outgoing { topic, payload, qos: 0, retain: false, props: None });
        if i % 500 == 499 {
            // pace at 5 000 msg/s
            let target = Duration::from_micros((i + 1) * 200);
            if let Some(wait) = target.checked_sub(start.elapsed()) {
                tokio::time::sleep(wait).await;
            }
        }
    }
    let sent_at = start.elapsed();
    let mut lag = Duration::ZERO;
    let t0 = std::time::Instant::now();
    loop {
        let got = lock(&c.engine).memory().messages;
        let _ = got;
        let count: u64 = {
            let e = lock(&c.engine);
            let d = e.msg_rate(now_ms());
            let _ = d;
            e.seq()
        };
        if count >= total || t0.elapsed() > Duration::from_secs(5) {
            lag = lag.max(t0.elapsed());
            println!("Q-02 pipeline: sent {total} in {:.2} s ({:.0} msg/s), engine seq {count}, lag after last send {} ms, topics {}", sent_at.as_secs_f64(), total as f64 / sent_at.as_secs_f64(), lag.as_millis(), lock(&c.engine).topic_count());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(lock(&c.engine).seq() >= total * 99 / 100);
}

// ---------------------------------------------------------------- "Connect to this app's broker" (B-12, C-13)

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// An AppState with in-memory secrets, a temporary data folder and a broker on free localhost ports, plus a
/// mutual-TLS listener (the most secure one).
fn local_state(dir: &std::path::Path) -> (AppState, String) {
    use mqttpanel_broker::*;
    let secrets: Arc<dyn mqttpanel_profiles::SecretStore> = Arc::new(mqttpanel_profiles::MemorySecrets::new());
    let store = mqttpanel_profiles::Store::new(dir.join("profiles"), secrets.clone());
    let m = BrokerManager::open(&dir.join("broker"), secrets).unwrap();
    m.set_bind(Bind::Localhost).unwrap();
    for mut l in m.config().listeners {
        l.port = free_port();
        // the client certificate requirement is broker-wide: keep one TLS listener, the mutual-TLS one added below
        l.enabled = l.enabled && l.kind != ListenerKind::Tls;
        m.update_listener(l).unwrap();
    }
    let cfg = m
        .add_listener(AddListenerRequest { preset: Preset::Mtls, kind: ListenerKind::Tls, port: u32::from(free_port()) })
        .unwrap();
    assert!(cfg.require_client_cert);
    let mtls = cfg.listeners.iter().find(|l| l.preset == Preset::Mtls).unwrap().id.clone();
    let state = AppState {
        store,
        settings: std::sync::Mutex::new(Settings::default()),
        conns: std::sync::Mutex::new(Default::default()),
        pending_trust: std::sync::Mutex::new(Default::default()),
        broker: crate::broker::BrokerState::with_manager(m),
    };
    (state, mtls)
}

async fn run_flow(state: &AppState, listener: &str) -> Vec<String> {
    let mut out = vec![];
    for step in ["user", "cert", "trust", "broker", "connect"] {
        let r = crate::broker::app_broker_step_on(state, listener, step, "Local broker").await.unwrap_or_else(|e| panic!("step {step}: {}", e.text));
        if step == "connect" {
            if let Some(ConnectOutcome::Failed { error }) = &r.outcome {
                panic!("connect failed: {}", error.text);
            }
        }
        if step == "user" && r.result != "skipped" {
            assert!(r.password.as_ref().is_some_and(|p| p.len() >= 20));
        }
        out.push(r.result);
    }
    out
}

fn manager(state: &AppState) -> &mqttpanel_broker::BrokerManager {
    // Test-only access to the manager opened by `local_state`.
    crate::broker::test_manager(&state.broker)
}

/// B-12: preselects the most secure enabled listener, creates "mqtt-panel" once, issues "mqttpanel-desktop" only
/// when required, restarts only when needed, connects, and keeps a single profile "Local broker".
#[tokio::test]
async fn b12_this_apps_broker_flow_three_runs_no_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path());
    let m = manager(&state);
    assert_eq!(m.preferred_listener().unwrap().id, mtls);
    assert!(!m.is_running());
    let t0 = std::time::Instant::now();
    assert_eq!(run_flow(&state, &mtls).await, ["created", "issued", "trusted", "started", "connected"]);
    assert!(t0.elapsed() < Duration::from_secs(5), "connected within 5 s from a stopped broker: {:?}", t0.elapsed());
    for _ in 0..2 {
        assert_eq!(run_flow(&state, &mtls).await, ["kept", "kept", "trusted", "running", "connected"]);
    }
    let locals: Vec<_> = state.store.list().into_iter().filter(|p| p.profile.is_local_broker).collect();
    assert_eq!(locals.len(), 1, "one profile Local broker");
    let p = &locals[0].profile;
    assert_eq!((p.name.as_str(), p.username.as_str(), p.scheme.as_str(), p.mqtt_version.as_str()), ("Local broker", "mqtt-panel", "mqtts", "5"));
    assert!(p.has_password && p.tls.verify_server && p.tls.ca_cert.is_some() && p.tls.client_cert.is_some());
    let users = m.config().users;
    assert_eq!(users.iter().filter(|u| u.username == "mqtt-panel").count(), 1, "{users:?}");
    assert!(users[0].system);
    // The profile password is the user's password and can be revealed (app-owned).
    assert_eq!(crate::commands::local_profile_password(&state, &p.id), m.user_password("mqtt-panel").unwrap());
    assert_eq!(m.issued_certs().iter().filter(|c| c.system).count(), 1);
    // A pending listener change: the flow restarts the broker by itself.
    let mut tcp = m.config().listeners.into_iter().find(|l| l.id == "tcp").unwrap();
    tcp.port = free_port();
    m.update_listener(tcp).unwrap();
    assert!(m.restart_needed());
    assert_eq!(run_flow(&state, &mtls).await, ["kept", "kept", "trusted", "restarted", "connected"]);
    assert!(!m.restart_needed());
    // A plain listener: no certificate, no CA trust.
    assert_eq!(run_flow(&state, "tcp").await, ["kept", "skipped", "skipped", "running", "connected"]);
    let p = state.store.list().into_iter().find(|p| p.profile.is_local_broker).unwrap().profile;
    assert!(p.tls.ca_cert.is_none() && p.tls.client_cert.is_none() && p.scheme == "mqtt");
    assert_eq!(state.store.list().iter().filter(|p| p.profile.is_local_broker).count(), 1);
    let id = p.id.clone();
    crate::commands::close(&state, &id).await;
    m.stop().await;
}

/// C-13 (adapter part): with Anonymous on no user is created and no client certificate is needed.
#[tokio::test]
async fn c13_this_apps_broker_flow_with_anonymous_on() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path());
    let m = manager(&state);
    m.set_anonymous(true).unwrap();
    assert_eq!(run_flow(&state, &mtls).await, ["skipped", "skipped", "trusted", "started", "connected"]);
    assert!(m.config().users.is_empty(), "no user created");
    let p = state.store.list().into_iter().find(|p| p.profile.is_local_broker).unwrap().profile;
    assert!(p.username.is_empty() && !p.has_password && p.tls.client_cert.is_none());
    crate::commands::close(&state, &p.id).await;
    m.stop().await;
}

fn local_profile(state: &AppState) -> mqttpanel_profiles::StoredProfile {
    state.store.list().into_iter().find(|p| p.profile.is_local_broker).unwrap()
}

/// C-16: the profile "Local broker" follows every broker change at once and reconnects after the restart.
#[tokio::test]
async fn c16_local_broker_profile_follows_the_broker() {
    use mqttpanel_profiles::SecretStore;
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path());
    let m = manager(&state);
    assert_eq!(run_flow(&state, &mtls).await, ["created", "issued", "trusted", "started", "connected"]);
    let id = local_profile(&state).profile.id.clone();
    let open = |state: &AppState| lock(&state.conns).get(&id).map(|c| (c.is_open(), lock(&c.link).state.clone()));
    assert_eq!(open(&state), Some((true, "connected".to_string())));

    // Port of its listener: the profile has the new port at once; the connection runs on until the restart.
    let mut l = m.config().listeners.into_iter().find(|l| l.id == mtls).unwrap();
    let new_port = free_port();
    l.port = new_port;
    m.update_listener(l).unwrap();
    crate::broker::follow_local_profile(&state);
    assert_eq!(local_profile(&state).profile.port, new_port);
    assert!(m.restart_needed());
    assert_eq!(open(&state).unwrap().1, "connected");
    m.restart().await;
    crate::broker::reconnect_local(&state).await;
    assert_eq!(open(&state), Some((true, "connected".to_string())), "reconnected after the restart");
    assert_eq!(m.clients().iter().filter(|c| c.client_id == "mqttpanel-desktop").count(), 1);

    // Reachability is broker-wide (change set 4): the profile keeps localhost and reconnects after the restart.
    m.set_bind(mqttpanel_broker::Bind::Network).unwrap();
    crate::broker::follow_local_profile(&state);
    assert_eq!(local_profile(&state).profile.host, "localhost");
    assert!(m.restart_needed());
    m.restart().await;
    crate::broker::reconnect_local(&state).await;
    assert_eq!(open(&state), Some((true, "connected".to_string())), "reconnected after a reachability change");
    m.set_bind(mqttpanel_broker::Bind::Localhost).unwrap();
    m.restart().await;
    crate::broker::reconnect_local(&state).await;

    // Client certificate requirement off and on again.
    m.set_require_client_cert(false).unwrap();
    crate::broker::follow_local_profile(&state);
    let p = local_profile(&state);
    assert!(p.profile.tls.client_cert.is_none() && p.blobs.client_cert_pem.is_none());
    assert!(!state.store.secrets().contains(&mqttpanel_profiles::model::secret_keys::client_key(&id)).unwrap());
    m.set_require_client_cert(true).unwrap();
    crate::broker::follow_local_profile(&state);
    assert!(local_profile(&state).profile.tls.client_cert.is_some());

    // The mqtt-panel password.
    m.set_user_password("mqtt-panel", "a-new-password-0123456789").unwrap();
    crate::broker::follow_local_profile(&state);
    assert_eq!(state.store.password(&id).as_deref(), Some("a-new-password-0123456789"));

    // A CA rotation: the profile trusts the new CA and carries a re-issued certificate.
    let ca_before = local_profile(&state).blobs.ca_pem.clone();
    let cert_before = local_profile(&state).blobs.client_cert_pem.clone();
    m.rotate_ca().unwrap();
    crate::broker::follow_local_profile(&state);
    let p = local_profile(&state);
    assert_ne!(p.blobs.ca_pem, ca_before);
    assert_ne!(p.blobs.client_cert_pem, cert_before);
    m.restart().await;
    crate::broker::reconnect_local(&state).await;
    assert_eq!(open(&state), Some((true, "connected".to_string())), "connected with the new CA and certificate");

    // Anonymous: no user, no password, no client certificate; off again restores them.
    m.set_anonymous(true).unwrap();
    crate::broker::follow_local_profile(&state);
    let p = local_profile(&state);
    assert!(p.profile.username.is_empty() && !p.profile.has_password && p.profile.tls.client_cert.is_none());
    m.set_anonymous(false).unwrap();
    crate::broker::follow_local_profile(&state);
    let p = local_profile(&state);
    assert_eq!(p.profile.username, "mqtt-panel");
    assert!(p.profile.has_password && p.profile.tls.client_cert.is_some());

    // Its listener switched off: the profile moves to the most secure enabled listener (here plain TCP).
    let mut l = m.config().listeners.into_iter().find(|l| l.id == mtls).unwrap();
    l.enabled = false;
    m.update_listener(l).unwrap();
    crate::broker::follow_local_profile(&state);
    let p = local_profile(&state).profile;
    assert_eq!((p.scheme.as_str(), p.broker_listener_id.as_deref()), ("mqtt", Some("tcp")));
    m.restart().await;
    crate::broker::reconnect_local(&state).await;
    assert_eq!(open(&state), Some((true, "connected".to_string())));
    // Not connected: a restart does not open a connection.
    crate::commands::close(&state, &id).await;
    m.restart().await;
    crate::broker::reconnect_local(&state).await;
    assert!(open(&state).is_none());
    m.stop().await;
}

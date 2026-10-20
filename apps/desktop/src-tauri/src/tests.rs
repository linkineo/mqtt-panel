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
    let sources = [include_str!("commands.rs"), include_str!("broker.rs"), include_str!("mcp/commands.rs"), include_str!("charts.rs")].concat();
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
        let params: Vec<String> = l[open + 1..close].split(", ").filter(|p| !p.is_empty()).map(|p| snake(p.split(':').next().unwrap().trim().trim_end_matches('?'))).collect();
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
async fn local_state(dir: &std::path::Path) -> (AppState, String) {
    use mqttpanel_broker::*;
    let secrets: Arc<dyn mqttpanel_profiles::SecretStore> = Arc::new(mqttpanel_profiles::MemorySecrets::new());
    let store = mqttpanel_profiles::Store::new(dir.join("profiles"), secrets.clone());
    let m = BrokerManager::open(&dir.join("broker"), secrets).unwrap();
    m.set_bind(Bind::Localhost).await.unwrap();
    for mut l in m.config().listeners {
        l.port = free_port();
        // the client certificate requirement is broker-wide: keep one TLS listener, the mutual-TLS one added below
        l.enabled = l.enabled && l.kind != ListenerKind::Tls;
        m.update_listener(l).await.unwrap();
    }
    let cfg = m
        .add_listener(AddListenerRequest { preset: Preset::Mtls, kind: ListenerKind::Tls, port: u32::from(free_port()) }).await
        .unwrap();
    assert!(cfg.require_client_cert);
    let mtls = cfg.listeners.iter().find(|l| l.preset == Preset::Mtls).unwrap().id.clone();
    let state = AppState {
        store,
        settings: std::sync::Mutex::new(Settings::default()),
        conns: std::sync::Mutex::new(Default::default()),
        pending_trust: std::sync::Mutex::new(Default::default()),
        broker: crate::broker::BrokerState::with_manager(m),
        assistant: Default::default(),
        charts: Default::default(),
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
/// when required, starts the broker only when it is stopped (never restarts), connects, and keeps a single profile
/// "Local broker".
#[tokio::test]
async fn b12_this_apps_broker_flow_three_runs_no_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path()).await;
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
    assert!(p.has_password && !p.tls.skip_verify && p.tls.ca_cert.is_some() && p.tls.client_cert.is_some());
    let users = m.config().users;
    assert_eq!(users.iter().filter(|u| u.username == "mqtt-panel").count(), 1, "{users:?}");
    assert!(users[0].system);
    // The profile password is the user's password and can be revealed (app-owned).
    assert_eq!(crate::commands::stored_password(&state, &p.id).unwrap(), m.user_password("mqtt-panel").unwrap());
    assert_eq!(m.issued_certs().iter().filter(|c| c.system).count(), 1);
    // A listener change applies at once: the flow finds the broker running and never restarts it (change set 7).
    let started_at = m.status().started_at;
    let mut tcp = m.config().listeners.into_iter().find(|l| l.id == "tcp").unwrap();
    tcp.port = free_port();
    m.update_listener(tcp).await.unwrap();
    assert_eq!(run_flow(&state, &mtls).await, ["kept", "kept", "trusted", "running", "connected"]);
    assert_eq!(m.status().started_at, started_at, "never restarted");
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
    let (state, mtls) = local_state(dir.path()).await;
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

/// C-16: the profile "Local broker" follows every broker change at once; when connected it reconnects at once if its
/// own listener or login changed and stays connected otherwise (change set 7: nothing restarts).
#[tokio::test]
async fn c16_local_broker_profile_follows_the_broker() {
    use mqttpanel_profiles::SecretStore;
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path()).await;
    let m = manager(&state);
    assert_eq!(run_flow(&state, &mtls).await, ["created", "issued", "trusted", "started", "connected"]);
    let started_at = m.status().started_at;
    let id = local_profile(&state).profile.id.clone();
    let open = |state: &AppState| lock(&state.conns).get(&id).map(|c| (c.is_open(), lock(&c.link).state.clone()));
    let connected = Some((true, "connected".to_string()));
    assert_eq!(open(&state), connected);
    // The broker session of the profile: (connected at, count); a reconnect makes a new one.
    let session = || {
        let cs: Vec<_> = m.clients().into_iter().filter(|c| c.client_id == "mqttpanel-desktop").collect();
        (cs.first().map(|c| c.connected_at), cs.len())
    };
    // One change as the commands apply it: the fingerprint before, the change, then follow and reconnect if needed.
    macro_rules! change {
        ($e:expr) => {{
            let before = crate::broker::local_fingerprint(&state);
            $e;
            crate::broker::follow_and_reconnect(&state, before).await;
        }};
    }
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    // A change that does not touch it (another listener's port, another user): it stays connected, same session.
    let s0 = session();
    change!({
        let mut tcp = m.config().listeners.into_iter().find(|l| l.id == "tcp").unwrap();
        tcp.port = free_port();
        m.update_listener(tcp).await.unwrap();
    });
    change!(m.add_user("someone", "pw-someone-0123").unwrap());
    settle().await;
    assert_eq!(open(&state), connected);
    assert_eq!(session(), s0, "not reconnected");

    // Port of its listener: the profile has the new port and reconnects at once.
    let mut l = m.config().listeners.into_iter().find(|l| l.id == mtls).unwrap();
    let new_port = free_port();
    l.port = new_port;
    change!(m.update_listener(l).await.unwrap());
    assert_eq!(local_profile(&state).profile.port, new_port);
    assert_eq!(open(&state), connected, "reconnected on the new port");
    assert!(tests_support_wait(|| session().1 == 1).await);
    assert_ne!(session().0, s0.0, "a new session");

    // Reachability is broker-wide (change set 4): the profile keeps localhost and is connected afterwards.
    change!(m.set_bind(mqttpanel_broker::Bind::Network).await.unwrap());
    assert_eq!(local_profile(&state).profile.host, "localhost");
    assert_eq!(open(&state), connected, "connected after a reachability change");
    change!(m.set_bind(mqttpanel_broker::Bind::Localhost).await.unwrap());
    assert_eq!(open(&state), connected);

    // Client certificate requirement off and on again: its login changes, it reconnects each time.
    change!(m.set_require_client_cert(false).unwrap());
    let p = local_profile(&state);
    assert!(p.profile.tls.client_cert.is_none() && p.blobs.client_cert_pem.is_none());
    assert!(!state.store.secrets().contains(&mqttpanel_profiles::model::secret_keys::client_key(&id)).unwrap());
    assert_eq!(open(&state), connected);
    change!(m.set_require_client_cert(true).unwrap());
    assert!(local_profile(&state).profile.tls.client_cert.is_some());
    assert_eq!(open(&state), connected);

    // The mqtt-panel password.
    change!(m.set_user_password("mqtt-panel", "a-new-password-0123456789").unwrap());
    assert_eq!(state.store.password(&id).as_deref(), Some("a-new-password-0123456789"));
    assert_eq!(open(&state), connected);

    // A CA rotation: the profile trusts the new CA, carries a re-issued certificate and reconnects with them.
    let ca_before = local_profile(&state).blobs.ca_pem.clone();
    let cert_before = local_profile(&state).blobs.client_cert_pem.clone();
    change!(m.rotate_ca().unwrap());
    let p = local_profile(&state);
    assert_ne!(p.blobs.ca_pem, ca_before);
    assert_ne!(p.blobs.client_cert_pem, cert_before);
    assert_eq!(open(&state), connected, "connected with the new CA and certificate");

    // Anonymous: no user, no password, no client certificate; off again restores them.
    change!(m.set_anonymous(true).unwrap());
    let p = local_profile(&state);
    assert!(p.profile.username.is_empty() && !p.profile.has_password && p.profile.tls.client_cert.is_none());
    assert_eq!(open(&state), connected);
    change!(m.set_anonymous(false).unwrap());
    let p = local_profile(&state);
    assert_eq!(p.profile.username, "mqtt-panel");
    assert!(p.profile.has_password && p.profile.tls.client_cert.is_some());
    assert_eq!(open(&state), connected);

    // Its listener switched off: the profile moves to the most secure enabled listener (here plain TCP).
    let mut l = m.config().listeners.into_iter().find(|l| l.id == mtls).unwrap();
    l.enabled = false;
    change!(m.update_listener(l).await.unwrap());
    let p = local_profile(&state).profile;
    assert_eq!((p.scheme.as_str(), p.broker_listener_id.as_deref()), ("mqtt", Some("tcp")));
    assert_eq!(open(&state), connected);
    assert_eq!(m.status().started_at, started_at, "the broker never restarted");
    // Not connected: a change does not open a connection.
    crate::commands::close(&state, &id).await;
    change!(m.set_user_password("mqtt-panel", "another-password-0123456789").unwrap());
    assert!(open(&state).is_none());
    m.stop().await;
}

/// CONN-001: after a server certificate from another issuer is imported, the trust step saves what the broker serves
/// (the imported CA, or a pin of the served certificate when the import had no CA), the flow connects, and a
/// re-import, "Regenerate" or a CA rotation make "Local broker" follow and reconnect.
#[tokio::test]
async fn c17_imported_server_identity_trust_and_follow() {
    use mqttpanel_tls::kit::{local_sans, pkcs12, Ca, KeyAlg, Usage, Validity};
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path()).await;
    let m = manager(&state);
    let ext = Ca::new_root("QA external CA", KeyAlg::Ecdsa, Validity::years(1)).unwrap();
    let issue = || ext.issue("localhost", &local_sans(), Usage::Server, KeyAlg::Ecdsa, Validity::years(1)).unwrap();
    let fp = mqttpanel_tls::fingerprint_sha256;
    let ca_fp = |p: &mqttpanel_profiles::StoredProfile| {
        p.blobs.ca_pem.as_ref().map(|pem| fp(mqttpanel_tls::parse_certs(pem.as_bytes()).unwrap()[0].as_ref()))
    };
    // Imported before the flow: PKCS#12 with the CA. The trust step saves that CA and no pin; the flow connects.
    let a = issue();
    let p12 = pkcs12(&a.cert_der, &[ext.cert_der.clone()], &a.key_der, "qa", "server").unwrap();
    m.import_server_cert(&p12, None, Some("qa")).unwrap();
    assert_eq!(run_flow(&state, &mtls).await, ["created", "issued", "trusted", "started", "connected"]);
    let p = local_profile(&state);
    assert_eq!(ca_fp(&p), Some(fp(&ext.cert_der)));
    assert!(p.profile.tls.ca_cert.is_some() && p.profile.tls.trusted_fingerprints.is_empty());
    let id = p.profile.id.clone();
    let connected = || lock(&state.conns).get(&id).map(|c| (c.is_open(), lock(&c.link).state.clone())) == Some((true, "connected".to_string()));
    assert!(connected());
    macro_rules! change {
        ($e:expr) => {{
            let before = crate::broker::local_fingerprint(&state);
            $e;
            crate::broker::follow_and_reconnect(&state, before.clone()).await;
            assert_ne!(crate::broker::local_fingerprint(&state), before, "the follow fingerprint changed");
        }};
    }
    // Leaf-only PEM import: no CA known, the served certificate is pinned; it follows and reconnects.
    let b = issue();
    change!(m.import_server_cert(b.cert_pem.as_bytes(), Some(b.key_pem.as_bytes()), None).unwrap());
    let p = local_profile(&state);
    assert!(p.blobs.ca_pem.is_none() && p.profile.tls.ca_cert.is_none());
    assert_eq!(p.profile.tls.trusted_fingerprints, vec![fp(&b.cert_der)]);
    assert!(connected(), "connected with the pinned certificate");
    // The flow again: "trusted" (the pin), connected.
    assert_eq!(run_flow(&state, &mtls).await, ["kept", "kept", "trusted", "running", "connected"]);
    // Another leaf-only import: the pin follows.
    let c = issue();
    change!(m.import_server_cert(c.cert_pem.as_bytes(), Some(c.key_pem.as_bytes()), None).unwrap());
    assert_eq!(local_profile(&state).profile.tls.trusted_fingerprints, vec![fp(&c.cert_der)]);
    assert!(connected());
    // PEM leaf + CA: the CA again, no pin.
    let d = issue();
    change!(m.import_server_cert(format!("{}{}", d.cert_pem, ext.cert_pem).as_bytes(), Some(d.key_pem.as_bytes()), None).unwrap());
    let p = local_profile(&state);
    assert_eq!(ca_fp(&p), Some(fp(&ext.cert_der)));
    assert!(p.profile.tls.trusted_fingerprints.is_empty());
    assert!(connected());
    // "Regenerate": back to the local CA.
    change!(m.regenerate(&[], KeyAlg::Ecdsa).unwrap());
    let p = local_profile(&state);
    assert_eq!(ca_fp(&p), Some(fp(mqttpanel_tls::parse_certs(m.ca_pem().as_bytes()).unwrap()[0].as_ref())));
    assert!(p.profile.tls.trusted_fingerprints.is_empty());
    assert!(connected());
    // Leaf-only import, then a CA rotation (which issues a new server certificate from the new local CA).
    let e = issue();
    change!(m.import_server_cert(e.cert_pem.as_bytes(), Some(e.key_pem.as_bytes()), None).unwrap());
    assert!(connected());
    change!(m.rotate_ca().unwrap());
    let p = local_profile(&state);
    assert_eq!(ca_fp(&p), Some(fp(mqttpanel_tls::parse_certs(m.ca_pem().as_bytes()).unwrap()[0].as_ref())));
    assert!(p.profile.tls.trusted_fingerprints.is_empty());
    assert!(connected(), "connected after the CA rotation");
    crate::commands::close(&state, &id).await;
    m.stop().await;
}

/// Waits up to 5 s for a condition.
pub(crate) async fn tests_support_wait(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    cond()
}

/// C-33 (adapter part): "Generate key and request a certificate". The key goes to the secret store, the CSR carries
/// the chosen common name and is signed by the broker's "Import CSR…", a certificate for another key is refused, the
/// signed certificate is accepted, and the connection authenticates with the generated key against the embedded
/// broker with client certificates required. Start over and switching the way clear the material.
#[tokio::test]
async fn c33_generated_key_csr_and_signed_certificate_connect() {
    use crate::commands::{client_cert_way, client_csr_pem, generate_key_for, import_signed_cert_bytes, stored_password};
    use mqttpanel_profiles::model::secret_keys;
    use mqttpanel_profiles::FileField;
    let dir = tempfile::tempdir().unwrap();
    let (state, mtls) = local_state(dir.path()).await;
    let m = manager(&state);
    assert_eq!(m.start().await.state, mqttpanel_broker::BrokerState::Running);
    let port = m.running_port(&mtls).unwrap();
    let mut p = Profile::new_default();
    p.name = "Generated".into();
    p.host = "localhost".into();
    p.port = port;
    p.scheme = "mqtts".into();
    p.mqtt_version = "5".into();
    p.client_id = "gen-client".into();
    let p = state.store.save(p).unwrap();
    let id = p.id.clone();
    let ca = m.ca_pem();
    state
        .store
        .update(&id, |s| {
            s.blobs.ca_pem = Some(ca.clone());
            s.profile.tls.ca_cert = Some(FileField { file_name: "CA.pem".into(), summary: None, in_keychain: false });
        })
        .unwrap();
    // The password eye reads the stored password of any profile (change set 11).
    state.store.set_password(&id, "typed-secret").unwrap();
    assert_eq!(stored_password(&state, &id).unwrap().as_deref(), Some("typed-secret"));
    state.store.set_password(&id, "").unwrap();

    // Without a generated key there is nothing to request.
    assert_eq!(client_csr_pem(&state, &id, "gen-client").unwrap_err().key, "file.noGeneratedKey");
    // Generate: the key is in the secret store, the profile shows the algorithm and the fingerprint.
    let p = generate_key_for(&state, &id, mqttpanel_tls::kit::KeyAlg::Ecdsa).unwrap();
    assert!(p.tls.client_key_generated && p.tls.client_cert.is_none());
    let s = p.tls.client_key.as_ref().unwrap().summary.as_ref().unwrap();
    assert_eq!((s.kind.as_str(), s.key_type.as_deref()), ("generated", Some("ECDSA P-256")));
    let key = state.store.secrets().get(&secret_keys::client_key(&id)).unwrap().unwrap();
    assert_eq!(mqttpanel_tls::kit::describe_key(&key).unwrap().1, s.fingerprint.clone().unwrap());
    let profile_file = std::fs::read_to_string(dir.path().join("profiles").join("profiles.json")).unwrap_or_default();
    assert!(!profile_file.contains("PRIVATE KEY"), "the key never reaches the profile file");

    // CSR with the chosen common name, signed by the broker.
    assert_eq!(client_csr_pem(&state, &id, "  ").unwrap_err().key, "file.csrNoName");
    let csr = client_csr_pem(&state, &id, "gen-client").unwrap();
    assert!(csr.starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
    let (_, signed) = m.import_csr(csr.as_bytes(), None, 2).unwrap();
    assert!(m.issued_certs().iter().any(|c| c.name == "gen-client"));

    // A certificate for another key is refused; the signed one is accepted (DER too).
    let (_, other) = m
        .issue_client_cert(mqttpanel_broker::IssueCertRequest { name: "someone-else".into(), key_type: mqttpanel_tls::kit::KeyAlg::Ecdsa, valid_years: 1, format: mqttpanel_broker::CertFormat::Pem, passphrase: String::new(), save_ca: false })
        .unwrap()
        .into_iter()
        .find(|(n, _)| n.ends_with(".pem") && !n.contains("CA"))
        .unwrap();
    assert_eq!(import_signed_cert_bytes(&state, &id, "someone-else.pem", &other).unwrap_err().key, "file.notGeneratedKey");
    let der = mqttpanel_tls::parse_certs(&signed).unwrap()[0].as_ref().to_vec();
    let p = import_signed_cert_bytes(&state, &id, "gen-client.der", &der).unwrap();
    assert_eq!(p.tls.client_cert.as_ref().unwrap().file_name, "gen-client.der");
    assert!(p.tls.client_key.is_some(), "the generated key stays");

    // The connection authenticates with the generated key (client certificates required on this listener).
    let stored = state.store.get(&id).unwrap();
    match crate::commands::connect_stored(&state, stored, false).await {
        ConnectOutcome::Connected { .. } => {}
        other => panic!("connect with the generated key: {:?}", serde_json::to_string(&other).unwrap()),
    }
    assert!(m.clients().iter().any(|c| c.client_id == "gen-client" && c.cert_cn.as_deref() == Some("gen-client")));
    crate::commands::close(&state, &id).await;

    // Removing the certificate keeps the key; Start over clears both; switching to Import clears and flips the way.
    let p = client_cert_way(&state, &id, true).unwrap();
    assert!(p.tls.client_key_generated && p.tls.client_key.is_none() && p.tls.client_cert.is_none());
    assert!(state.store.secrets().get(&secret_keys::client_key(&id)).unwrap().is_none());
    generate_key_for(&state, &id, mqttpanel_tls::kit::KeyAlg::Rsa).unwrap();
    let p = client_cert_way(&state, &id, false).unwrap();
    assert!(!p.tls.client_key_generated && p.tls.client_key.is_none());
    assert!(state.store.secrets().get(&secret_keys::client_key(&id)).unwrap().is_none());
    m.stop().await;
}

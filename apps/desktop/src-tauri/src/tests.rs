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

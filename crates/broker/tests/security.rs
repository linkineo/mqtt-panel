//! B-30 to B-33: depth caps, control orders with a full in-flight window, limits before login, web origins.
//! Each test reproduces a proof of concept of the security review first, then checks the fix.

mod common;

use bytes::BytesMut;
use common::*;
use mqttpanel_broker::config::ListenerKind;
use mqttpanel_broker::runtime::{
    AuthConfig, Broker, BrokerHandle, Credential, Limits, ListenerSpec, OriginPrompt,
    OriginRequest, RuntimeConfig,
};
use rumqttc::mqttbytes::v4;
use rumqttc::v5::mqttbytes::v5;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

fn anon() -> AuthConfig {
    AuthConfig {
        anonymous: true,
        ..Default::default()
    }
}

async fn start_with(
    listeners: Vec<ListenerSpec>,
    auth: AuthConfig,
    limits: Limits,
    prompt: Option<OriginPrompt>,
) -> BrokerHandle {
    Broker::start(RuntimeConfig {
        listeners,
        auth,
        limits,
        origin_prompt: prompt,
        ..Default::default()
    })
    .await
    .expect("broker starts")
}

/// Reads and discards bytes (never acknowledges) until the broker closes the connection; false after `d`.
async fn closed_within<S: tokio::io::AsyncRead + Unpin>(s: &mut S, d: Duration) -> bool {
    let mut buf = vec![0u8; 64 * 1024];
    let deadline = tokio::time::Instant::now() + d;
    loop {
        match tokio::time::timeout_at(deadline, s.read(&mut buf)).await {
            Err(_) => return false,
            Ok(Ok(0)) | Ok(Err(_)) => return true,
            Ok(Ok(_)) => {}
        }
    }
}

/// MQTT 5 CONNECT with Receive Maximum 1 (the client takes one QoS 1/2 delivery at a time).
async fn connect_v5_receive_max_1(r: &mut Raw, id: &str, login: Option<(&str, &str)>) -> u8 {
    let mut p = v5::ConnectProperties::new();
    p.receive_maximum = Some(1);
    let c = v5::Connect {
        keep_alive: 60,
        client_id: id.into(),
        clean_start: true,
        properties: Some(p),
    };
    r.send5(v5::Packet::Connect(
        c,
        None,
        login.map(|(u, p)| v5::Login::new(u, p)),
    ))
    .await;
    match r.read().await {
        Pkt::ConnAck { code, .. } => code,
        other => panic!("expected CONNACK, got {other:?}"),
    }
}

/// Subscribes at QoS 1 without reading anything afterwards (the raw reader would acknowledge).
async fn subscribe_q1(r: &mut Raw, filter: &str) {
    assert_eq!(r.subscribe(filter, 1).await, vec![1]);
}

/// Fills a subscriber's window: `n` QoS 1 messages it never acknowledges.
async fn flood(port: u16, topic: &str, n: usize) {
    let mut p = Raw::tcp(port, false).await;
    assert_eq!(
        p.connect(&format!("flood-{topic}"), None, None, 30).await,
        0
    );
    let mut b = BytesMut::new();
    for i in 0..n {
        let mut m = v4::Publish::new(
            topic,
            rumqttc::QoS::AtLeastOnce,
            format!("{i}").into_bytes(),
        );
        m.pkid = (i % 60_000 + 1) as u16;
        v4::Packet::Publish(m).write(&mut b, usize::MAX).unwrap();
    }
    p.send_raw(&b).await.unwrap();
    // Wait for the PUBACKs so the messages are routed.
    let mut acks = 0;
    while acks < n {
        match tokio::time::timeout(T, p.read()).await.expect("pubacks") {
            Pkt::Other(0x40) => acks += 1,
            Pkt::Closed => panic!("publisher closed"),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ B-30 depth caps (SEC-01)

#[tokio::test]
async fn b30_deep_filter_refused_and_broker_keeps_serving() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp)], anon()).await;
    let port = b.port("tcp").unwrap();
    let deep = "/".repeat(60_000);

    // MQTT 5: SUBACK 0x8F (topic filter invalid), the session goes on; then a disconnect (the removal path).
    let mut c5 = Raw::tcp(port, true).await;
    assert_eq!(c5.connect("deep5", None, None, 30).await, 0);
    assert_eq!(c5.subscribe(&deep, 0).await, vec![0x8F]);
    assert_eq!(c5.subscribe("ok/#", 0).await, vec![0]);
    c5.send_raw(&[0xE0, 0x00]).await.unwrap();

    // MQTT 3.1.1: no reason code exists, the connection closes.
    let mut c4 = Raw::tcp(port, false).await;
    assert_eq!(c4.connect("deep4", None, None, 30).await, 0);
    let mut s = v4::Subscribe::new(&deep, rumqttc::QoS::AtMostOnce);
    s.pkid = 1;
    c4.send4(v4::Packet::Subscribe(s)).await;
    assert_eq!(
        tokio::time::timeout(T, c4.read()).await.unwrap(),
        Pkt::Closed
    );

    // A 129-level topic is refused on PUBLISH (MQTT 5: DISCONNECT 0x90).
    let mut p = Raw::tcp(port, true).await;
    assert_eq!(p.connect("deep-pub", None, None, 30).await, 0);
    p.publish(&vec!["x"; 129].join("/"), b"x", false).await;
    assert_eq!(
        tokio::time::timeout(T, p.read()).await.unwrap(),
        Pkt::Disconnect(0x90)
    );

    // A last will with a too deep topic: CONNACK 0x90.
    let mut w = Raw::tcp(port, true).await;
    assert_eq!(
        w.connect(
            "deep-will",
            None,
            Some((&vec!["w"; 200].join("/"), "x")),
            30
        )
        .await,
        0x90
    );

    // 128 levels work end to end.
    let topic128 = vec!["l"; 128].join("/");
    let mut sub = Raw::tcp(port, true).await;
    assert_eq!(sub.connect("sub128", None, None, 30).await, 0);
    assert_eq!(sub.subscribe(&topic128, 0).await, vec![0]);
    let mut pubr = Raw::tcp(port, true).await;
    assert_eq!(pubr.connect("pub128", None, None, 30).await, 0);
    pubr.publish(&topic128, b"deep", false).await;
    match sub.next_publish(T).await {
        Some(Pkt::Publish { topic, .. }) => assert_eq!(topic, topic128),
        other => panic!("expected the 128-level message, got {other:?}"),
    }
    sub.send_raw(&[0xE0, 0x00]).await.unwrap();

    // Still serving.
    let mut ok = Raw::tcp(port, true).await;
    assert_eq!(ok.connect("still-alive", None, None, 30).await, 0);
    b.stop().await;
}

// ------------------------------------------------------------------ B-31 control orders and in-flight timeout (SEC-03)

#[tokio::test]
async fn b31_takeover_with_full_window_closes_old_connection_within_1s() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp)], anon()).await;
    let port = b.port("tcp").unwrap();
    // The reviewer's scenario: a 3.1.1 client keeps 1024 QoS 1 deliveries unacknowledged.
    let mut old = Raw::tcp(port, false).await;
    assert_eq!(old.connect("victim", None, None, 60).await, 0);
    subscribe_q1(&mut old, "fill/#").await;
    flood(port, "fill/a", 1100).await;
    assert!(
        eventually(|| b
            .clients()
            .iter()
            .any(|c| c.client_id == "victim" && c.msg_out >= 1024))
        .await
    );
    // Takeover by the same client id.
    let mut new = Raw::tcp(port, false).await;
    assert_eq!(new.connect("victim", None, None, 60).await, 0);
    let t0 = Instant::now();
    assert!(
        closed_within(&mut old.io, Duration::from_secs(1)).await,
        "old connection still open"
    );
    assert!(t0.elapsed() < Duration::from_secs(1));
    assert!(eventually(|| b.clients().len() == 1).await);
    b.stop().await;
}

#[tokio::test]
async fn b31_kick_and_live_apply_drops_reach_a_full_window() {
    let mut auth = AuthConfig::default();
    auth.users
        .insert("dev".into(), Credential::Plain("pw".into()));
    auth.users
        .insert("ops".into(), Credential::Plain("pw".into()));
    let b = start_mtls(
        vec![
            spec("tcp", ListenerKind::Tcp),
            spec("mtls", ListenerKind::Tls),
        ],
        auth.clone(),
    )
    .await;
    let port = b.port("tcp").unwrap();
    let k = kit();

    // Remove user: a client with a full window (Receive Maximum 1, never acknowledges).
    let mut dev = Raw::tcp(port, true).await;
    assert_eq!(
        connect_v5_receive_max_1(&mut dev, "dev-1", Some(("dev", "pw"))).await,
        0
    );
    subscribe_q1(&mut dev, "w/dev").await;
    // Disconnect from the Broker screen (0x98): same window.
    let mut ops = Raw::tcp(port, true).await;
    assert_eq!(
        connect_v5_receive_max_1(&mut ops, "ops-1", Some(("ops", "pw"))).await,
        0
    );
    subscribe_q1(&mut ops, "w/ops").await;
    // Revoked certificate on the TLS listener.
    let tls = tls_stream(b.port("mtls").unwrap(), kit_tls(Some(&k.client_revoked)))
        .await
        .unwrap();
    let mut rev = Raw::new(Box::new(tls), true);
    assert_eq!(connect_v5_receive_max_1(&mut rev, "rev-1", None).await, 0);
    subscribe_q1(&mut rev, "w/rev").await;

    let mut p = Raw::tcp(port, true).await;
    assert_eq!(p.connect("filler", Some(("ops", "pw")), None, 30).await, 0);
    for t in ["w/dev", "w/ops", "w/rev"] {
        for _ in 0..3 {
            let mut m = v5::Publish::new(
                t,
                rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
                b"x".to_vec(),
                None,
            );
            m.pkid = 7;
            p.send5(v5::Packet::Publish(m)).await;
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut no_dev = auth.clone();
    no_dev.users.remove("dev");
    b.update_auth(no_dev);
    b.revocations().revoke(&k.client_revoked.serial);
    assert_eq!(b.revalidate(), 2, "user removed + certificate revoked");
    assert!(b.disconnect_client("ops-1"));
    for (name, c) in [
        ("removed user", &mut dev),
        ("kicked", &mut ops),
        ("revoked", &mut rev),
    ] {
        assert!(
            closed_within(&mut c.io, Duration::from_secs(1)).await,
            "{name}: still open"
        );
    }
    b.stop().await;
}

#[tokio::test]
async fn b31_unacknowledged_delivery_resent_once_then_connection_closed() {
    let limits = Limits {
        inflight_resend: Duration::from_millis(300),
        inflight_close: Duration::from_millis(900),
        ..Default::default()
    };
    let b = start_with(vec![spec("tcp", ListenerKind::Tcp)], anon(), limits, None).await;
    let port = b.port("tcp").unwrap();
    let mut c = Raw::tcp(port, true).await;
    assert_eq!(connect_v5_receive_max_1(&mut c, "stuck", None).await, 0);
    subscribe_q1(&mut c, "s").await;
    let mut p = Raw::tcp(port, true).await;
    assert_eq!(p.connect("pub", None, None, 30).await, 0);
    let mut m = v5::Publish::new(
        "s",
        rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        b"one".to_vec(),
        None,
    );
    m.pkid = 1;
    p.send5(v5::Packet::Publish(m.clone())).await;
    m.pkid = 2;
    p.send5(v5::Packet::Publish(m)).await;
    // Read raw frames without acknowledging: first delivery, its resend with DUP, then DISCONNECT 0x98.
    let mut buf = BytesMut::new();
    let mut frames: Vec<u8> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        let mut chunk = [0u8; 4096];
        match tokio::time::timeout_at(deadline, c.io.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) => break,
            Ok(Ok(n)) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => panic!("not closed in time; frames {frames:x?}"),
        }
        while buf.len() >= 2 && buf.len() >= 2 + buf[1] as usize {
            let f = buf.split_to(2 + buf[1] as usize);
            frames.push(f[0]);
            if f[0] == 0xE0 {
                assert_eq!(f[2], 0x98);
            }
        }
    }
    let publishes: Vec<u8> = frames.iter().copied().filter(|b| b >> 4 == 3).collect();
    assert_eq!(
        publishes.len(),
        2,
        "one delivery and one resend: {frames:x?}"
    );
    assert_eq!(publishes[1] & 0x08, 0x08, "the resend carries DUP");
    assert_eq!(frames.last(), Some(&0xE0));
    assert!(eventually(|| b.log_buffer().contains("no acknowledgement or no reading")).await);
    b.stop().await;
}

// ------------------------------------------------------------------ B-32 limits before login (SEC-04)

#[tokio::test]
async fn b32_pending_connections_capped_per_address_and_per_listener() {
    let limits = Limits {
        pending_per_address: 8,
        pending_per_listener: 64,
        ..Default::default()
    };
    let b = start_with(vec![spec("tcp", ListenerKind::Tcp)], anon(), limits, None).await;
    let port = b.port("tcp").unwrap();
    let mut idle = Vec::new();
    for _ in 0..8 {
        idle.push(tcp(port).await);
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut ninth = tcp(port).await;
    assert!(
        closed_within(&mut ninth, Duration::from_secs(1)).await,
        "9th pending connection refused"
    );
    let mut tenth = tcp(port).await;
    assert!(closed_within(&mut tenth, Duration::from_secs(1)).await);
    let warnings = b
        .log(mqttpanel_broker::config::LogLevel::Warn, 0)
        .iter()
        .filter(|l| l.text.contains("waiting to log in from this address"))
        .count();
    assert_eq!(warnings, 1, "one warning per minute per address");
    // A slot frees when a pending connection goes away; a logged-in client does not count.
    drop(idle.pop());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut ok = Raw::tcp(port, true).await;
    assert_eq!(ok.connect("after", None, None, 30).await, 0);
    let mut ok2 = Raw::tcp(port, true).await;
    assert_eq!(
        timeout_connect(&mut ok2).await,
        None,
        "still 7 pending + this one"
    );
    drop(idle);
    b.stop().await;

    // Per listener: 64 by default, here 3 with a high per-address cap.
    let limits = Limits {
        pending_per_address: 100,
        pending_per_listener: 3,
        ..Default::default()
    };
    let b = start_with(vec![spec("tcp", ListenerKind::Tcp)], anon(), limits, None).await;
    let port = b.port("tcp").unwrap();
    let _idle: Vec<_> = futures_join(port, 3).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut fourth = tcp(port).await;
    assert!(closed_within(&mut fourth, Duration::from_secs(1)).await);
    assert!(b
        .log_buffer()
        .contains("waiting to log in on the TCP listener"));
    b.stop().await;
}

async fn futures_join(port: u16, n: usize) -> Vec<tokio::net::TcpStream> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(tcp(port).await);
    }
    v
}

/// Connects with MQTT 5; None when the CONNACK came (code 0), Some(()) never: a refusal panics in `connect`.
async fn timeout_connect(r: &mut Raw) -> Option<u8> {
    match r.connect("ok2", None, None, 30).await {
        0 => None,
        c => Some(c),
    }
}

#[tokio::test]
async fn b32_connect_over_64_kib_refused_before_login() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp)], anon()).await;
    let port = b.port("tcp").unwrap();
    // 70 000-byte password: over the pre-login limit, below the 16 MiB maximum packet.
    let big = "p".repeat(70_000);
    let mut r = Raw::tcp(port, false).await;
    let mut c = v4::Connect::new("big");
    c.login = Some(v4::Login::new("u", big.as_str()));
    r.send4(v4::Packet::Connect(c)).await;
    assert_eq!(
        tokio::time::timeout(T, r.read()).await.unwrap(),
        Pkt::Closed
    );
    // 60 000 bytes are fine.
    let ok = "p".repeat(60_000);
    let mut r = Raw::tcp(port, false).await;
    assert_eq!(
        r.connect("fine", Some(("u", ok.as_str())), None, 30).await,
        0
    );
    // After login the configured maximum applies: a 100 KiB PUBLISH goes through.
    let mut s = Raw::tcp(port, false).await;
    assert_eq!(s.connect("s", None, None, 30).await, 0);
    s.subscribe("big", 0).await;
    r.publish("big", &vec![7u8; 100 * 1024], false).await;
    match s.next_publish(T).await {
        Some(Pkt::Publish { payload, .. }) => assert_eq!(payload.len(), 100 * 1024),
        other => panic!("{other:?}"),
    }
    b.stop().await;
}

#[tokio::test]
async fn b32_failed_login_waits_one_second_and_closes() {
    let mut auth = AuthConfig::default();
    auth.users
        .insert("dev".into(), Credential::Plain("pw".into()));
    let b = start(vec![spec("tcp", ListenerKind::Tcp)], auth).await;
    let port = b.port("tcp").unwrap();
    for v5 in [false, true] {
        let mut r = Raw::tcp(port, v5).await;
        let t0 = Instant::now();
        let code = r.connect("bad", Some(("dev", "wrong")), None, 30).await;
        assert_eq!(code, if v5 { 0x86 } else { 0x05 });
        assert!(
            t0.elapsed() >= Duration::from_millis(950),
            "refusal after {:?}",
            t0.elapsed()
        );
        assert_eq!(
            tokio::time::timeout(T, r.read()).await.unwrap(),
            Pkt::Closed
        );
    }
    let mut ok = Raw::tcp(port, true).await;
    let t0 = Instant::now();
    assert_eq!(ok.connect("good", Some(("dev", "pw")), None, 30).await, 0);
    assert!(
        t0.elapsed() < Duration::from_millis(900),
        "a good login is not delayed"
    );
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b32_password_checks_run_at_most_four_at_once() {
    let hash = mqttpanel_broker::runtime::hash_password("pw").unwrap();
    let mut auth = AuthConfig::default();
    auth.users.insert("dev".into(), Credential::Argon2(hash));
    let limits = Limits {
        pending_per_address: 64,
        ..Default::default()
    };
    let b = start_with(vec![spec("tcp", ListenerKind::Tcp)], auth, limits, None).await;
    let port = b.port("tcp").unwrap();
    let mut tasks = Vec::new();
    for i in 0..12 {
        tasks.push(tokio::spawn(async move {
            let mut r = Raw::tcp(port, true).await;
            let pw = if i % 2 == 0 { "pw" } else { "wrong" };
            r.connect(&format!("c{i}"), Some(("dev", pw)), None, 30)
                .await
        }));
    }
    for (i, t) in tasks.into_iter().enumerate() {
        let code = t.await.unwrap();
        assert_eq!(code, if i % 2 == 0 { 0 } else { 0x86 });
    }
    let peak = b.password_checks_peak();
    assert!((1..=4).contains(&peak), "peak {peak}");
    b.stop().await;
}

// ------------------------------------------------------------------ B-33 web origins (SEC-06)

/// WebSocket upgrade with an optional Origin header; Err carries the HTTP status on a refusal.
async fn ws_with_origin(port: u16, origin: Option<&str>) -> Result<Raw, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let s = tcp(port).await;
    let mut req = format!("ws://localhost:{port}/mqtt")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", "mqtt".parse().unwrap());
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", o.parse().unwrap());
    }
    match tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::client_async(req, s),
    )
    .await
    {
        Err(_) => Err("timeout".into()),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(resp))) => {
            Err(resp.status().as_u16().to_string())
        }
        Ok(Err(e)) => Err(e.to_string()),
        Ok(Ok((ws, _))) => Ok(Raw::new(
            Box::new(mqttpanel_broker::runtime::ws::WsBytes::new(ws)),
            true,
        )),
    }
}

/// A prompt that records requests and answers with `answer` after `delay` (None: never answers).
fn prompt(
    answer: Option<bool>,
    delay: Duration,
    seen: Arc<Mutex<Vec<OriginRequest>>>,
) -> OriginPrompt {
    OriginPrompt(Arc::new(move |req| {
        seen.lock().unwrap().push(req);
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            match answer {
                Some(a) => {
                    let _ = tx.send(a);
                }
                None => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    drop(tx);
                }
            }
        });
        rx
    }))
}

#[tokio::test]
async fn b33_web_origin_rule() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut ws = spec("ws", ListenerKind::Ws);
    ws.allowed_origins = vec!["https://Listed.example/".into()];
    let b = start_with(
        vec![ws],
        anon(),
        Limits::default(),
        Some(prompt(Some(true), Duration::from_millis(50), seen.clone())),
    )
    .await;
    let port = b.port("ws").unwrap();

    // No Origin (not a browser): unchanged.
    let mut c = ws_with_origin(port, None).await.unwrap();
    assert_eq!(c.connect("device", None, None, 30).await, 0);
    // Listed origin (compared without case and trailing slash): no prompt.
    let mut c = ws_with_origin(port, Some("https://listed.example"))
        .await
        .unwrap();
    assert_eq!(c.connect("listed", None, None, 30).await, 0);
    assert!(seen.lock().unwrap().is_empty());
    // `null` is never asked about.
    assert_eq!(
        ws_with_origin(port, Some("null")).await.err().as_deref(),
        Some("403")
    );
    assert!(seen.lock().unwrap().is_empty());
    // Unknown origin: the owner is asked; Allow lets it through and adds it to the list.
    let mut c = ws_with_origin(port, Some("https://new.example"))
        .await
        .unwrap();
    assert_eq!(c.connect("new", None, None, 30).await, 0);
    {
        let s = seen.lock().unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].origin, "https://new.example");
        assert_eq!(s[0].listener_id, "ws");
        assert_eq!(s[0].port, port);
    }
    assert!(b
        .allowed_origins("ws")
        .unwrap()
        .contains(&"https://new.example".to_string()));
    // Asked once: the next upgrade passes without a prompt.
    assert!(ws_with_origin(port, Some("https://new.example"))
        .await
        .is_ok());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(b
        .log_buffer()
        .contains("web page at https://new.example allowed"));
    b.stop().await;
}

#[tokio::test]
async fn b33_web_origin_with_login_required_is_accepted() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut auth = AuthConfig::default();
    auth.users
        .insert("dev".into(), Credential::Plain("pw".into()));
    let b = start_with(
        vec![spec("ws", ListenerKind::Ws)],
        auth,
        Limits::default(),
        Some(prompt(Some(false), Duration::ZERO, seen.clone())),
    )
    .await;
    let port = b.port("ws").unwrap();
    let mut c = ws_with_origin(port, Some("https://any.example"))
        .await
        .unwrap();
    assert_eq!(c.connect("page", Some(("dev", "pw")), None, 30).await, 0);
    assert!(
        seen.lock().unwrap().is_empty(),
        "no prompt when a login is required"
    );
    b.stop().await;
}

#[tokio::test]
async fn b33_web_origin_deny_timeout_and_one_prompt_per_origin() {
    // Deny: HTTP 403, logged, not added.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let b = start_with(
        vec![spec("ws", ListenerKind::Ws)],
        anon(),
        Limits::default(),
        Some(prompt(
            Some(false),
            Duration::from_millis(300),
            seen.clone(),
        )),
    )
    .await;
    let port = b.port("ws").unwrap();
    // Two upgrades from the same origin while the prompt is open: one prompt, both refused.
    let (a, c) = tokio::join!(
        ws_with_origin(port, Some("https://evil.example")),
        ws_with_origin(port, Some("https://evil.example"))
    );
    assert_eq!(a.err().as_deref(), Some("403"));
    assert_eq!(c.err().as_deref(), Some("403"));
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(b.allowed_origins("ws").unwrap().is_empty());
    assert!(b
        .log_buffer()
        .contains("web page at https://evil.example refused"));
    // Device clients are not affected.
    let mut d = ws_with_origin(port, None).await.unwrap();
    assert_eq!(d.connect("device", None, None, 30).await, 0);
    b.stop().await;

    // No answer within the time limit: refused.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let limits = Limits {
        origin_prompt_timeout: Duration::from_millis(400),
        ..Default::default()
    };
    let b = start_with(
        vec![spec("ws", ListenerKind::Ws)],
        anon(),
        limits,
        Some(prompt(None, Duration::ZERO, seen.clone())),
    )
    .await;
    let port = b.port("ws").unwrap();
    let t0 = Instant::now();
    assert_eq!(
        ws_with_origin(port, Some("https://slow.example"))
            .await
            .err()
            .as_deref(),
        Some("403")
    );
    assert!(t0.elapsed() >= Duration::from_millis(350));
    b.stop().await;

    // No prompt configured (command-line broker): unknown origins are refused.
    let b = start_with(
        vec![spec("ws", ListenerKind::Ws)],
        anon(),
        Limits::default(),
        None,
    )
    .await;
    let port = b.port("ws").unwrap();
    assert_eq!(
        ws_with_origin(port, Some("https://x.example"))
            .await
            .err()
            .as_deref(),
        Some("403")
    );
    // set_allowed_origins applies to the next upgrade.
    assert!(b.set_allowed_origins("ws", &["https://x.example".into()]));
    assert!(ws_with_origin(port, Some("https://x.example"))
        .await
        .is_ok());
    b.stop().await;
}

#[tokio::test]
async fn b33_manager_keeps_allowed_origins_per_listener() {
    use mqttpanel_broker::BrokerManager;
    use mqttpanel_profiles::MemorySecrets;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecrets::new());
    let m = BrokerManager::open(dir.path(), secrets.clone()).unwrap();
    let origins = |m: &BrokerManager| {
        m.config()
            .listeners
            .into_iter()
            .find(|l| l.id == "ws")
            .unwrap()
            .allowed_origins
    };
    assert!(origins(&m).is_empty());
    m.set_allowed_origins(
        "ws",
        &["https://Dash.example/".into(), "http://localhost:3000".into(), "https://dash.example".into()],
    )
    .unwrap();
    assert_eq!(origins(&m), vec!["https://dash.example", "http://localhost:3000"]);
    for bad in ["null", "dash.example", "https://dash.example/path", "https://*.example", "https://u@h"] {
        let e = m.set_allowed_origins("ws", &[bad.into()]).unwrap_err();
        assert_eq!(e.key(), "broker.originInvalid", "{bad}");
    }
    assert!(m.set_allowed_origins("tcp", &["https://a.example".into()]).is_err());
    m.allow_origin("ws", "https://b.example").unwrap();
    assert_eq!(origins(&m).len(), 3);
    assert!(m.log_buffer().contains("web origin https://b.example allowed on listener WS 8083"));
    drop(m);
    let m = BrokerManager::open(dir.path(), secrets).unwrap();
    assert_eq!(origins(&m), vec!["https://dash.example", "http://localhost:3000", "https://b.example"]);
}

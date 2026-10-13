//! Runtime tests on the F-02 harness: protocols, transports, QoS, client certificates (T-11..T-14 broker side),
//! bind addresses (B-04), shared users on every listener (B-06), retained/disconnect/will (B-11), TOML config (B-13), robustness.

mod common;

use common::*;
use mqttpanel_broker::config::{ListenerKind, LogLevel};
use mqttpanel_broker::runtime::{
    normalize_serial, AuthConfig, BindAddr, Broker, Credential, RuntimeConfig,
};
use rumqttc::v5::mqttbytes::v5;
use std::time::Duration;

fn anon() -> AuthConfig {
    AuthConfig {
        anonymous: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn qos_round_trips_mqtt_311() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut sub = c4(opts4("sub4", port, None, None)).await.unwrap();
    sub.subscribe("q/#", 2).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let publ = c4(opts4("pub4", port, None, None)).await.unwrap();
    for (i, q) in [0u8, 1, 2].into_iter().enumerate() {
        publ.publish(&format!("q/{i}"), q, false, format!("m{i}"), None)
            .await;
        let p = next(&mut sub.rx).await.expect("delivered");
        assert_eq!(p.topic, format!("q/{i}"));
        assert_eq!(&p.payload[..], format!("m{i}").as_bytes());
        assert_eq!(p.qos, q);
    }
    assert!(eventually(|| b.stats().msg_in_total == 3 && b.stats().msg_out_total == 3).await);
    let clients = b.clients();
    assert_eq!(clients.len(), 2);
    let s = clients.iter().find(|c| c.client_id == "sub4").unwrap();
    assert_eq!(
        (
            s.protocol.as_str(),
            s.transport.as_str(),
            s.subscriptions,
            s.msg_out
        ),
        ("3.1.1", "TCP", 1, 3)
    );
    assert_eq!(s.address, "localhost");
    b.stop().await;
}

#[tokio::test]
async fn qos_round_trips_mqtt_5_with_properties() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut sub = c5(opts5("sub5", port, None, None)).await.unwrap();
    assert!(sub.max_packet.is_some(), "CONNACK maximum packet size");
    sub.subscribe("p/#", 2).await;
    let mut sub4 = c4(opts4("sub4b", port, None, None)).await.unwrap();
    sub4.subscribe("p/#", 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let publ = c5(opts5("pub5", port, None, None)).await.unwrap();
    let props = v5::PublishProperties {
        payload_format_indicator: Some(1),
        message_expiry_interval: Some(60),
        topic_alias: None,
        response_topic: Some("p/reply".into()),
        correlation_data: Some(bytes::Bytes::from_static(b"corr-1")),
        user_properties: vec![("k".into(), "v".into())],
        subscription_identifiers: vec![],
        content_type: Some("application/json".into()),
    };
    for q in [1u8, 2] {
        publ.publish("p/x", q, false, "{\"a\":1}", Some(props.clone()))
            .await;
        let p = next(&mut sub.rx).await.expect("v5 delivery");
        assert_eq!(p.qos, q);
        let pp = p.props.expect("properties passed through");
        assert_eq!(pp.content_type.as_deref(), Some("application/json"));
        assert_eq!(pp.response_topic.as_deref(), Some("p/reply"));
        assert_eq!(pp.correlation_data.as_deref(), Some(&b"corr-1"[..]));
        assert_eq!(pp.user_properties, vec![("k".to_string(), "v".to_string())]);
        assert_eq!(pp.payload_format_indicator, Some(1));
        assert!(pp
            .message_expiry_interval
            .is_some_and(|e| (58..=60).contains(&e)));
        let p4 = next(&mut sub4.rx).await.expect("3.1.1 delivery");
        assert_eq!(&p4.payload[..], b"{\"a\":1}");
    }
    b.stop().await;
}

#[tokio::test]
async fn v5_empty_client_id_gets_assigned_id() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let c = c5(opts5("", b.port("tcp").unwrap(), None, None))
        .await
        .unwrap();
    let id = c.assigned_id.clone().expect("assigned id");
    assert!(id.starts_with("auto-"));
    b.stop().await;
}

#[tokio::test]
async fn session_takeover_disconnects_old_connection_with_0x8e() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut first = Raw::tcp(port, true).await;
    assert_eq!(first.connect("same", None, None, 30).await, 0);
    let mut second = Raw::tcp(port, true).await;
    assert_eq!(second.connect("same", None, None, 30).await, 0);
    let p = tokio::time::timeout(T, first.read()).await.unwrap();
    assert_eq!(p, Pkt::Disconnect(0x8E));
    assert_eq!(
        tokio::time::timeout(T, first.read()).await.unwrap(),
        Pkt::Closed
    );
    assert!(eventually(|| b.clients().len() == 1).await);
    b.stop().await;
}

#[tokio::test]
async fn keep_alive_is_enforced() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let mut c = Raw::tcp(b.port("tcp").unwrap(), true).await;
    assert_eq!(c.connect("sleepy", None, None, 1).await, 0);
    let started = std::time::Instant::now();
    let p = tokio::time::timeout(Duration::from_secs(4), c.read())
        .await
        .expect("closed by the broker");
    assert_eq!(p, Pkt::Disconnect(0x8D));
    assert!(started.elapsed() >= Duration::from_millis(1400));
    b.stop().await;
}

#[tokio::test]
async fn persistent_session_receives_messages_sent_while_offline() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut o = opts4("durable", port, None, None);
    o.clean = false;
    let sub = c4(o.clone()).await.unwrap();
    sub.subscribe("d/#", 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    sub.disconnect().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(sub);
    let publ = c4(opts4("p", port, None, None)).await.unwrap();
    publ.publish("d/1", 1, false, "while away", None).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut again = c4(o).await.unwrap();
    let p = next(&mut again.rx).await.expect("queued message");
    assert_eq!(&p.payload[..], b"while away");
    b.stop().await;
}

#[tokio::test]
async fn shared_subscription_delivers_to_one_member() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut a = c5(opts5("ga", port, None, None)).await.unwrap();
    let mut c = c5(opts5("gb", port, None, None)).await.unwrap();
    a.subscribe("$share/g/s/#", 0).await;
    c.subscribe("$share/g/s/#", 0).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let p = c5(opts5("gp", port, None, None)).await.unwrap();
    for i in 0..10 {
        p.publish(&format!("s/{i}"), 0, false, "x", None).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (mut na, mut nc) = (0, 0);
    while a.rx.try_recv().is_ok() {
        na += 1;
    }
    while c.rx.try_recv().is_ok() {
        nc += 1;
    }
    assert_eq!(na + nc, 10);
    assert!(na > 0 && nc > 0, "round robin: {na} / {nc}");
    b.stop().await;
}

#[tokio::test]
async fn websocket_and_secure_websocket_on_path_mqtt() {
    let b = start_all_anonymous().await;
    let (ws, wss) = (b.port("ws").unwrap(), b.port("wss").unwrap());
    let s = ws_upgrade(tcp(ws).await, "ws", ws, "/mqtt").await.unwrap();
    let mut sub = Raw::new(Box::new(s), false);
    assert_eq!(sub.connect("ws-sub", None, None, 30).await, 0);
    assert_eq!(sub.subscribe("w/#", 1).await, vec![1]);
    let t = tls_stream(wss, kit_tls(None)).await.unwrap();
    let s = ws_upgrade(t, "wss", wss, "/mqtt").await.unwrap();
    let mut publ = Raw::new(Box::new(s), true);
    assert_eq!(publ.connect("wss-pub", None, None, 30).await, 0);
    publ.publish("w/1", b"over wss", false).await;
    match sub.next_publish(T).await {
        Some(Pkt::Publish { topic, payload, .. }) => {
            assert_eq!(topic, "w/1");
            assert_eq!(payload, b"over wss");
        }
        other => panic!("no publish: {other:?}"),
    }
    let transports: Vec<String> = b.clients().into_iter().map(|c| c.transport).collect();
    assert!(transports.contains(&"WS".to_string()) && transports.contains(&"WSS".to_string()));
    assert!(
        ws_upgrade(tcp(ws).await, "ws", ws, "/other").await.is_err(),
        "wrong path refused"
    );
    b.stop().await;
}

#[tokio::test]
async fn t11_client_cert_required_but_none_given() {
    let b = start(
        vec![spec("mtls", ListenerKind::Tls, true)],
        AuthConfig::default(),
    )
    .await;
    let port = b.port("mtls").unwrap();
    assert!(c4(opts4("nocert", port, Some(kit_tls(None)), None))
        .await
        .is_err());
    assert!(
        eventually(|| b
            .log_buffer()
            .contains("TLS handshake failed from 127.0.0.1"))
        .await
    );
    assert!(b.log_buffer().contains("no client certificate"));
    b.stop().await;
}

#[tokio::test]
async fn t12_client_ok_accepted_with_cn_in_log_and_last_seen() {
    let b = start(
        vec![spec("mtls", ListenerKind::Tls, true)],
        AuthConfig::default(),
    )
    .await;
    let port = b.port("mtls").unwrap();
    let k = kit();
    let c = c4(opts4(
        "dev-1",
        port,
        Some(kit_tls(Some(&k.client_ok))),
        None,
    ))
    .await
    .expect("client-ok accepted");
    assert!(b.log_buffer().contains("certificate client-ok"));
    let cl = b.clients();
    assert_eq!(cl[0].cert_cn.as_deref(), Some("client-ok"));
    assert_eq!(cl[0].transport, "TLS");
    let serial = normalize_serial(&k.client_ok.serial);
    assert_eq!(b.cert_seen().get(&serial).map(|s| s.connected), Some(1));
    c.disconnect().await;
    drop(c);
    assert!(
        eventually(|| b
            .cert_seen()
            .get(&serial)
            .is_some_and(|s| s.connected == 0 && s.last_seen.is_some()))
        .await
    );
    // RSA client certificates work too.
    assert!(c4(opts4(
        "dev-rsa",
        port,
        Some(kit_tls(Some(&k.client_rsa))),
        None
    ))
    .await
    .is_ok());
    b.stop().await;
}

#[tokio::test]
async fn t13_client_cert_from_other_ca_refused() {
    let b = start(
        vec![spec("mtls", ListenerKind::Tls, true)],
        AuthConfig::default(),
    )
    .await;
    let port = b.port("mtls").unwrap();
    assert!(c4(opts4(
        "other",
        port,
        Some(kit_tls(Some(&kit().client_other_ca))),
        None
    ))
    .await
    .is_err());
    assert!(eventually(|| b.log_buffer().contains("unknown CA")).await);
    b.stop().await;
}

#[tokio::test]
async fn t14_revoked_client_cert_refused_at_next_handshake() {
    let b = start(
        vec![spec("mtls", ListenerKind::Tls, true)],
        AuthConfig::default(),
    )
    .await;
    let port = b.port("mtls").unwrap();
    let k = kit();
    let first = c4(opts4(
        "rev",
        port,
        Some(kit_tls(Some(&k.client_revoked))),
        None,
    ))
    .await;
    assert!(first.is_ok(), "accepted before revocation");
    b.revocations().revoke(&k.client_revoked.serial);
    assert!(c4(opts4(
        "rev2",
        port,
        Some(kit_tls(Some(&k.client_revoked))),
        None
    ))
    .await
    .is_err());
    assert!(
        eventually(|| b
            .log_buffer()
            .contains("revoked certificate client-revoked"))
        .await
    );
    b.stop().await;
}

#[tokio::test]
async fn b04_localhost_bind_refuses_lan_address() {
    let mut local = spec("local", ListenerKind::Tcp, false);
    local.bind = BindAddr::Localhost;
    let mut net = spec("net", ListenerKind::Tcp, false);
    net.bind = BindAddr::Network;
    let b = start(vec![local, net], anon()).await;
    let (lp, np) = (b.port("local").unwrap(), b.port("net").unwrap());
    assert!(c4(opts4("l1", lp, None, None)).await.is_ok());
    assert!(c4(opts4("n1", np, None, None)).await.is_ok());
    match lan_ipv4() {
        Some(ip) => {
            let to_local = tokio::time::timeout(T, tokio::net::TcpStream::connect((ip, lp))).await;
            assert!(
                !matches!(to_local, Ok(Ok(_))),
                "localhost listener reachable on {ip}"
            );
            let mut o = opts4("n2", np, None, None);
            o.host = ip.to_string();
            assert!(c4(o).await.is_ok(), "network listener reachable on {ip}");
        }
        None => eprintln!("SKIP part of B-04: no non-loopback IPv4 address on this machine"),
    }
    b.stop().await;
}

#[tokio::test]
async fn b06_shared_users_on_every_listener_no_listener_login() {
    let mut auth = AuthConfig::default();
    auth.users.insert(
        "shared".into(),
        Credential::Argon2(mqttpanel_broker::runtime::hash_password("pw-shared").unwrap()),
    );
    let b = start(
        vec![
            spec("tcp", ListenerKind::Tcp, false),
            spec("tls", ListenerKind::Tls, false),
            spec("ws", ListenerKind::Ws, false),
            spec("wss", ListenerKind::Wss, false),
        ],
        auth,
    )
    .await;
    let (tcp_p, tls_p) = (b.port("tcp").unwrap(), b.port("tls").unwrap());
    let (ws_p, wss_p) = (b.port("ws").unwrap(), b.port("wss").unwrap());
    // TCP and TLS
    assert!(c4(opts4("x1", tcp_p, None, Some(("shared", "pw-shared"))))
        .await
        .is_ok());
    assert!(c4(opts4("x2", tls_p, Some(kit_tls(None)), Some(("shared", "pw-shared"))))
        .await
        .is_ok());
    let mut raw = Raw::tcp(tcp_p, false).await;
    assert_eq!(raw.connect("x3", Some(("shared", "wrong")), None, 30).await, 0x05);
    // WS and WSS
    let s = ws_upgrade(tcp(ws_p).await, "ws", ws_p, "/mqtt").await.unwrap();
    let mut r = Raw::new(Box::new(s), false);
    assert_eq!(r.connect("x4", Some(("shared", "pw-shared")), None, 30).await, 0);
    let t = tls_stream(wss_p, kit_tls(None)).await.unwrap();
    let s = ws_upgrade(t, "wss", wss_p, "/mqtt").await.unwrap();
    let mut r = Raw::new(Box::new(s), true);
    assert_eq!(r.connect("x5", Some(("shared", "pw-shared")), None, 30).await, 0);
    let s = ws_upgrade(tcp(ws_p).await, "ws", ws_p, "/mqtt").await.unwrap();
    let mut r = Raw::new(Box::new(s), false);
    assert_eq!(r.connect("x6", None, None, 30).await, 0x05, "no login, no access");
    b.stop().await;
    // A mutual-TLS listener: a user login never replaces the certificate.
    let mut auth2 = AuthConfig::default();
    auth2.users.insert("dev".into(), Credential::Plain("pw-dev".into()));
    let b2 = start(vec![spec("m", ListenerKind::Tls, true)], auth2).await;
    let pm = b2.port("m").unwrap();
    assert!(
        c4(opts4("m1", pm, Some(kit_tls(None)), Some(("dev", "pw-dev"))))
            .await
            .is_err(),
        "login never replaces the certificate"
    );
    assert!(
        c4(opts4("m2", pm, Some(kit_tls(Some(&kit().client_ok))), None))
            .await
            .is_ok()
    );
    b2.stop().await;
}

#[tokio::test]
async fn b11_retained_count_disconnect_and_last_will() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut p = Raw::tcp(port, false).await;
    assert_eq!(p.connect("pub", None, None, 30).await, 0);
    p.publish("r/a", b"1", true).await;
    p.publish("r/b", b"2", true).await;
    assert!(eventually(|| b.stats().retained == 2).await);
    p.publish("r/b", b"", true).await;
    assert!(eventually(|| b.retained_count() == 1).await);
    let mut s = Raw::tcp(port, true).await;
    assert_eq!(s.connect("sub", None, None, 30).await, 0);
    s.subscribe("r/#", 1).await;
    match s.next_publish(T).await {
        Some(Pkt::Publish { topic, retain, .. }) => {
            assert_eq!((topic.as_str(), retain), ("r/a", true))
        }
        other => panic!("retained not delivered: {other:?}"),
    }
    // Last will on an abrupt disconnect.
    s.subscribe("will/#", 1).await;
    let mut w = Raw::tcp(port, false).await;
    assert_eq!(
        w.connect("dying", None, Some(("will/dying", "gone")), 30)
            .await,
        0
    );
    drop(w);
    match s.next_publish(T).await {
        Some(Pkt::Publish { topic, payload, .. }) => {
            assert_eq!((topic.as_str(), &payload[..]), ("will/dying", &b"gone"[..]))
        }
        other => panic!("will not delivered: {other:?}"),
    }
    assert!(b.log_buffer().contains("last will of dying"));
    // Disconnect a client from the clients table.
    assert!(eventually(|| b.clients().iter().any(|c| c.client_id == "pub")).await);
    assert!(b.disconnect_client("pub"));
    assert_eq!(
        tokio::time::timeout(T, p.read()).await.unwrap(),
        Pkt::Closed
    );
    assert!(eventually(|| !b.clients().iter().any(|c| c.client_id == "pub")).await);
    // A clean DISCONNECT does not publish the will.
    let mut w2 = Raw::tcp(port, false).await;
    assert_eq!(
        w2.connect("polite", None, Some(("will/polite", "x")), 30)
            .await,
        0
    );
    w2.send_raw(&[0xE0, 0x00]).await.unwrap();
    assert!(s.next_publish(Duration::from_millis(500)).await.is_none());
    b.stop().await;
}

#[tokio::test]
async fn b05_runtime_connack_codes_and_log_line() {
    let mut auth = AuthConfig::default();
    auth.users.insert(
        "alice".into(),
        Credential::Argon2(mqttpanel_broker::runtime::hash_password("right").unwrap()),
    );
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], auth).await;
    let port = b.port("tcp").unwrap();
    let mut r4 = Raw::tcp(port, false).await;
    assert_eq!(
        r4.connect("c4", Some(("alice", "wrong")), None, 30).await,
        0x05
    );
    let mut r5 = Raw::tcp(port, true).await;
    assert_eq!(
        r5.connect("c5", Some(("alice", "wrong")), None, 30).await,
        0x86
    );
    let mut anon5 = Raw::tcp(port, true).await;
    assert_eq!(anon5.connect("c6", None, None, 30).await, 0x86);
    let mut ok = Raw::tcp(port, true).await;
    assert_eq!(
        ok.connect("c7", Some(("alice", "right")), None, 30).await,
        0
    );
    let lines = b.log(LogLevel::Warn, 0);
    assert!(lines
        .iter()
        .any(|l| l.text == "client c4 rejected: bad username or password"));
    assert!(lines
        .iter()
        .any(|l| l.text == "client c5 rejected: bad username or password"));
    b.stop().await;
}

#[tokio::test]
async fn garbage_input_never_takes_the_broker_down() {
    let b = start(
        vec![
            spec("tcp", ListenerKind::Tcp, false),
            spec("ws", ListenerKind::Ws, false),
        ],
        anon(),
    )
    .await;
    let port = b.port("tcp").unwrap();
    let mut seed: u64 = 0xdead_beef_1234_5678;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 0..200 {
        let mut r = Raw::tcp(port, i % 2 == 0).await;
        if i % 3 == 0 {
            let _ = r.connect(&format!("g{i}"), None, None, 30).await;
        }
        let len = (rnd() % 300) as usize;
        let mut junk: Vec<u8> = (0..len).map(|_| rnd() as u8).collect();
        if i % 4 == 0 && !junk.is_empty() {
            junk[0] = [0x30, 0x82, 0x62, 0xE0, 0x10, 0xA2][(rnd() % 6) as usize];
        }
        let _ = r.send_raw(&junk).await;
    }
    // Oversized remaining length and an MQTT 3.1 CONNECT.
    let mut r = Raw::tcp(port, false).await;
    let _ = r.send_raw(&[0x10, 0xff, 0xff, 0xff, 0x7f]).await;
    let mut r31 = Raw::tcp(port, false).await;
    let _ = r31
        .send_raw(&[
            0x10, 0x0c, 0x00, 0x06, b'M', b'Q', b'I', b's', b'd', b'p', 0x03, 0x02, 0x00, 0x3c,
        ])
        .await;
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", b.port("ws").unwrap()))
        .await
        .unwrap();
    let _ =
        tokio::io::AsyncWriteExt::write_all(&mut s, b"GET / HTTP/1.1\r\n\r\n\x00\x01garbage").await;
    // Still serving.
    let mut ok = Raw::tcp(port, true).await;
    assert_eq!(ok.connect("still-alive", None, None, 30).await, 0);
    b.stop().await;
}

#[tokio::test]
async fn slow_subscriber_drops_qos0_instead_of_growing() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut slow = Raw::tcp(port, false).await;
    assert_eq!(slow.connect("slow", None, None, 300).await, 0);
    slow.subscribe("flood/#", 0).await;
    let mut p = Raw::tcp(port, false).await;
    assert_eq!(p.connect("flooder", None, None, 300).await, 0);
    let payload = vec![7u8; 16 * 1024];
    for _ in 0..20_000 {
        p.publish("flood/x", &payload, false).await;
        if b.stats().dropped > 0 {
            break;
        }
    }
    assert!(
        eventually(|| b.stats().dropped > 0).await,
        "QoS 0 dropped for the slow client"
    );
    assert!(b.log_buffer().contains("client slow is too slow"));
    b.stop().await;
}

#[tokio::test]
async fn stop_closes_listeners_and_clients() {
    let b = start(vec![spec("tcp", ListenerKind::Tcp, false)], anon()).await;
    let port = b.port("tcp").unwrap();
    let mut c = Raw::tcp(port, true).await;
    assert_eq!(c.connect("bye", None, None, 30).await, 0);
    b.stop().await;
    assert_eq!(
        tokio::time::timeout(T, c.read()).await.unwrap(),
        Pkt::Disconnect(0x8B)
    );
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_err());
}

#[tokio::test]
async fn b13_toml_config_file_and_bind_override() {
    let dir = tempfile::tempdir().unwrap();
    kit().write_to(dir.path()).unwrap();
    let (p1, p2) = (free_port(), free_port());
    let text = format!(
        r#"
max_clients = 50
[auth]
anonymous = false
[[auth.users]]
username = "alice"
password = "pw"
[tls]
cert = "server.pem"
key = "server.key"
client_ca = "root-ca.pem"
revoked = ["{revoked}"]
[[listener]]
kind = "tcp"
port = {p1}
bind = "network"
[[listener]]
kind = "tls"
port = {p2}
require_client_cert = true
"#,
        revoked = kit().client_revoked.serial
    );
    let path = dir.path().join("broker.toml");
    std::fs::write(&path, text).unwrap();
    let cfg = RuntimeConfig::from_toml_file(&path)
        .unwrap()
        .with_bind("127.0.0.1".parse().unwrap());
    assert!(cfg
        .listeners
        .iter()
        .all(|l| l.bind == BindAddr::Addr("127.0.0.1".parse().unwrap())));
    assert_eq!(cfg.max_clients, 50);
    let b = Broker::start(cfg).await.unwrap();
    assert_eq!(b.port("tcp"), Some(p1));
    assert!(c4(opts4("t1", p1, None, Some(("alice", "pw"))))
        .await
        .is_ok());
    assert!(c4(opts4("t2", p1, None, Some(("alice", "nope"))))
        .await
        .is_err());
    assert!(
        c4(opts4("t3", p2, Some(kit_tls(Some(&kit().client_ok))), None))
            .await
            .is_ok()
    );
    assert!(c4(opts4(
        "t4",
        p2,
        Some(kit_tls(Some(&kit().client_revoked))),
        None
    ))
    .await
    .is_err());
    b.stop().await;
    // Unknown keys are refused with a catalog message, not a parser dump.
    std::fs::write(&path, "[[listener]]\nkind = \"tcp\"\nport = 1\nbogus = 1\n").unwrap();
    let e = RuntimeConfig::from_toml_file(&path).unwrap_err();
    assert_eq!(e.key(), "file.unreadable");
    // B-06: there is no per-listener login any more; the old key is refused like any unknown key.
    std::fs::write(
        &path,
        "[[listener]]\nkind = \"tcp\"\nport = 1883\nlogin = { username = \"dev\", password = \"pw\" }\n",
    )
    .unwrap();
    assert_eq!(RuntimeConfig::from_toml_file(&path).unwrap_err().key(), "file.unreadable");
}

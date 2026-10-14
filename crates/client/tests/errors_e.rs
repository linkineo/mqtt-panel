//! TESTPLAN section 7: every failure gives the exact user-facing sentence, never a raw library error.

mod common;
use common::*;
use mqttpanel_client::*;
use std::time::Duration;

fn failure_key(r: Result<TestReport, ConnectFailure>) -> UserMessage {
    match r {
        Ok(_) => panic!("expected a failure"),
        Err(ConnectFailure::Message(m)) => m,
        Err(ConnectFailure::Trust(t)) => panic!("unexpected trust prompt {:?}", t.reason),
    }
}

#[tokio::test]
async fn e01_dns_failure() {
    let o = plain_opts(1883);
    let o = ConnectOptions { host: "no-such-host.invalid".into(), ..o };
    let m = failure_key(test_connection(&o).await);
    assert_eq!(m.key, "error.dns");
    assert_eq!(m.text, "Cannot find the host no-such-host.invalid. Check the name or your network.");
}

#[tokio::test]
async fn e02_connection_refused() {
    let port = closed_port().await;
    let m = failure_key(test_connection(&plain_opts(port)).await);
    assert_eq!(m.text, format!("Nothing is listening on localhost:{port}. Is the broker running on that port?"));
}

#[tokio::test]
async fn e03_timeout() {
    let mut o = plain_opts(1883);
    o.host = "10.255.255.1".into();
    o.connect_timeout = Duration::from_secs(2);
    let m = failure_key(test_connection(&o).await);
    // Some networks reject the address immediately instead of dropping packets.
    if m.key == "error.timeout" {
        assert_eq!(m.text, "No answer from 10.255.255.1:1883 after 2 s. A firewall may be blocking the port.");
    } else {
        assert!(m.key == "error.network" || m.key == "error.refused", "{m:?}");
    }
}

#[tokio::test]
async fn e04_tls_on_plain_port() {
    let b = start(FakeConfig::default()).await;
    let m = failure_key(test_connection(&tls_opts(b.port)).await);
    assert_eq!(m.key, "error.tlsOnPlain");
    assert_eq!(m.text, format!("localhost:{} did not start TLS. Try mqtt:// instead of mqtts://, or the broker's TLS port.", b.port));
}

#[tokio::test]
async fn e05_plain_on_tls_port() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(ServerTls::new(vec![k.server.cert_der.clone()], k.server.key_der.clone())), ..Default::default() }).await;
    let m = failure_key(test_connection(&plain_opts(b.port)).await);
    assert_eq!(m.text, format!("localhost:{} expects TLS. Try mqtts://.", b.port));
}

#[tokio::test]
async fn e06_bad_login_v311_and_v5() {
    let beh = Behavior { login: Some(("commissioning".into(), "right".into())), ..Default::default() };
    let b = start(FakeConfig { behavior: beh, ..Default::default() }).await;
    for (v, code) in [(VersionPref::V311, "0x04"), (VersionPref::V5, "0x86")] {
        let mut o = plain_opts(b.port);
        o.version = v;
        o.username = Some("commissioning".into());
        o.password = Some("wrong".into());
        let m = failure_key(test_connection(&o).await);
        assert_eq!(m.key, "error.badLogin");
        assert_eq!(m.text, format!("The broker refused the login for user commissioning (bad username or password). Reason code {code}."));
    }
    let mut o = plain_opts(b.port);
    o.username = Some("commissioning".into());
    o.password = Some("right".into());
    assert!(test_connection(&o).await.is_ok());
}

#[tokio::test]
async fn e07_not_authorized() {
    let b = start(FakeConfig { behavior: Behavior { connack_v5: 0x87, ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    let m = failure_key(test_connection(&o).await);
    assert!(m.text.starts_with("The broker does not allow this client to connect (not authorized)."), "{}", m.text);
}

#[tokio::test]
async fn e08_client_id_rejected() {
    let b = start(FakeConfig { behavior: Behavior { connack_v4: 2, connack_v5: 0x85, ..Default::default() }, ..Default::default() }).await;
    for v in [VersionPref::V311, VersionPref::V5] {
        let mut o = plain_opts(b.port);
        o.version = v;
        o.client_id = "dup-id".into();
        let m = failure_key(test_connection(&o).await);
        assert_eq!(m.key, "error.clientIdRejected");
        assert!(m.text.contains("dup-id") && m.text.contains("Generate"), "{}", m.text);
    }
}

#[tokio::test]
async fn e09_server_unavailable() {
    let b = start(FakeConfig { behavior: Behavior { connack_v4: 3, connack_v5: 0x88, ..Default::default() }, ..Default::default() }).await;
    for v in [VersionPref::V311, VersionPref::V5] {
        let mut o = plain_opts(b.port);
        o.version = v;
        let m = failure_key(test_connection(&o).await);
        assert!(m.text.starts_with("The broker is up but not accepting connections right now."), "{}", m.text);
    }
}

#[tokio::test]
async fn e10_protocol_version_refused() {
    let b = start(FakeConfig { behavior: Behavior { v311_only: true, ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    let m = failure_key(test_connection(&o).await);
    assert_eq!(m.text, "This broker does not speak MQTT 5. Set the version to 3.1.1 or auto.");
    // C-03: auto falls back to 3.1.1
    o.version = VersionPref::Auto;
    let r = test_connection(&o).await.unwrap();
    assert_eq!(r.info.version, Version::V311);
}

#[tokio::test]
async fn e11_keep_alive_timeout() {
    let b = start(FakeConfig { behavior: Behavior { ignore_ping: true, ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.keep_alive = 1;
    let (_h, _info, mut rx) = connect(o).await.unwrap();
    let lost = tokio::time::timeout(Duration::from_secs(8), async {
        while let Some(e) = rx.recv().await {
            if let ClientEvent::Lost(m) = e {
                return m;
            }
        }
        panic!("closed")
    })
    .await
    .unwrap();
    assert_eq!(lost.text, "The broker stopped answering; reconnecting…");
}

#[tokio::test]
async fn e12_disconnect_reason_session_taken_over() {
    let b = start(FakeConfig { behavior: Behavior { disconnect_after: Some((200, 0x8E)), ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    let (_h, _info, mut rx) = connect(o).await.unwrap();
    let lost = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(e) = rx.recv().await {
            if let ClientEvent::Lost(m) = e {
                return m;
            }
        }
        panic!("closed")
    })
    .await
    .unwrap();
    assert_eq!(lost.text, "Disconnected by the broker: session taken over by another client with the same ID.");
    assert_eq!(lost.render(Lang::Fr), "Déconnecté par le broker\u{202f}: session reprise par un autre client avec le même ID.");
}

#[tokio::test]
async fn e13_subscribe_refused() {
    let b = start(FakeConfig::default()).await;
    for v in [VersionPref::V5, VersionPref::V311] {
        let mut o = plain_opts(b.port);
        o.version = v;
        o.subscriptions = vec![("building/#".into(), 0), ("forbidden/#".into(), 0)];
        let (_h, _i, mut rx) = connect(o).await.unwrap();
        let mut results = vec![];
        while results.len() < 2 {
            if let ClientEvent::Subscribed { filter, result } = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap() { results.push((filter, result)) }
        }
        assert!(results[0].1.is_ok());
        let err = results[1].1.clone().unwrap_err();
        if v == VersionPref::V5 {
            assert_eq!(err.text, "Subscription to forbidden/# was refused (not authorized).");
        } else {
            assert!(err.text.starts_with("Subscription to forbidden/# was refused"));
        }
    }
}

#[tokio::test]
async fn e14_p09_packet_too_large_refused_before_sending() {
    let b = start(FakeConfig { behavior: Behavior { max_packet_size: Some(1024 * 1024), ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    let (h, info, mut rx) = connect(o).await.unwrap();
    assert_eq!(info.max_packet_size, Some(1024 * 1024));
    let token = h.publish(Outgoing { topic: "big".into(), payload: vec![0u8; 1024 * 1024 + 10], qos: 1, ..Default::default() });
    loop {
        if let ClientEvent::Published { token: t, result } = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap() {
            assert_eq!(t, token);
            assert_eq!(result.unwrap_err().text, "This broker accepts at most 1 MB per message.");
            break;
        }
    }
}

#[tokio::test]
async fn e15_websocket_wrong_path() {
    let b = start(FakeConfig { ws_path: Some("/mqtt".into()), ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.scheme = Scheme::Ws;
    o.ws_path = "/x".into();
    let m = failure_key(test_connection(&o).await);
    assert_eq!(m.text, "The broker closed the WebSocket: path /x not accepted (HTTP 404).");
    // C-04: right path works
    o.ws_path = "/mqtt".into();
    test_connection(&o).await.unwrap();
}

#[test]
fn e17_every_catalog_key_exists_in_fr_and_de() {
    let en = catalog::keys(Lang::En);
    let fr = catalog::keys(Lang::Fr);
    let de = catalog::keys(Lang::De);
    for k in &en {
        assert!(fr.contains(k), "missing fr {k}");
        assert!(de.contains(k), "missing de {k}");
    }
}

#[test]
fn e17_every_key_used_in_client_source_exists() {
    let en = catalog::keys(Lang::En);
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let re_like = |s: &str| -> Vec<String> {
        let mut out = vec![];
        for part in s.split("msg!(\"").skip(1) {
            if let Some(end) = part.find('"') {
                out.push(part[..end].to_string());
            }
        }
        for part in s.split("\"@").skip(1) {
            if let Some(end) = part.find('"') {
                out.push(part[..end].to_string());
            }
        }
        out
    };
    for entry in std::fs::read_dir(src_dir).unwrap() {
        let s = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for key in re_like(&s).into_iter().filter(|k| k.contains('.') && !k.contains(' ')) {
            assert!(en.contains(&key), "catalog key {key} missing");
        }
    }
}

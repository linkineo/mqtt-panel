//! Session behaviour: versions, QoS flows, retained, reconnect.

mod common;
use common::*;
use mqttpanel_client::*;
use std::time::Duration;

async fn next_published(rx: &mut tokio::sync::mpsc::Receiver<ClientEvent>) -> (u64, Result<PubStatus, UserMessage>) {
    loop {
        if let ClientEvent::Published { token, result } = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap() {
            return (token, result);
        }
    }
}

async fn next_message(rx: &mut tokio::sync::mpsc::Receiver<ClientEvent>) -> Incoming {
    loop {
        if let ClientEvent::Message(m) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap() {
            return m;
        }
    }
}

#[tokio::test]
async fn c03_versions_negotiated() {
    let b = start(FakeConfig::default()).await;
    for (pref, expect) in [(VersionPref::V311, Version::V311), (VersionPref::V5, Version::V5), (VersionPref::Auto, Version::V5)] {
        let mut o = plain_opts(b.port);
        o.version = pref;
        assert_eq!(test_connection(&o).await.unwrap().info.version, expect);
    }
}

#[tokio::test]
async fn p01_qos_0_1_2_with_and_without_retain() {
    let b = start(FakeConfig::default()).await;
    for v in [VersionPref::V311, VersionPref::V5] {
        let mut o = plain_opts(b.port);
        o.version = v;
        o.subscriptions = vec![("t/#".into(), 2)];
        let (h, _, mut rx) = connect(o).await.unwrap();
        for qos in 0..=2u8 {
            for retain in [false, true] {
                let token = h.publish(Outgoing { topic: format!("t/{qos}"), payload: b"21.5".to_vec(), qos, retain, props: None });
                let (t, r) = next_published(&mut rx).await;
                assert_eq!(t, token);
                assert_eq!(r.unwrap(), if qos == 0 { PubStatus::Sent } else { PubStatus::Acked });
            }
        }
        h.disconnect().await;
    }
}

#[tokio::test]
async fn p02_publish_not_authorized_reason() {
    let b = start(FakeConfig::default()).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    let (h, _, mut rx) = connect(o).await.unwrap();
    for qos in [1u8, 2] {
        h.publish(Outgoing { topic: "forbidden/x".into(), payload: b"1".to_vec(), qos, ..Default::default() });
        let (_, r) = next_published(&mut rx).await;
        let e = r.unwrap_err();
        assert_eq!(e.key, "error.publishRefused");
        assert_eq!(e.text, "The broker refused the message: not authorized. Reason code 0x87.");
    }
}

#[tokio::test]
async fn messages_and_v5_properties_round_trip() {
    let b = start(FakeConfig::default()).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    o.subscriptions = vec![("cmd/#".into(), 0)];
    let (h, _, mut rx) = connect(o).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let props = Props5 {
        content_type: Some("application/json".into()),
        response_topic: Some("cmd/ack".into()),
        correlation_data: Some(b"c-1041".to_vec()),
        user_properties: vec![("operator".into(), "commissioning".into())],
        message_expiry: Some(30),
        ..Default::default()
    };
    h.publish(Outgoing { topic: "cmd/restart".into(), payload: br#"{"cmd":"restart"}"#.to_vec(), qos: 1, retain: false, props: Some(props) });
    let m = next_message(&mut rx).await;
    assert_eq!(m.topic, "cmd/restart");
    let p = m.props.unwrap();
    assert_eq!(p.content_type.as_deref(), Some("application/json"));
    assert_eq!(p.correlation_data.as_deref(), Some(&b"c-1041"[..]));
    assert_eq!(p.user_properties, vec![("operator".to_string(), "commissioning".to_string())]);
}

#[tokio::test]
async fn p03_delete_retained() {
    let b = start(FakeConfig::default()).await;
    let o = plain_opts(b.port);
    let (h, _, mut rx) = connect(o.clone()).await.unwrap();
    h.publish(Outgoing { topic: "r/setpoint".into(), payload: b"21.5".to_vec(), qos: 1, retain: true, props: None });
    next_published(&mut rx).await.1.unwrap();
    h.publish(Outgoing { topic: "r/setpoint".into(), payload: vec![], qos: 1, retain: true, props: None });
    next_published(&mut rx).await.1.unwrap();
    h.disconnect().await;
}

#[tokio::test]
async fn c06_reconnects_after_loss() {
    let b = start(FakeConfig { behavior: Behavior { disconnect_after: Some((150, 0x8B)), ..Default::default() }, ..Default::default() }).await;
    let mut o = plain_opts(b.port);
    o.version = VersionPref::V5;
    o.reconnect = true;
    o.reconnect_min = Duration::from_millis(100);
    o.subscriptions = vec![("a/#".into(), 0)];
    let (h, _, mut rx) = connect(o).await.unwrap();
    let mut saw_lost = false;
    let mut connected_again = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline && !connected_again {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(ClientEvent::Lost(_))) => saw_lost = true,
            Ok(Some(ClientEvent::Connected(_))) if saw_lost => connected_again = true,
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert!(saw_lost && connected_again);
    h.disconnect().await;
    assert!(b.connects.lock().unwrap().len() >= 2);
}

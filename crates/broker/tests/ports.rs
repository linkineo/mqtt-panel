//! B-03 port validation. In its own test binary: it runs `lsof`, and a process spawned while other test threads
//! create sockets can briefly inherit them, which would make parallel tests see their ports "in use".

mod common;

use common::*;
use mqttpanel_broker::config::*;
use mqttpanel_broker::BrokerManager;
use mqttpanel_profiles::MemorySecrets;
use std::sync::Arc;

fn open(dir: &std::path::Path) -> BrokerManager {
    BrokerManager::open(dir, Arc::new(MemorySecrets::new())).expect("open")
}

fn randomize_ports(m: &BrokerManager) {
    for mut l in m.config().listeners {
        l.port = free_port();
        m.update_listener(l).expect("update port");
    }
}

fn port_of(m: &BrokerManager, id: &str) -> u16 {
    m.config()
        .listeners
        .into_iter()
        .find(|l| l.id == id)
        .unwrap()
        .port
}

#[tokio::test]
async fn b03_port_validation_messages() {
    let dir = tempfile::tempdir().unwrap();
    let m = open(dir.path());
    let key = |p: u32| m.validate_port(p, None).map(|u| u.key);
    assert_eq!(key(80).as_deref(), Some("broker.portPrivileged"));
    assert_eq!(key(1023).as_deref(), Some("broker.portPrivileged"));
    assert_eq!(key(65536).as_deref(), Some("broker.portRange"));
    assert_eq!(key(0).as_deref(), Some("broker.portRange"));
    for p in [1024, 65535] {
        let k = key(p);
        assert!(
            k.is_none()
                || k.as_deref()
                    .is_some_and(|k| k.starts_with("broker.portInUse")),
            "{p}: {k:?}"
        );
    }
    let msg = m.validate_port(80, None).unwrap();
    assert_eq!(
        msg.text,
        "Ports below 1024 need administrator rights; choose 1024 or higher."
    );
    assert_eq!(
        m.validate_port(8883, Some("tcp")).unwrap().key,
        "broker.portDuplicate"
    );
    assert!(m
        .validate_port(8883, Some("tls"))
        .is_none_or(|u| u.key.starts_with("broker.portInUse")));
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bp = busy.local_addr().unwrap().port();
    let u = m.validate_port(u32::from(bp), None).expect("in use");
    assert!(
        u.key == "broker.portInUseBy" || u.key == "broker.portInUse",
        "{}",
        u.key
    );
    if u.key == "broker.portInUseBy" {
        assert!(u.params["process"].as_str().is_some_and(|p| !p.is_empty()));
    }
    assert_eq!(u.params["port"], serde_json::json!(bp));
    // The broker's own running ports are not "in use"; adding a listener on a used port is refused before start.
    randomize_ports(&m);
    m.add_user("u", "p").unwrap();
    assert_eq!(m.start().await.state, BrokerState::Running);
    let tcp_port = port_of(&m, "tcp");
    assert!(m.validate_port(u32::from(tcp_port), Some("tcp")).is_none());
    let e = m
        .add_listener(AddListenerRequest {
            preset: Preset::Plain,
            kind: ListenerKind::Tcp,
            port: u32::from(bp),
            bind: Bind::Network,
            login: None,
        })
        .unwrap_err();
    assert!(e.key().starts_with("broker.portInUse"));
    m.stop().await;
}

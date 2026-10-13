//! Manager tests: defaults (B-01), presets (B-02), ports (B-03), users (B-05), anonymous (B-07), issued
//! certificates (B-08), CSR (B-09), CA rotation (B-10), the "This app's broker" connection settings (B-12), and the CLI path (B-13).

mod common;

use common::*;
use mqttpanel_broker::config::*;
use mqttpanel_broker::runtime::{Broker, RuntimeConfig};
use mqttpanel_broker::BrokerManager;
use mqttpanel_profiles::{MemorySecrets, SecretStore};
use mqttpanel_tls::kit::KeyAlg;
use rustls::pki_types::CertificateDer;
use std::sync::Arc;
use std::time::Duration;

fn open(dir: &std::path::Path) -> (BrokerManager, MemorySecrets) {
    let secrets = MemorySecrets::new();
    let m = BrokerManager::open(dir, Arc::new(secrets.clone())).expect("open");
    (m, secrets)
}

/// Moves every listener to a free port (tests never clash with a broker running on the machine).
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

fn ca_roots(m: &BrokerManager) -> Vec<CertificateDer<'static>> {
    der(&m.ca_pem())
}

fn identity(cert: &[u8], key: Option<&[u8]>, pass: Option<&str>) -> mqttpanel_tls::Identity {
    mqttpanel_tls::load_identity(cert, key, pass).expect("identity")
}

#[tokio::test]
async fn b01_defaults_listeners_accept_and_cert_sans() {
    let dir = tempfile::tempdir().unwrap();
    let (m, secrets) = open(dir.path());
    let c = m.config();
    let summary: Vec<(String, ListenerKind, u16, bool, Bind)> = c
        .listeners
        .iter()
        .map(|l| (l.id.clone(), l.kind, l.port, l.enabled, l.bind))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("tcp".into(), ListenerKind::Tcp, 1883, true, Bind::Network),
            ("tls".into(), ListenerKind::Tls, 8883, true, Bind::Network),
            ("ws".into(), ListenerKind::Ws, 8083, true, Bind::Network),
            ("wss".into(), ListenerKind::Wss, 8084, false, Bind::Network),
        ]
    );
    assert!(!c.anonymous);
    assert_eq!(c.max_clients, 1000);
    let certs = m.broker_certs().unwrap();
    assert!(certs.ca.subject.contains("MQTT Panel Local CA"));
    assert!(certs.ca.is_ca && certs.ca.key_type == "ECDSA P-256");
    assert!(certs.server.san.contains(&"localhost".to_string()));
    for name in mqttpanel_tls::kit::local_sans() {
        assert!(certs.server.san.contains(&name), "SAN {name}");
    }
    let json = std::fs::read_to_string(dir.path().join("broker.json")).unwrap();
    assert!(
        !json.contains("PRIVATE KEY"),
        "keys stay in the secret store"
    );
    assert!(
        secrets.get("broker/ca-key").unwrap().is_some()
            && secrets.get("broker/server-key").unwrap().is_some()
    );

    randomize_ports(&m);
    m.add_user("alice", "pw").unwrap();
    let st = m.start().await;
    assert_eq!(st.state, BrokerState::Running, "{:?}", st.error);
    assert!(st.listeners.iter().filter(|l| l.id != "wss").all(|l| l.up));
    let roots = ca_roots(&m);
    let _tcp = c4(opts4(
        "b01-tcp",
        port_of(&m, "tcp"),
        None,
        Some(("alice", "pw")),
    ))
    .await
    .expect("tcp");
    let _tls = c4(opts4(
        "b01-tls",
        port_of(&m, "tls"),
        Some(tls_cfg(&roots, None)),
        Some(("alice", "pw")),
    ))
    .await
    .expect("tls");
    let ws = port_of(&m, "ws");
    let s = ws_upgrade(tcp(ws).await, "ws", ws, "/mqtt").await.unwrap();
    let mut r = Raw::new(Box::new(s), false);
    assert_eq!(
        r.connect("b01-ws", Some(("alice", "pw")), None, 30).await,
        0
    );
    assert_eq!(m.status().clients, 3);
    // Reopening keeps the same CA.
    let fp = m.broker_certs().unwrap().ca.fingerprint;
    m.stop().await;
    let (m2, _) = (
        BrokerManager::open(dir.path(), Arc::new(secrets.clone())).unwrap(),
        (),
    );
    assert_eq!(m2.broker_certs().unwrap().ca.fingerprint, fp);
}

#[tokio::test]
async fn b02_presets_create_the_right_combination() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    let add = |preset, kind| {
        let port = free_port();
        let cfg = m
            .add_listener(AddListenerRequest {
                preset,
                kind,
                port: u32::from(port),
                bind: Bind::Localhost,
            })
            .unwrap();
        cfg.listeners.into_iter().find(|l| l.port == port).unwrap()
    };
    let plain = add(Preset::Plain, ListenerKind::Tcp);
    let enc = add(Preset::Encrypted, ListenerKind::Tls);
    let pw = add(Preset::Passwords, ListenerKind::Tls);
    assert_eq!(plain.kind, ListenerKind::Tcp);
    assert_eq!(enc.kind, ListenerKind::Tls);
    assert_eq!((pw.kind, pw.preset), (ListenerKind::Tls, Preset::Passwords));
    assert!(
        !m.config().require_client_cert,
        "encrypted presets leave the broker-wide setting alone"
    );
    let mtls = add(Preset::Mtls, ListenerKind::Tls);
    assert_eq!(mtls.preset, Preset::Mtls);
    let cfg = m.config();
    assert!(
        cfg.require_client_cert,
        "B-18: the Mutual TLS preset sets the broker-wide requirement to Required"
    );
    assert!(!cfg.anonymous, "plain does not turn anonymous on");
    // Encrypting a TCP request upgrades it to TLS.
    let up = add(Preset::Encrypted, ListenerKind::Tcp);
    assert_eq!(up.kind, ListenerKind::Tls);

    for l in m.config().listeners {
        if [&plain.id, &enc.id, &pw.id, &mtls.id].contains(&&l.id) {
            continue;
        }
        let mut l = l;
        l.enabled = false;
        m.update_listener(l).unwrap();
    }
    // Logins first with the client certificate optional (broker-wide), then Required below.
    m.set_require_client_cert(false).unwrap();
    m.add_user("shared", "pw-s").unwrap();
    m.add_user("dev", "pw-dev").unwrap();
    assert_eq!(m.start().await.state, BrokerState::Running);
    let roots = ca_roots(&m);
    assert!(c4(opts4("p1", plain.port, None, Some(("shared", "pw-s"))))
        .await
        .is_ok());
    assert!(c4(opts4(
        "p2",
        enc.port,
        Some(tls_cfg(&roots, None)),
        Some(("shared", "pw-s"))
    ))
    .await
    .is_ok());
    assert!(c4(opts4(
        "p3",
        pw.port,
        Some(tls_cfg(&roots, None)),
        Some(("dev", "pw-dev"))
    ))
    .await
    .is_ok());
    // One broker, several ports: every user is accepted on every listener.
    assert!(c4(opts4(
        "p4",
        enc.port,
        Some(tls_cfg(&roots, None)),
        Some(("dev", "pw-dev"))
    ))
    .await
    .is_ok());
    assert!(c4(opts4(
        "p4b",
        pw.port,
        Some(tls_cfg(&roots, None)),
        Some(("dev", "wrong"))
    ))
    .await
    .is_err());
    assert!(c4(opts4(
        "p5",
        mtls.port,
        Some(tls_cfg(&roots, None)),
        Some(("shared", "pw-s"))
    ))
    .await
    .is_ok(), "Optional: a login is enough on every encrypted listener");
    m.set_require_client_cert(true).unwrap();
    assert!(m.status().restart_needed);
    assert_eq!(m.restart().await.state, BrokerState::Running);
    assert!(c4(opts4(
        "p5b",
        mtls.port,
        Some(tls_cfg(&roots, None)),
        Some(("shared", "pw-s"))
    ))
    .await
    .is_err());
    assert!(c4(opts4(
        "p5c",
        enc.port,
        Some(tls_cfg(&roots, None)),
        Some(("shared", "pw-s"))
    ))
    .await
    .is_err(), "B-18: Required applies to every encrypted listener");
    assert!(c4(opts4("p5d", plain.port, None, Some(("shared", "pw-s"))))
        .await
        .is_ok(), "B-18: a plain listener is never affected");
    let files = m
        .issue_client_cert(IssueCertRequest {
            name: "sensor-1".into(),
            valid_years: 2,
            key_type: KeyAlg::Ecdsa,
            format: CertFormat::Pem,
            passphrase: String::new(),
            save_ca: true,
        })
        .unwrap();
    assert_eq!(
        files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        vec!["sensor-1.pem", "sensor-1.key", "CA.pem"]
    );
    let id = identity(&files[0].1, Some(&files[1].1), None);
    assert!(c4(opts4(
        "p6",
        mtls.port,
        Some(tls_cfg(&roots, Some(id.clone()))),
        None
    ))
    .await
    .is_ok());
    assert!(c4(opts4("p7", enc.port, Some(tls_cfg(&roots, Some(id))), None))
        .await
        .is_ok());
    m.stop().await;
}

#[tokio::test]
async fn b05_users_argon2_at_rest_and_logins_on_tcp_and_tls() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    m.add_user("alice", "first-pw").unwrap();
    assert_eq!(
        m.add_user("alice", "x").unwrap_err().key(),
        "broker.userExists"
    );
    assert_eq!(
        m.add_user("with space", "x").unwrap_err().key(),
        "broker.userInvalid"
    );
    assert_eq!(m.add_user("", "x").unwrap_err().key(), "broker.userInvalid");
    m.set_user_password("alice", "second-pw").unwrap();
    let json = std::fs::read_to_string(dir.path().join("broker.json")).unwrap();
    assert!(json.contains("$argon2id$"));
    assert!(!json.contains("second-pw") && !json.contains("first-pw"));
    assert_eq!(
        m.config().users,
        vec![UserView {
            username: "alice".into(),
            system: false,
            has_password: true,
        }]
    );
    // The plain password stays in the secret store so the UI can reveal and copy it.
    assert_eq!(m.user_password("alice").unwrap().as_deref(), Some("second-pw"));
    assert_eq!(m.user_password("nobody").unwrap(), None);
    assert_eq!(m.start().await.state, BrokerState::Running);
    let (tp, sp) = (port_of(&m, "tcp"), port_of(&m, "tls"));
    let roots = ca_roots(&m);
    assert!(c4(opts4("u1", tp, None, Some(("alice", "second-pw"))))
        .await
        .is_ok());
    assert!(c4(opts4(
        "u2",
        sp,
        Some(tls_cfg(&roots, None)),
        Some(("alice", "second-pw"))
    ))
    .await
    .is_ok());
    assert!(c4(opts4("u3", tp, None, Some(("alice", "first-pw"))))
        .await
        .is_err());
    let mut r = Raw::tcp(tp, false).await;
    assert_eq!(
        r.connect("u4", Some(("alice", "wrong")), None, 30).await,
        0x05
    );
    let mut r5 = Raw::new(
        Box::new(tls_stream(sp, tls_cfg(&roots, None)).await.unwrap()),
        true,
    );
    assert_eq!(
        r5.connect("u5", Some(("alice", "wrong")), None, 30).await,
        0x86
    );
    assert!(m
        .log(LogLevel::Warn, 0)
        .iter()
        .any(|l| l.text == "client u5 rejected: bad username or password"));
    // Removal applies at once, without a restart.
    m.remove_user("alice").unwrap();
    assert!(c4(opts4("u6", tp, None, Some(("alice", "second-pw"))))
        .await
        .is_err());
    assert!(!m.status().restart_needed);
    m.stop().await;
}

#[tokio::test]
async fn b07_anonymous_gates_everything_and_off_restores() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    let mp = free_port();
    m.add_listener(AddListenerRequest {
        preset: Preset::Mtls,
        kind: ListenerKind::Tls,
        port: u32::from(mp),
        bind: Bind::Localhost,
    })
    .unwrap();
    m.add_user("alice", "pw").unwrap();
    assert_eq!(m.start().await.state, BrokerState::Running);
    let tp = port_of(&m, "tcp");
    let roots = ca_roots(&m);
    assert!(c4(opts4("a1", tp, None, None)).await.is_err());
    assert!(c4(opts4("a2", mp, Some(tls_cfg(&roots, None)), None))
        .await
        .is_err());
    m.set_anonymous(true).unwrap();
    assert!(m.config().anonymous);
    assert!(c4(opts4("a3", tp, None, None)).await.is_ok());
    assert!(
        c4(opts4("a4", tp, None, Some(("alice", "wrong"))))
            .await
            .is_ok(),
        "users ignored"
    );
    assert!(
        c4(opts4("a5", mp, Some(tls_cfg(&roots, None)), None))
            .await
            .is_ok(),
        "mTLS listener accepts without a certificate"
    );
    m.set_anonymous(false).unwrap();
    assert!(c4(opts4("a6", tp, None, None)).await.is_err());
    assert!(c4(opts4("a7", mp, Some(tls_cfg(&roots, None)), None))
        .await
        .is_err());
    assert!(
        c4(opts4("a8", tp, None, Some(("alice", "pw"))))
            .await
            .is_ok(),
        "previous setup restored"
    );
    assert!(m.config().require_client_cert);
    m.stop().await;
}

#[tokio::test]
async fn b08_issue_export_pem_and_pkcs12_then_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let (m, secrets) = open(dir.path());
    randomize_ports(&m);
    let mut tls = m
        .config()
        .listeners
        .into_iter()
        .find(|l| l.id == "tls")
        .unwrap();
    tls.preset = Preset::Mtls;
    m.update_listener(tls).unwrap();
    m.set_require_client_cert(true).unwrap();
    let req = |name: &str, format, pass: &str| IssueCertRequest {
        name: name.into(),
        valid_years: 2,
        key_type: KeyAlg::Ecdsa,
        format,
        passphrase: pass.into(),
        save_ca: false,
    };
    let pem = m
        .issue_client_cert(req("dev-pem", CertFormat::Pem, "file-pass"))
        .unwrap();
    assert!(String::from_utf8_lossy(&pem[1].1).contains("ENCRYPTED PRIVATE KEY"));
    assert_eq!(
        secrets
            .get("broker/client/dev-pem/passphrase")
            .unwrap()
            .as_deref(),
        Some("file-pass")
    );
    assert!(secrets.get("broker/client/dev-pem/key").unwrap().is_some());
    let p12 = m
        .issue_client_cert(req("dev-p12", CertFormat::Pkcs12, "p12-pass"))
        .unwrap();
    assert_eq!(p12[0].0, "dev-p12.p12");
    assert_eq!(
        m.issue_client_cert(req("dev-pem", CertFormat::Pem, ""))
            .unwrap_err()
            .key(),
        "broker.certExists"
    );
    assert_eq!(
        m.issue_client_cert(req("bad name", CertFormat::Pem, ""))
            .unwrap_err()
            .key(),
        "broker.nameInvalid"
    );
    assert_eq!(m.start().await.state, BrokerState::Running);
    let port = port_of(&m, "tls");
    let roots = ca_roots(&m);
    let id_pem = identity(&pem[0].1, Some(&pem[1].1), Some("file-pass"));
    let c = c4(opts4(
        "dev-pem",
        port,
        Some(tls_cfg(&roots, Some(id_pem.clone()))),
        None,
    ))
    .await
    .expect("PEM identity");
    let id_p12 = identity(&p12[0].1, None, Some("p12-pass"));
    assert!(
        c4(opts4(
            "dev-p12",
            port,
            Some(tls_cfg(&roots, Some(id_p12))),
            None
        ))
        .await
        .is_ok(),
        "PKCS#12 identity"
    );
    let seen = |name: &str| {
        m.issued_certs()
            .into_iter()
            .find(|c| c.name == name)
            .unwrap()
            .last_seen
    };
    assert_eq!(seen("dev-pem"), LastSeen::Connected);
    drop(c);
    assert!(eventually(|| matches!(seen("dev-pem"), LastSeen::At(_))).await);
    // Export again later (key kept in the secret store).
    let again = m
        .export_client_cert("dev-pem", CertFormat::Pkcs12, "other")
        .unwrap();
    assert!(c4(opts4(
        "dev-pem-2",
        port,
        Some(tls_cfg(
            &roots,
            Some(identity(&again[0].1, None, Some("other")))
        )),
        None
    ))
    .await
    .is_ok());
    // Revoke: refused at the next handshake, without a restart.
    let list = m.revoke_client_cert("dev-pem").unwrap();
    assert!(list.iter().find(|c| c.name == "dev-pem").unwrap().revoked);
    assert!(c4(opts4(
        "dev-pem-3",
        port,
        Some(tls_cfg(&roots, Some(id_pem))),
        None
    ))
    .await
    .is_err());
    assert!(eventually(|| m.log_buffer().contains("revoked certificate dev-pem")).await);
    assert!(!m.status().restart_needed);
    let json = std::fs::read_to_string(dir.path().join("broker.json")).unwrap();
    assert!(!json.contains("PRIVATE KEY"));
    m.stop().await;
}

#[tokio::test]
async fn b09_csr_import_device_keeps_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    m.set_require_client_cert(true).unwrap();
    let (csr, key_der) = mqttpanel_tls::kit::make_csr("device-7", KeyAlg::Ecdsa).unwrap();
    let (fname, cert) = m.import_csr(csr.as_bytes(), None, 2).unwrap();
    assert_eq!(fname, "device-7.pem");
    assert_eq!(
        m.import_csr(b"not a csr", Some("x"), 2).unwrap_err().key(),
        "file.notCsr"
    );
    let ic = m.issued_certs();
    assert!(ic.iter().any(|c| c.name == "device-7" && !c.system));
    // Export of a CSR-signed certificate gives the certificate only.
    let ex = m
        .export_client_cert("device-7", CertFormat::Pkcs12, "")
        .unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(m.start().await.state, BrokerState::Running);
    let key_pem = mqttpanel_tls::to_pem("PRIVATE KEY", &key_der);
    let id = identity(&cert, Some(key_pem.as_bytes()), None);
    assert!(c4(opts4(
        "device-7",
        port_of(&m, "tls"),
        Some(tls_cfg(&ca_roots(&m), Some(id))),
        None
    ))
    .await
    .is_ok());
    m.stop().await;
}

#[tokio::test]
async fn b10_rotate_ca_refuses_old_certs_and_reissues_desktop_cert() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    let mp = free_port();
    m.add_listener(AddListenerRequest {
        preset: Preset::Mtls,
        kind: ListenerKind::Tls,
        port: u32::from(mp),
        bind: Bind::Localhost,
    })
    .unwrap();
    let files = m
        .issue_client_cert(IssueCertRequest {
            name: "old-dev".into(),
            valid_years: 1,
            key_type: KeyAlg::Ecdsa,
            format: CertFormat::Pem,
            passphrase: String::new(),
            save_ca: false,
        })
        .unwrap();
    let lc1 = m.local_connection(Some("tls-2")).unwrap();
    assert_eq!(lc1.port, mp);
    assert_eq!(m.start().await.state, BrokerState::Running);
    let old_roots = ca_roots(&m);
    let old_id = identity(&files[0].1, Some(&files[1].1), None);
    assert!(c4(opts4(
        "old-dev",
        mp,
        Some(tls_cfg(&old_roots, Some(old_id.clone()))),
        None
    ))
    .await
    .is_ok());
    let fp_before = m.broker_certs().unwrap().ca.fingerprint;
    m.rotate_ca().unwrap();
    assert_ne!(m.broker_certs().unwrap().ca.fingerprint, fp_before);
    assert!(m.status().restart_needed);
    assert!(m
        .log_buffer()
        .contains("CA rotated: issued client certificates must be re-issued"));
    assert!(m.issued_certs().iter().all(|c| c.superseded));
    assert_eq!(m.restart().await.state, BrokerState::Running);
    let new_roots = ca_roots(&m);
    assert!(c4(opts4(
        "old-dev-2",
        mp,
        Some(tls_cfg(&new_roots, Some(old_id))),
        None
    ))
    .await
    .is_err());
    assert!(
        eventually(|| m.log_buffer().contains("unknown CA")).await,
        "{:?}",
        m.log(LogLevel::Warn, 0)
    );
    let lc2 = m.local_connection(Some("tls-2")).unwrap();
    assert_ne!(
        lc1.client_cert_pem, lc2.client_cert_pem,
        "desktop certificate re-issued"
    );
    let id = identity(
        lc2.client_cert_pem.as_ref().unwrap().as_bytes(),
        Some(lc2.client_key_pem.as_ref().unwrap().as_bytes()),
        None,
    );
    let roots = der(lc2.ca_pem.as_ref().unwrap());
    assert!(
        c4(opts4("desktop", mp, Some(tls_cfg(&roots, Some(id))), None))
            .await
            .is_ok()
    );
    m.stop().await;
}

#[tokio::test]
async fn b12_local_connection_picks_most_secure_listener() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    let lc = m.local_connection(None).unwrap();
    assert_eq!(
        (
            lc.listener_id.as_str(),
            lc.scheme.as_str(),
            lc.host.as_str(),
            lc.port
        ),
        ("tls", "mqtts", "localhost", 8883)
    );
    assert!(lc.ca_pem.is_some() && lc.client_cert_pem.is_none());
    let mp = free_port();
    m.add_listener(AddListenerRequest {
        preset: Preset::Mtls,
        kind: ListenerKind::Tls,
        port: u32::from(mp),
        bind: Bind::Localhost,
    })
    .unwrap();
    let mut first_pw = None;
    assert!(m.config().require_client_cert);
    for _ in 0..3 {
        // Broker-wide: both TLS listeners are mutual TLS now; the first one ranks first.
        let lc = m.local_connection(None).unwrap();
        assert_eq!(lc.listener_id, "tls");
        assert_eq!(m.local_connection(Some("tls-2")).unwrap().port, mp);
        assert!(lc.client_cert_pem.is_some() && lc.client_key_pem.is_some());
        assert_eq!(lc.username.as_deref(), Some("mqtt-panel"));
        let pw = lc.password.clone().unwrap();
        assert!(pw.len() >= 20 && pw.chars().all(|c| c.is_ascii_alphanumeric()), "strong generated password");
        if let Some(p) = &first_pw {
            assert_eq!(p, &pw, "the user keeps its password on later runs");
        }
        first_pw = Some(pw);
    }
    let users = m.config().users;
    assert_eq!(users.len(), 1, "user created once: {users:?}");
    assert!(users[0].system && users[0].has_password);
    assert_eq!(m.user_password("mqtt-panel").unwrap(), first_pw);
    assert!(m.log_buffer().contains("user mqtt-panel created for the profile \"Local broker\""));
    let desktop: Vec<IssuedCert> = m.issued_certs().into_iter().filter(|c| c.system).collect();
    assert_eq!(desktop.len(), 1, "issued once");
    assert_eq!(desktop[0].name, "mqttpanel-desktop");
    // Ranking: mutual TLS, TLS, secure WebSocket, TCP, WebSocket.
    let rank_order = |m: &BrokerManager| m.preferred_listener().map(|l| l.id);
    assert_eq!(rank_order(&m).as_deref(), Some("tls"));
    let mut cfg = m.config();
    for l in cfg.listeners.iter_mut() {
        l.enabled = l.kind == ListenerKind::Wss || l.kind == ListenerKind::Tcp || l.kind == ListenerKind::Ws;
        m.update_listener(l.clone()).unwrap();
    }
    assert_eq!(rank_order(&m).as_deref(), Some("wss"));
    for mut l in m.config().listeners {
        if l.kind.encrypted() {
            l.enabled = false;
            m.update_listener(l).unwrap();
        }
    }
    assert_eq!(m.local_connection(None).unwrap().scheme, "mqtt");
    let ws = m.local_connection(Some("ws")).unwrap();
    assert_eq!(
        (ws.scheme.as_str(), ws.ws_path.as_deref()),
        ("ws", Some("/mqtt"))
    );
    // With Anonymous on no user and no client certificate are needed.
    m.set_anonymous(true).unwrap();
    let lc = m.local_connection(Some("tls-2")).unwrap();
    assert!(lc.username.is_none() && lc.password.is_none() && lc.client_cert_pem.is_none());
    assert!(!m.needs_desktop_cert("tls-2"));
}

#[tokio::test]
async fn b13_cli_runtime_config_from_manager_with_bind_override() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    m.add_user("cli", "pw").unwrap();
    let cfg = RuntimeConfig::from_manager(&m)
        .unwrap()
        .with_bind("127.0.0.1".parse().unwrap());
    let b = Broker::start(cfg).await.unwrap();
    assert!(c4(opts4(
        "cli-1",
        port_of(&m, "tcp"),
        None,
        Some(("cli", "pw"))
    ))
    .await
    .is_ok());
    assert!(c4(opts4(
        "cli-2",
        port_of(&m, "tls"),
        Some(tls_cfg(&ca_roots(&m), None)),
        Some(("cli", "pw"))
    ))
    .await
    .is_ok());
    b.stop().await;
}

#[tokio::test]
async fn status_counts_and_restart_notice() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    randomize_ports(&m);
    m.set_anonymous(true).unwrap();
    let st = m.start().await;
    assert_eq!(st.state, BrokerState::Running);
    assert!(st.started_at.is_some());
    let c = c4(opts4("s1", port_of(&m, "tcp"), None, None))
        .await
        .unwrap();
    c.publish("x/y", 1, true, "1", None).await;
    assert!(eventually(|| m.status().retained == 1 && m.status().clients == 1).await);
    assert!(m.status().msg_in_rate > 0.0);
    assert_eq!(m.clients()[0].client_id, "s1");
    m.disconnect_client("s1").unwrap();
    assert!(eventually(|| m.clients().is_empty()).await);
    m.regenerate(&["broker.lab".to_string()], KeyAlg::Ecdsa)
        .unwrap();
    assert!(m.status().restart_needed);
    assert_eq!(
        m.broker_certs().unwrap().server.san,
        vec!["broker.lab".to_string()]
    );
    m.stop().await;
    assert_eq!(
        m.disconnect_client("s1").unwrap_err().key(),
        "broker.notRunning"
    );
    let json = serde_json::to_value(m.status()).unwrap();
    for k in [
        "state",
        "startedAt",
        "clients",
        "maxClients",
        "msgInRate",
        "msgOutRate",
        "retained",
        "restartNeeded",
        "error",
        "listeners",
    ] {
        assert!(json.get(k).is_some(), "status.{k}");
    }
    let ic = serde_json::to_value(m.issued_certs()).unwrap();
    assert!(ic.as_array().unwrap().is_empty());
    tokio::time::sleep(Duration::from_millis(10)).await;
}

#[tokio::test]
async fn import_server_cert_and_client_ca() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = open(dir.path());
    let k = kit();
    let certs = m
        .import_server_cert(
            k.server.cert_pem.as_bytes(),
            Some(k.server.key_pem.as_bytes()),
            None,
        )
        .unwrap();
    assert!(certs.imported);
    assert_eq!(
        m.import_server_cert(
            k.server.cert_pem.as_bytes(),
            Some(k.client_ok.key_pem.as_bytes()),
            None
        )
        .unwrap_err()
        .key(),
        "tls.keyMismatch"
    );
    let certs = m
        .import_server_cert(&k.pkcs12_with_pass, None, Some("test-passphrase"))
        .unwrap();
    assert!(certs.server.subject.contains("client-ok"));
    assert_eq!(
        m.import_server_cert(&k.pkcs12_with_pass, None, Some("bad"))
            .unwrap_err()
            .key(),
        "tls.wrongPassphrase"
    );
    assert_eq!(
        m.set_verify_against(VerifyAgainst::Imported)
            .unwrap_err()
            .key(),
        "file.notCertificate"
    );
    let certs = m.import_client_ca(k.root.cert_pem.as_bytes()).unwrap();
    assert!(certs.imported_ca.is_some());
    assert_eq!(
        m.set_verify_against(VerifyAgainst::Imported)
            .unwrap()
            .verify_against,
        VerifyAgainst::Imported
    );
    assert_eq!(
        m.import_client_ca(b"garbage").unwrap_err().key(),
        "file.notCertificate"
    );
    // With the kit server certificate and the kit root as client CA, kit client certificates are accepted.
    m.import_server_cert(
        k.server.cert_pem.as_bytes(),
        Some(k.server.key_pem.as_bytes()),
        None,
    )
    .unwrap();
    randomize_ports(&m);
    m.set_require_client_cert(true).unwrap();
    assert_eq!(m.start().await.state, BrokerState::Running);
    assert!(c4(opts4(
        "imp",
        port_of(&m, "tls"),
        Some(kit_tls(Some(&k.client_ok))),
        None
    ))
    .await
    .is_ok());
    let (name, ca) = m.export_ca();
    assert_eq!(name, "CA.pem");
    assert!(String::from_utf8(ca).unwrap().contains("BEGIN CERTIFICATE"));
    m.stop().await;
}

#[tokio::test]
async fn b18_client_certificate_requirement_is_broker_wide() {
    let dir = tempfile::tempdir().unwrap();
    let (m, secrets) = open(dir.path());
    randomize_ports(&m);
    let cfg = m.config();
    assert!(!cfg.require_client_cert, "Optional by default");
    let json = serde_json::to_value(&cfg).unwrap();
    assert_eq!(json["requireClientCert"], serde_json::Value::Bool(false));
    assert!(
        json["listeners"][0].get("requireClientCert").is_none(),
        "no per-listener field any more"
    );
    // Stopped: no restart needed, applied at the next start.
    m.set_require_client_cert(true).unwrap();
    assert!(!m.status().restart_needed);
    assert!(m.config().require_client_cert);
    assert!(m.log_buffer().contains("client certificate required on encrypted listeners"));
    // Persisted at the top of broker.json.
    let text = std::fs::read_to_string(dir.path().join("broker.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["requireClientCert"], serde_json::Value::Bool(true));
    drop(m);
    let m = BrokerManager::open(dir.path(), Arc::new(secrets.clone())).unwrap();
    assert!(m.config().require_client_cert, "kept across restarts of the app");
    // Running: a change of the setting asks for a restart; setting the same value does not.
    m.add_user("u", "pw").unwrap();
    for mut l in m.config().listeners {
        l.enabled = l.kind == ListenerKind::Tcp || l.kind == ListenerKind::Tls;
        m.update_listener(l).unwrap();
    }
    assert_eq!(m.start().await.state, BrokerState::Running);
    m.set_require_client_cert(true).unwrap();
    assert!(!m.status().restart_needed);
    let roots = ca_roots(&m);
    assert!(c4(opts4("tcp", port_of(&m, "tcp"), None, Some(("u", "pw")))).await.is_ok());
    assert!(c4(opts4("tls", port_of(&m, "tls"), Some(tls_cfg(&roots, None)), Some(("u", "pw"))))
        .await
        .is_err());
    m.set_require_client_cert(false).unwrap();
    assert!(m.status().restart_needed);
    assert_eq!(m.restart().await.state, BrokerState::Running);
    assert!(c4(opts4("tls2", port_of(&m, "tls"), Some(tls_cfg(&roots, None)), Some(("u", "pw"))))
        .await
        .is_ok());
    m.stop().await;
}

/// Counts reads and writes of the secret store.
#[derive(Default)]
struct Counting {
    inner: MemorySecrets,
    gets: std::sync::atomic::AtomicUsize,
}

impl SecretStore for Counting {
    fn get(&self, key: &str) -> Result<Option<String>, mqttpanel_profiles::SecretError> {
        self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get(key)
    }
    fn set(&self, key: &str, value: &str) -> Result<(), mqttpanel_profiles::SecretError> {
        self.inner.set(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), mqttpanel_profiles::SecretError> {
        self.inner.delete(key)
    }
    /// Like the secrets file: answered from the record names, no decryption.
    fn contains(&self, key: &str) -> Result<bool, mqttpanel_profiles::SecretError> {
        self.inner.contains(key)
    }
}

#[tokio::test]
async fn v09_broker_restart_never_reads_the_secret_store() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (m, secrets) = open(dir.path());
        randomize_ports(&m);
        m.add_user("alice", "pw").unwrap();
        // carry the secrets over to a new manager, as a new session would find them in secrets.enc
        let c = Counting::default();
        for k in ["broker/ca-key", "broker/server-key", "broker/user/alice/password"] {
            c.inner.set(k, &secrets.get(k).unwrap().unwrap()).unwrap();
        }
        drop(m);
        let c = Arc::new(c);
        let m = BrokerManager::open(dir.path(), c.clone()).unwrap();
        let reads = || c.gets.load(std::sync::atomic::Ordering::SeqCst);
        let _ = m.config();
        let _ = m.status();
        assert_eq!(reads(), 0, "opening, the config and the status read no secret");
        assert_eq!(m.start().await.state, BrokerState::Running);
        let after_start = reads();
        assert_eq!(after_start, 1, "the first start reads the server key once");
        for _ in 0..3 {
            assert_eq!(m.restart().await.state, BrokerState::Running);
        }
        m.stop().await;
        assert_eq!(m.start().await.state, BrokerState::Running);
        assert_eq!(reads(), after_start, "restarts use the key kept in memory");
        m.stop().await;
    }
}

//! B-15 (change set 7): every broker change applies while the broker runs and drops exactly the sessions it
//! invalidates; the broker never stops; one log line per applied change; a listener that cannot start fails alone.

mod common;

use common::*;
use mqttpanel_broker::config::*;
use mqttpanel_broker::BrokerManager;
use mqttpanel_profiles::MemorySecrets;
use mqttpanel_tls::kit::KeyAlg;
use std::sync::Arc;

fn open(dir: &std::path::Path) -> BrokerManager {
    BrokerManager::open(dir, Arc::new(MemorySecrets::new())).expect("open")
}

fn listener(m: &BrokerManager, id: &str) -> Listener {
    m.config()
        .listeners
        .into_iter()
        .find(|l| l.id == id)
        .unwrap()
}

fn port_of(m: &BrokerManager, id: &str) -> u16 {
    listener(m, id).port
}

fn issue(m: &BrokerManager, name: &str) -> mqttpanel_tls::Identity {
    let files = m
        .issue_client_cert(IssueCertRequest {
            name: name.into(),
            valid_years: 1,
            key_type: KeyAlg::Ecdsa,
            format: CertFormat::Pem,
            passphrase: String::new(),
            save_ca: false,
        })
        .unwrap();
    mqttpanel_tls::load_identity(&files[0].1, Some(&files[1].1), None).unwrap()
}

/// Log lines written since `seq`.
fn lines_since(m: &BrokerManager, seq: u64) -> Vec<String> {
    m.log(LogLevel::Debug, seq)
        .into_iter()
        .map(|l| l.text)
        .filter(|t| !is_client_line(t) && !t.starts_with("TLS handshake"))
        .collect()
}

/// "client X connected …", "client X disconnected …" and the like: not lines of an applied change.
fn is_client_line(t: &str) -> bool {
    let mut w = t.split(' ');
    w.next() == Some("client")
        && w.nth(1).is_some_and(|v| {
            [
                "connected",
                "disconnected",
                "disconnected:",
                "subscribed",
                "unsubscribed",
                "rejected:",
                "taken",
            ]
            .contains(&v)
        })
}

/// Asserts the broker still runs and was not restarted.
fn running(m: &BrokerManager, started_at: i64) {
    let st = m.status();
    assert_eq!(st.state, BrokerState::Running, "{:?}", st.error);
    assert_eq!(st.started_at, Some(started_at), "the broker never restarts");
}

async fn all_open(clients: &mut [&mut Client], what: &str) {
    for (i, c) in clients.iter_mut().enumerate() {
        assert!(
            still_open(c).await,
            "{what}: client {i} must stay connected"
        );
    }
}

#[tokio::test]
async fn b15_live_apply_drops_exactly_the_invalidated_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let m = open(dir.path());
    // Free ports, WS and WSS off at first.
    for mut l in m.config().listeners {
        l.port = free_port();
        l.enabled = matches!(l.id.as_str(), "tcp" | "tls");
        m.update_listener(l).await.unwrap();
    }
    m.add_user("u", "pw-u").unwrap();
    m.add_user("d", "pw-d").unwrap();
    let dev = issue(&m, "dev");
    let st = m.start().await;
    assert_eq!(st.state, BrokerState::Running, "{:?}", st.error);
    let started_at = st.started_at.unwrap();
    let roots = der(&m.ca_pem());
    let tcp = port_of(&m, "tcp");
    let mut tls = port_of(&m, "tls");
    let tls_pw = |port: u16, roots: &[rustls::pki_types::CertificateDer<'static>]| {
        opts5(
            "b-tls-pw",
            port,
            Some(tls_cfg(roots, None)),
            Some(("u", "pw-u")),
        )
    };
    let tls_cert = |id: &str, port: u16, roots, ident: &mqttpanel_tls::Identity| {
        // Optional: the certificate comes with a login (the device's own user).
        opts5(
            id,
            port,
            Some(tls_cfg(roots, Some(ident.clone()))),
            Some(("d", "pw-d")),
        )
    };
    let mut a = c5(opts5("a-tcp-pw", tcp, None, Some(("u", "pw-u"))))
        .await
        .unwrap();
    let mut b = c5(tls_pw(tls, &roots)).await.unwrap();
    let mut c = c5(tls_cert("c-tls-cert", tls, &roots, &dev)).await.unwrap();
    all_open(&mut [&mut a, &mut b, &mut c], "start").await;

    // 1. Add a listener (switch WS on): nobody drops, it accepts.
    let seq = m.log_buffer().last_seq();
    let mut ws = listener(&m, "ws");
    ws.enabled = true;
    m.update_listener(ws.clone()).await.unwrap();
    assert_eq!(
        lines_since(&m, seq),
        vec![format!("listener WS {} started", ws.port)]
    );
    assert!(m.status().listeners.iter().any(|l| l.id == "ws" && l.up));
    let mut raw = tcp_stream_ws(ws.port).await;
    assert_eq!(
        raw.connect("ws-client", Some(("u", "pw-u")), None, 30)
            .await,
        0,
        "the new listener accepts"
    );
    all_open(&mut [&mut a, &mut b, &mut c], "add listener").await;
    running(&m, started_at);

    // 2. Port of TLS: only the two TLS sessions drop; they reconnect on the new port.
    let seq = m.log_buffer().last_seq();
    let mut l = listener(&m, "tls");
    let old = l.port;
    l.port = free_port();
    tls = l.port;
    m.update_listener(l).await.unwrap();
    assert!(
        dropped(&mut b).await && dropped(&mut c).await,
        "TLS sessions drop"
    );
    all_open(&mut [&mut a], "port change").await;
    assert_eq!(
        lines_since(&m, seq),
        vec![
            format!("listener TLS {old} stopped"),
            format!("listener TLS {tls} started · 2 sessions dropped: port changed"),
        ]
    );
    let mut b = c5(tls_pw(tls, &roots)).await.unwrap();
    let mut c = c5(tls_cert("c-tls-cert", tls, &roots, &dev)).await.unwrap();
    running(&m, started_at);

    // 3. Reachability both ways: nobody drops, new binds take effect.
    for bind in [Bind::Localhost, Bind::Network] {
        let seq = m.log_buffer().last_seq();
        m.set_bind(bind).await.unwrap();
        assert_eq!(lines_since(&m, seq).len(), 1, "{:?}", lines_since(&m, seq));
        all_open(&mut [&mut a, &mut b, &mut c], "reachability").await;
        assert!(c5(opts5("probe", tcp, None, Some(("u", "pw-u"))))
            .await
            .is_ok());
    }
    running(&m, started_at);

    // 4. Regenerate the server certificate: nobody drops; a new handshake sees the new certificate (here one that
    // does not name localhost, so the check of the name fails), then back to localhost.
    let seq = m.log_buffer().last_seq();
    m.regenerate(&["live.lab".to_string()], KeyAlg::Ecdsa)
        .unwrap();
    assert_eq!(
        lines_since(&m, seq),
        vec!["server certificate regenerated".to_string()]
    );
    let e = c5(opts5(
        "probe-tls",
        tls,
        Some(tls_cfg(&roots, None)),
        Some(("u", "pw-u")),
    ))
    .await
    .err()
    .unwrap_or_default();
    assert!(
        e.contains("certificate") || e.contains("Name"),
        "new certificate in use: {e}"
    );
    m.regenerate(&[], KeyAlg::Ecdsa).unwrap();
    assert!(c5(opts5(
        "probe-tls",
        tls,
        Some(tls_cfg(&roots, None)),
        Some(("u", "pw-u"))
    ))
    .await
    .is_ok());
    all_open(&mut [&mut a, &mut b, &mut c], "regenerate").await;
    running(&m, started_at);

    // 5. Rotate the CA: only the client-certificate session drops.
    let seq = m.log_buffer().last_seq();
    m.rotate_ca().unwrap();
    assert!(dropped(&mut c).await, "certificate from the old CA");
    all_open(&mut [&mut a, &mut b], "CA rotation").await;
    assert_eq!(
        lines_since(&m, seq),
        vec!["CA rotated: issued client certificates must be re-issued · 1 session dropped: CA rotated".to_string()]
    );
    let roots = der(&m.ca_pem());
    let dev2 = issue(&m, "dev2");
    let mut c = c5(tls_cert("c-tls-cert", tls, &roots, &dev2))
        .await
        .unwrap();
    running(&m, started_at);

    // 6. Optional -> Required: only the password session on TLS drops.
    let seq = m.log_buffer().last_seq();
    m.set_require_client_cert(true).unwrap();
    assert!(dropped(&mut b).await, "password session on TLS");
    all_open(&mut [&mut a, &mut c], "Required").await;
    assert_eq!(
        lines_since(&m, seq),
        vec!["client certificate required on encrypted listeners · 1 session dropped: client certificate required"
            .to_string()]
    );

    // 7. Required -> Optional: nobody drops.
    let seq = m.log_buffer().last_seq();
    m.set_require_client_cert(false).unwrap();
    all_open(&mut [&mut a, &mut c], "Optional").await;
    assert_eq!(
        lines_since(&m, seq),
        vec!["client certificate optional on encrypted listeners".to_string()]
    );
    let mut b = c5(tls_pw(tls, &roots)).await.unwrap();
    running(&m, started_at);

    // 8. Revoke the certificate: its session drops at once.
    let seq = m.log_buffer().last_seq();
    m.revoke_client_cert("dev2").unwrap();
    assert!(dropped(&mut c).await, "revoked certificate");
    all_open(&mut [&mut a, &mut b], "revoke").await;
    assert_eq!(
        lines_since(&m, seq),
        vec![
            "client certificate dev2 revoked · 1 session dropped: certificate revoked".to_string()
        ]
    );

    // 9. Remove the user: its sessions drop (TCP, TLS and the WS client of step 1), the other user's do not.
    let mut e = c5(opts5("e-tcp-pw", tcp, None, Some(("d", "pw-d"))))
        .await
        .unwrap();
    let seq = m.log_buffer().last_seq();
    m.remove_user("u").unwrap();
    assert!(
        dropped(&mut a).await && dropped(&mut b).await,
        "sessions of the removed user"
    );
    all_open(&mut [&mut e], "remove user").await;
    assert_eq!(
        raw.read().await,
        Pkt::Disconnect(0x87),
        "the WS session of user u"
    );
    assert_eq!(
        lines_since(&m, seq),
        vec!["user u removed · 3 sessions dropped: user removed".to_string()]
    );
    // A password change drops that user's sessions too.
    let seq = m.log_buffer().last_seq();
    m.set_user_password("d", "pw-d2").unwrap();
    assert!(dropped(&mut e).await, "password changed");
    assert_eq!(
        lines_since(&m, seq),
        vec!["password of user d changed · 1 session dropped: password changed".to_string()]
    );
    running(&m, started_at);

    // 10. Anonymous on, then off with an anonymous client: only that one drops on off.
    let mut e = c5(opts5("e-tcp-pw", tcp, None, Some(("d", "pw-d2"))))
        .await
        .unwrap();
    m.set_anonymous(true).unwrap();
    all_open(&mut [&mut e], "Anonymous on").await;
    let mut anon = c5(opts5("anon", tcp, None, None)).await.unwrap();
    let seq = m.log_buffer().last_seq();
    m.set_anonymous(false).unwrap();
    assert!(dropped(&mut anon).await, "anonymous session");
    all_open(&mut [&mut e], "Anonymous off").await;
    assert_eq!(
        lines_since(&m, seq),
        vec!["anonymous access off · 1 session dropped: anonymous access off".to_string()]
    );
    running(&m, started_at);

    // 11. A listener on a taken port fails alone: its row has the error, the others keep serving.
    let mut wss = listener(&m, "wss");
    let taken = free_port();
    wss.port = taken;
    m.update_listener(wss.clone()).await.unwrap(); // still off: nothing starts
                                                   // Another program on the port, on every interface (dual stack where available).
    let _blocker = std::net::TcpListener::bind(("::", taken))
        .or_else(|_| std::net::TcpListener::bind(("0.0.0.0", taken)))
        .unwrap();
    let seq = m.log_buffer().last_seq();
    wss.enabled = true;
    m.update_listener(wss).await.unwrap();
    let st = m.status();
    let row = st.listeners.iter().find(|l| l.id == "wss").unwrap();
    assert!(!row.up);
    assert_eq!(
        row.error.as_ref().map(|e| e.key.as_str()),
        Some("broker.listenerFailed")
    );
    assert!(
        st.listeners.iter().filter(|l| l.id != "wss").all(|l| l.up),
        "{:?}",
        st.listeners
    );
    assert_eq!(
        lines_since(&m, seq),
        vec![format!(
            "listener WSS {taken} could not start: port already in use"
        )]
    );
    all_open(&mut [&mut e], "taken port").await;
    running(&m, started_at);

    // 12. Remove a listener: only its sessions drop.
    let mut t = c5(opts5(
        "t-tls",
        tls,
        Some(tls_cfg(&roots, None)),
        Some(("d", "pw-d2")),
    ))
    .await
    .unwrap();
    let seq = m.log_buffer().last_seq();
    m.remove_listener("tls").await.unwrap();
    assert!(dropped(&mut t).await);
    all_open(&mut [&mut e], "remove listener").await;
    assert_eq!(
        lines_since(&m, seq),
        vec![format!(
            "listener TLS {tls} stopped · 1 session dropped: listener removed"
        )]
    );
    running(&m, started_at);
    m.stop().await;
}

/// A listener on a WebSocket port: the HTTP upgrade then a raw MQTT client.
async fn tcp_stream_ws(port: u16) -> Raw {
    let s = tcp(port).await;
    let ws = ws_upgrade(s, "ws", port, "/mqtt").await.expect("upgrade");
    Raw::new(Box::new(ws), true)
}

/// B-15, "Verify against": switching the verifying CA drops the sessions whose certificate the new CA did not sign,
/// and only those; the switch back does the same the other way.
#[tokio::test]
async fn b15_verify_against_drops_certificates_of_the_other_ca() {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let dir = tempfile::tempdir().unwrap();
    let m = open(dir.path());
    for mut l in m.config().listeners {
        l.port = free_port();
        l.enabled = matches!(l.id.as_str(), "tcp" | "tls");
        m.update_listener(l).await.unwrap();
    }
    m.set_require_client_cert(true).unwrap();
    m.add_user("u", "pw").unwrap();
    let local = issue(&m, "local-dev");
    let k = kit();
    m.import_client_ca(k.root.cert_pem.as_bytes()).unwrap();
    let foreign = mqttpanel_tls::Identity {
        certs: vec![CertificateDer::from(k.client_ok.cert_der.clone())],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(k.client_ok.key_der.clone())),
    };
    let started_at = m.start().await.started_at.unwrap();
    let roots = der(&m.ca_pem());
    let (tcp, tls) = (port_of(&m, "tcp"), port_of(&m, "tls"));
    let mut plain = c5(opts5("plain", tcp, None, Some(("u", "pw"))))
        .await
        .unwrap();
    let mut a = c5(opts5(
        "local-dev",
        tls,
        Some(tls_cfg(&roots, Some(local.clone()))),
        None,
    ))
    .await
    .unwrap();
    assert!(c5(opts5(
        "foreign",
        tls,
        Some(tls_cfg(&roots, Some(foreign.clone()))),
        None
    ))
    .await
    .is_err());

    let seq = m.log_buffer().last_seq();
    m.set_verify_against(VerifyAgainst::Imported).unwrap();
    assert!(dropped(&mut a).await, "signed by the local CA");
    all_open(&mut [&mut plain], "verify against").await;
    assert_eq!(
        lines_since(&m, seq),
        vec!["client certificates verified against the imported CA · 1 session dropped: verifying CA changed".to_string()]
    );
    let mut b = c5(opts5(
        "foreign",
        tls,
        Some(tls_cfg(&roots, Some(foreign.clone()))),
        None,
    ))
    .await
    .unwrap();
    assert!(c5(opts5(
        "local-dev",
        tls,
        Some(tls_cfg(&roots, Some(local.clone()))),
        None
    ))
    .await
    .is_err());

    // The server certificate changes, the verifying CA does not: nobody drops.
    m.regenerate(&[], KeyAlg::Ecdsa).unwrap();
    all_open(&mut [&mut plain, &mut b], "regenerate").await;

    m.set_verify_against(VerifyAgainst::Local).unwrap();
    assert!(dropped(&mut b).await, "signed by the imported CA");
    all_open(&mut [&mut plain], "verify against local").await;

    // Anonymous on drops nothing.
    let mut a = c5(opts5(
        "local-dev",
        tls,
        Some(tls_cfg(&roots, Some(local))),
        None,
    ))
    .await
    .unwrap();
    m.set_anonymous(true).unwrap();
    all_open(&mut [&mut plain, &mut a], "Anonymous on").await;
    running(&m, started_at);
    m.stop().await;
}

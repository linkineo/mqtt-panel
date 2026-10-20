//! T-28 to T-30 (change set 11): an external client using only what the broker hands out (CA.pem, issued certificates
//! as PEM or PKCS#12) against a real [`BrokerManager`] on free ports, with a temporary folder and in-memory secrets.
//! The outsider is the app's own client crate, configured exactly like a third-party device: no pinned fingerprint,
//! no skipped verification, the CA file as its only trust anchor besides the system roots.
//!
//! Host names: "by host name" connects to 127.0.0.1 and checks the name given as SNI (a hosts-file override, so the
//! test does not depend on the local DNS); "by LAN address" connects to the first usable IPv4 interface address and is
//! skipped, with a note, on a machine without one.

use mqttpanel_broker::config::*;
use mqttpanel_broker::BrokerManager;
use mqttpanel_client::{connect, ClientEvent, ConnectFailure, ConnectOptions, Outgoing, Scheme, VersionPref};
use mqttpanel_profiles::MemorySecrets;
use mqttpanel_tls::kit::KeyAlg;
use mqttpanel_tls::Identity;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const T: Duration = Duration::from_secs(5);

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Fixture {
    m: BrokerManager,
    tls: u16,
    wss: u16,
    _dir: tempfile::TempDir,
}

/// A broker reachable from the network with TLS and WSS on free ports (TCP and WS off), anonymous on.
async fn broker() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let m = BrokerManager::open(dir.path(), Arc::new(MemorySecrets::new())).unwrap();
    m.set_bind(Bind::Network).await.unwrap();
    for mut l in m.config().listeners {
        l.port = free_port();
        l.enabled = l.kind.encrypted();
        m.update_listener(l).await.unwrap();
    }
    m.set_anonymous(true).unwrap();
    let st = m.start().await;
    assert_eq!(st.state, BrokerState::Running, "{:?}", st.error);
    let port = |k: ListenerKind| m.config().listeners.iter().find(|l| l.kind == k).unwrap().port;
    let (tls, wss) = (port(ListenerKind::Tls), port(ListenerKind::Wss));
    Fixture { m, tls, wss, _dir: dir }
}

/// How the outsider reaches the broker.
#[derive(Clone, Debug)]
struct Target {
    /// Address the TCP connection goes to.
    host: String,
    /// Name checked against the certificate (hosts-file style override); None: the host itself.
    name: Option<String>,
}

fn by(host: &str) -> Target {
    Target { host: host.into(), name: None }
}

fn by_name(name: &str) -> Target {
    Target { host: "127.0.0.1".into(), name: Some(name.into()) }
}

fn opts(t: &Target, port: u16, scheme: Scheme, v: VersionPref, ca: &[u8], id: Option<Identity>, client_id: &str) -> ConnectOptions {
    let mut o = ConnectOptions::new(&t.host, port);
    o.scheme = scheme;
    o.version = v;
    o.client_id = client_id.into();
    o.reconnect = false;
    o.connect_timeout = T;
    o.tls.verify_server = true;
    o.tls.ca = Some(ca.to_vec());
    o.tls.sni = t.name.clone();
    o.tls.identity = id;
    o
}

/// Why a connection failed: "trust:<reason>:<reason text key>" for a certificate the client would prompt about, otherwise
/// "<catalog key> | <English text>".
fn why(f: ConnectFailure) -> String {
    match f {
        ConnectFailure::Trust(t) => format!("trust:{:?}:{}", t.reason, t.reason_text.key),
        ConnectFailure::Message(m) => format!("{} | {}", m.key, m.text),
    }
}

async fn next_event(rx: &mut mpsc::Receiver<ClientEvent>) -> Option<ClientEvent> {
    tokio::time::timeout(T, rx.recv()).await.ok().flatten()
}

/// Connects, subscribes with QoS 2 and publishes one message at QoS 0, 1 and 2 to itself; each comes back once with
/// its QoS. Err carries the reason a connection was refused.
async fn round_trip(o: ConnectOptions) -> Result<(), String> {
    let topic = format!("ext/{}", o.client_id);
    let (h, _info, mut rx) = connect(o).await.map_err(why)?;
    h.subscribe(vec![(format!("{topic}/#"), 2)]);
    loop {
        match next_event(&mut rx).await {
            Some(ClientEvent::Subscribed { result, .. }) => {
                assert_eq!(result.map_err(|e| e.key)?, 2, "granted QoS 2");
                break;
            }
            Some(_) => continue,
            None => return Err("no SUBACK".into()),
        }
    }
    for q in 0..=2u8 {
        h.publish(Outgoing { topic: format!("{topic}/q{q}"), payload: format!("payload {q}").into_bytes(), qos: q, retain: false, props: None });
    }
    let mut got: Vec<(String, u8)> = Vec::new();
    while got.len() < 3 {
        match next_event(&mut rx).await {
            Some(ClientEvent::Message(m)) => got.push((m.topic.clone(), m.qos)),
            Some(ClientEvent::Published { result, .. }) => {
                result.map_err(|e| e.key)?;
            }
            Some(ClientEvent::Lost(m)) => return Err(m.key),
            Some(_) => {}
            None => return Err(format!("only {} of 3 messages", got.len())),
        }
    }
    got.sort();
    assert_eq!(got, (0..=2u8).map(|q| (format!("{topic}/q{q}"), q)).collect::<Vec<_>>());
    h.disconnect().await;
    Ok(())
}

/// Connects and stays connected (for the revocation test); returns the event stream.
async fn session(o: ConnectOptions) -> Result<(mqttpanel_client::ClientHandle, mpsc::Receiver<ClientEvent>), String> {
    let (h, _info, rx) = connect(o).await.map_err(why)?;
    Ok((h, rx))
}

fn ca_file(m: &BrokerManager) -> Vec<u8> {
    let (name, bytes) = m.export_ca();
    assert_eq!(name, "CA.pem");
    bytes
}

/// The host name of this computer and its .local name, as the certificate carries them.
fn host_names() -> Vec<String> {
    mqttpanel_tls::kit::local_sans().into_iter().skip(3).filter(|s| s.parse::<std::net::IpAddr>().is_err()).collect()
}

fn lan_v4() -> Option<String> {
    mqttpanel_tls::kit::interface_addresses().into_iter().find(|ip| ip.is_ipv4()).map(|ip| ip.to_string())
}

const SCHEMES: [(Scheme, &str); 2] = [(Scheme::Mqtts, "tls"), (Scheme::Wss, "wss")];
const VERSIONS: [(VersionPref, &str); 2] = [(VersionPref::V311, "311"), (VersionPref::V5, "5")];

fn port_for(f: &Fixture, s: Scheme) -> u16 {
    if s == Scheme::Wss {
        f.wss
    } else {
        f.tls
    }
}

/// T-28: TLS and WSS by localhost, by the host name and by a LAN address with CA.pem only; MQTT 3.1.1 and 5; QoS 0, 1
/// and 2 round trips. A wrong CA and a wrong host name are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t28_external_client_with_ca_pem_only() {
    let f = broker().await;
    let ca = ca_file(&f.m);
    let mut targets = vec![("localhost", by("localhost")), ("127.0.0.1", by("127.0.0.1"))];
    let names = host_names();
    assert!(names.iter().any(|n| n.ends_with(".local")), "a .local name in {names:?}");
    for n in &names {
        targets.push((if n.ends_with(".local") { ".local name" } else { "host name" }, by_name(n)));
    }
    match lan_v4() {
        Some(ip) => targets.push(("LAN address", by(&ip))),
        None => eprintln!("T-28: no usable IPv4 interface address on this machine; the LAN case is skipped"),
    }
    let mut n = 0;
    for (label, t) in &targets {
        for (scheme, sl) in SCHEMES {
            for (v, vl) in VERSIONS {
                let id = format!("t28-{sl}-{vl}-{n}");
                n += 1;
                round_trip(opts(t, port_for(&f, scheme), scheme, v, &ca, None, &id))
                    .await
                    .unwrap_or_else(|e| panic!("{label} {sl} MQTT {vl}: {e}"));
            }
        }
    }
    assert!(n >= 8);

    // Refusals: a CA the broker did not hand out, a name the certificate does not carry.
    let other = mqttpanel_tls::kit::Ca::new_root("Unrelated CA", KeyAlg::Ecdsa, mqttpanel_tls::kit::Validity::years(1)).unwrap();
    for (scheme, sl) in SCHEMES {
        let e = round_trip(opts(&by("localhost"), port_for(&f, scheme), scheme, VersionPref::V5, other.cert_pem.as_bytes(), None, "t28-wrong-ca")).await.unwrap_err();
        assert!(e.starts_with("trust:"), "{sl} wrong CA refused: {e}");
        let e = round_trip(opts(&by_name("not-this-broker.example"), port_for(&f, scheme), scheme, VersionPref::V5, &ca, None, "t28-wrong-name")).await.unwrap_err();
        assert!(e.starts_with("tls.wrongName "), "{sl} wrong name refused: {e}");
    }
    f.m.stop().await;
}

fn issue(m: &BrokerManager, name: &str, format: CertFormat, passphrase: &str) -> Vec<(String, Vec<u8>)> {
    m.issue_client_cert(IssueCertRequest { name: name.into(), valid_years: 2, key_type: KeyAlg::Ecdsa, format, passphrase: passphrase.into(), save_ca: true }).unwrap()
}

fn file<'a>(files: &'a [(String, Vec<u8>)], suffix: &str) -> &'a [u8] {
    &files.iter().find(|(n, _)| n.ends_with(suffix)).unwrap_or_else(|| panic!("no {suffix} in {:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>())).1
}

/// The identity of an issued certificate as a device loads it: PEM certificate and key, or the PKCS#12 bundle.
fn pem_identity(files: &[(String, Vec<u8>)]) -> Identity {
    mqttpanel_tls::load_identity(file(files, ".pem"), Some(file(files, ".key")), None).unwrap()
}

fn p12_identity(files: &[(String, Vec<u8>)], pass: &str) -> Identity {
    mqttpanel_tls::parse_pkcs12(file(files, ".p12"), Some(pass)).unwrap()
}

/// Waits until the session is dropped by the broker.
async fn dropped(rx: &mut mpsc::Receiver<ClientEvent>) -> bool {
    let deadline = tokio::time::Instant::now() + T;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ClientEvent::Lost(_) | ClientEvent::Failed(_) | ClientEvent::Closed)) | Ok(None) => return true,
            Ok(Some(_)) => continue,
            Err(_) => return false,
        }
    }
    false
}

/// T-29: client certificates issued by the broker (PEM, and PKCS#12 with a passphrase) with "Required" on; no
/// certificate is refused; after revocation the next handshake is refused and the open session is dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t29_client_certificates_required_and_revoked() {
    let f = broker().await;
    f.m.set_anonymous(false).unwrap();
    f.m.set_require_client_cert(true).unwrap();
    let pem = issue(&f.m, "device-pem", CertFormat::Pem, "");
    let p12 = issue(&f.m, "device-p12", CertFormat::Pkcs12, "p12-pass-29");
    let ca = file(&pem, "CA.pem").to_vec();
    assert_eq!(ca, ca_file(&f.m), "the CA.pem saved with the certificate is the exported one");
    // A PKCS#12 with a passphrase does not open without it.
    assert!(mqttpanel_tls::parse_pkcs12(file(&p12, ".p12"), Some("wrong")).is_err());
    for (scheme, sl) in SCHEMES {
        for (v, vl) in VERSIONS {
            let port = port_for(&f, scheme);
            round_trip(opts(&by("localhost"), port, scheme, v, &ca, Some(pem_identity(&pem)), &format!("t29-pem-{sl}-{vl}"))).await.unwrap_or_else(|e| panic!("PEM {sl} {vl}: {e}"));
            round_trip(opts(&by("localhost"), port, scheme, v, &ca, Some(p12_identity(&p12, "p12-pass-29")), &format!("t29-p12-{sl}-{vl}"))).await.unwrap_or_else(|e| panic!("PKCS#12 {sl} {vl}: {e}"));
        }
        let e = round_trip(opts(&by("localhost"), port_for(&f, scheme), scheme, VersionPref::V5, &ca, None, &format!("t29-none-{sl}"))).await.unwrap_err();
        assert!(e.starts_with("tls.") || e.starts_with("error."), "{sl} without a certificate refused: {e}");
    }
    let names: Vec<String> = f.m.clients().into_iter().map(|c| c.client_id).collect();
    assert!(names.is_empty(), "every round trip disconnected: {names:?}");

    // Revocation: the open sessions (TLS and WSS) are dropped, the next handshake is refused.
    let (_h1, mut rx1) = session(opts(&by("localhost"), f.tls, Scheme::Mqtts, VersionPref::V5, &ca, Some(pem_identity(&pem)), "t29-open-tls")).await.unwrap();
    let (_h2, mut rx2) = session(opts(&by("localhost"), f.wss, Scheme::Wss, VersionPref::V311, &ca, Some(pem_identity(&pem)), "t29-open-wss")).await.unwrap();
    let (_h3, mut rx3) = session(opts(&by("localhost"), f.tls, Scheme::Mqtts, VersionPref::V5, &ca, Some(p12_identity(&p12, "p12-pass-29")), "t29-keep")).await.unwrap();
    f.m.revoke_client_cert("device-pem").unwrap();
    assert!(dropped(&mut rx1).await, "TLS session of the revoked certificate dropped");
    assert!(dropped(&mut rx2).await, "WSS session of the revoked certificate dropped");
    assert!(!dropped_within(&mut rx3, Duration::from_millis(500)).await, "another device stays connected");
    for (scheme, sl) in SCHEMES {
        let e = round_trip(opts(&by("localhost"), port_for(&f, scheme), scheme, VersionPref::V5, &ca, Some(pem_identity(&pem)), &format!("t29-revoked-{sl}"))).await.unwrap_err();
        assert!(e.starts_with("tls.") || e.starts_with("error."), "{sl} revoked certificate refused: {e}");
    }
    f.m.stop().await;
}

async fn dropped_within(rx: &mut mpsc::Receiver<ClientEvent>, d: Duration) -> bool {
    match tokio::time::timeout(d, async {
        loop {
            match rx.recv().await {
                Some(ClientEvent::Lost(_) | ClientEvent::Failed(_) | ClientEvent::Closed) | None => return true,
                Some(_) => continue,
            }
        }
    })
    .await
    {
        Ok(v) => v,
        Err(_) => false,
    }
}

/// T-30: after a CA rotation the old client certificate and the old CA.pem are refused, a newly issued certificate is
/// accepted and the re-exported CA.pem verifies the server; after a regeneration with an extra name, a client
/// connecting by that name verifies the certificate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t30_ca_rotation_and_extra_names() {
    let f = broker().await;
    f.m.set_anonymous(false).unwrap();
    f.m.set_require_client_cert(true).unwrap();
    let old_ca = ca_file(&f.m);
    let old = issue(&f.m, "device-old", CertFormat::Pkcs12, "old-pass");
    round_trip(opts(&by("localhost"), f.tls, Scheme::Mqtts, VersionPref::V5, &old_ca, Some(p12_identity(&old, "old-pass")), "t30-before")).await.unwrap();

    f.m.rotate_ca().unwrap();
    let new_ca = ca_file(&f.m);
    assert_ne!(new_ca, old_ca);
    for (scheme, sl) in SCHEMES {
        let port = port_for(&f, scheme);
        // The old CA.pem no longer verifies the server.
        let e = round_trip(opts(&by("localhost"), port, scheme, VersionPref::V5, &old_ca, Some(p12_identity(&old, "old-pass")), &format!("t30-oldca-{sl}"))).await.unwrap_err();
        // Found by this test (change set 11): it used to be "certificate not accepted"; the prompt now says the CA changed.
        assert_eq!(e, "trust:UnknownCa:tls.reason.caChanged", "{sl} old CA.pem");
        // The old client certificate is refused even with the new CA.pem.
        let e = round_trip(opts(&by("localhost"), port, scheme, VersionPref::V5, &new_ca, Some(p12_identity(&old, "old-pass")), &format!("t30-oldcert-{sl}"))).await.unwrap_err();
        assert!(e.starts_with("tls.") || e.starts_with("error."), "{sl} old certificate refused: {e}");
    }
    let fresh = issue(&f.m, "device-new", CertFormat::Pem, "");
    assert_eq!(file(&fresh, "CA.pem"), new_ca.as_slice());
    for (scheme, sl) in SCHEMES {
        round_trip(opts(&by("localhost"), port_for(&f, scheme), scheme, VersionPref::V5, &new_ca, Some(pem_identity(&fresh)), &format!("t30-new-{sl}"))).await.unwrap_or_else(|e| panic!("{sl} new certificate: {e}"));
    }

    // Extra names: refused before, verified after the regeneration (a DNS name and a WAN-style address).
    let wan_name = "mqtt.plant.example";
    let e = round_trip(opts(&by_name(wan_name), f.tls, Scheme::Mqtts, VersionPref::V5, &new_ca, Some(pem_identity(&fresh)), "t30-extra-before")).await.unwrap_err();
    assert!(e.starts_with("tls.wrongName "), "{e}");
    let certs = f.m.regenerate_with_extra(&[wan_name.to_string(), "203.0.113.30".to_string()], KeyAlg::Ecdsa).unwrap();
    assert!(certs.server.san.contains(&wan_name.to_string()));
    let ca_after = ca_file(&f.m);
    assert_eq!(ca_after, new_ca, "a regeneration keeps the CA");
    for (scheme, sl) in SCHEMES {
        for (v, vl) in VERSIONS {
            round_trip(opts(&by_name(wan_name), port_for(&f, scheme), scheme, v, &ca_after, Some(pem_identity(&fresh)), &format!("t30-extra-{sl}-{vl}")))
                .await
                .unwrap_or_else(|e| panic!("{sl} {vl} by the extra name: {e}"));
        }
        round_trip(opts(&by_name("203.0.113.30"), port_for(&f, scheme), scheme, VersionPref::V5, &ca_after, Some(pem_identity(&fresh)), &format!("t30-extra-ip-{sl}")))
            .await
            .unwrap_or_else(|e| panic!("{sl} by the extra address: {e}"));
        // The automatic names are still there.
        round_trip(opts(&by("localhost"), port_for(&f, scheme), scheme, VersionPref::V5, &ca_after, Some(pem_identity(&fresh)), &format!("t30-local-{sl}"))).await.unwrap();
    }
    if let Some(ip) = lan_v4() {
        round_trip(opts(&by(&ip), f.tls, Scheme::Mqtts, VersionPref::V5, &ca_after, Some(pem_identity(&fresh)), "t30-lan")).await.unwrap_or_else(|e| panic!("LAN {ip}: {e}"));
    }
    f.m.stop().await;
}

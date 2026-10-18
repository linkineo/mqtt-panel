//! TESTPLAN section 2, the certificate matrix, against the fake TLS broker built from the F-01 kit.

mod common;
use common::*;
use mqttpanel_client::*;
use mqttpanel_tls::{load_identity, parse_pkcs12};

fn server(chain: Vec<Vec<u8>>, key: Vec<u8>) -> ServerTls {
    ServerTls::new(chain, key)
}

fn with_ca(mut o: ConnectOptions) -> ConnectOptions {
    o.tls.ca = Some(kit().root.cert_pem.clone().into_bytes());
    o
}

fn msg_of(r: Result<TestReport, ConnectFailure>) -> UserMessage {
    match r {
        Err(ConnectFailure::Message(m)) => m,
        Err(ConnectFailure::Trust(t)) => panic!("unexpected trust prompt {:?}", t.reason),
        Ok(_) => panic!("unexpected success"),
    }
}

fn trust_of(r: Result<TestReport, ConnectFailure>) -> TrustInfo {
    match r {
        Err(ConnectFailure::Trust(t)) => *t,
        Err(ConnectFailure::Message(m)) => panic!("unexpected message {}", m.text),
        Ok(_) => panic!("unexpected success"),
    }
}

#[tokio::test]
async fn t01_root_ca() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server.cert_der.clone()], k.server.key_der.clone())), ..Default::default() }).await;
    let r = test_connection(&with_ca(tls_opts(b.port))).await.unwrap();
    let cert = r.info.transport.server_cert.unwrap();
    assert!(cert.issuer.contains("MQTT Panel Test Root CA"));
    assert_eq!(r.info.transport.verified, Some(true));
}

#[tokio::test]
async fn t02_intermediate_sent_by_server() {
    let k = kit();
    let chain = vec![k.server_from_intermediate.cert_der.clone(), k.intermediate.cert_der.clone()];
    let b = start(FakeConfig { tls: Some(server(chain, k.server_from_intermediate.key_der.clone())), ..Default::default() }).await;
    test_connection(&with_ca(tls_opts(b.port))).await.unwrap();
}

#[tokio::test]
async fn t03_intermediate_missing() {
    // CONN-001: an unknown issuer with a configured CA is a trust prompt, never a hard failure; the reason text
    // still mentions the possibly missing intermediate.
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_from_intermediate.cert_der.clone()], k.server_from_intermediate.key_der.clone())), ..Default::default() }).await;
    let t = trust_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(t.reason, TrustReason::UnknownCa);
    assert!(!t.ca_offer);
    assert_eq!(t.reason_text.key, "tls.reason.unknownCaMaybeIntermediate");
    assert_eq!(
        t.reason_text.text,
        "Reason: the certificate is signed by CN=MQTT Panel Test Intermediate CA, which is not the CA set in this profile. The broker may also have left out its intermediate certificate. Compare the fingerprint with the one shown on the broker before trusting it."
    );
}

#[tokio::test]
async fn t03b_leaf_from_other_ca_than_configured_is_trust_prompt() {
    // CONN-001: a leaf signed directly by a CA other than the configured one (single-cert chain) offers the trust
    // prompt, and pinning its fingerprint then connects.
    let k = kit();
    let leaf = k.other_ca.issue("localhost", &mqttpanel_tls::kit::local_sans(), mqttpanel_tls::kit::Usage::Server, mqttpanel_tls::kit::KeyAlg::Ecdsa, mqttpanel_tls::kit::Validity::years(1)).unwrap();
    let b = start(FakeConfig { tls: Some(server(vec![leaf.cert_der.clone()], leaf.key_der.clone())), ..Default::default() }).await;
    let r = test_connection(&with_ca(tls_opts(b.port))).await;
    assert!(!matches!(r, Err(ConnectFailure::Message(_))), "an unknown issuer must not be a hard failure");
    let t = trust_of(r);
    assert_eq!(t.reason, TrustReason::UnknownCa);
    assert_eq!(t.leaf.fingerprint, mqttpanel_tls::fingerprint_sha256(&leaf.cert_der));
    let mut o = with_ca(tls_opts(b.port));
    o.tls.trusted_fingerprints = vec![t.leaf.fingerprint.clone()];
    test_connection(&o).await.unwrap();
}

#[tokio::test]
async fn t04_self_signed_trust_prompt_then_trusted() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.self_signed.cert_der.clone()], k.self_signed.key_der.clone())), ..Default::default() }).await;
    let t = trust_of(test_connection(&tls_opts(b.port)).await);
    assert_eq!(t.reason, TrustReason::SelfSigned);
    assert!(!t.ca_offer);
    assert!(t.leaf.san.contains(&"localhost".to_string()));
    assert_eq!(t.leaf.fingerprint, mqttpanel_tls::fingerprint_sha256(&k.self_signed.cert_der));
    assert!(t.reason_text.text.starts_with("Reason: the certificate is self-signed."));
    let mut o = tls_opts(b.port);
    o.tls.trusted_fingerprints = vec![t.leaf.fingerprint.clone()];
    let r = test_connection(&o).await.unwrap();
    assert_eq!(r.info.transport.verified, Some(false));
}

#[tokio::test]
async fn t05_fingerprint_changed() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.self_signed.cert_der.clone()], k.self_signed.key_der.clone())), ..Default::default() }).await;
    let mut o = tls_opts(b.port);
    let old = "AA:BB:CC".to_string();
    o.tls.trusted_fingerprints = vec![old.clone()];
    let t = trust_of(test_connection(&o).await);
    assert_eq!(t.reason, TrustReason::FingerprintChanged);
    assert_eq!(t.previous_fingerprint.as_deref(), Some(old.as_str()));
    assert!(t.reason_text.text.contains(&t.leaf.fingerprint));
}

#[tokio::test]
async fn t06_private_ca_offer_and_renewal() {
    let k = kit();
    // The server sends its chain up to the private root.
    let chain = vec![k.server.cert_der.clone(), k.root.cert_der.clone()];
    let b = start(FakeConfig { tls: Some(server(chain, k.server.key_der.clone())), ..Default::default() }).await;
    let t = trust_of(test_connection(&tls_opts(b.port)).await);
    assert_eq!(t.reason, TrustReason::UnknownCa);
    assert!(t.ca_offer);
    let ca = t.ca_der.clone().unwrap();
    // "Trust anything this CA signs" fills the CA field: reconnect works.
    let mut o = tls_opts(b.port);
    o.tls.ca = Some(ca.clone());
    test_connection(&o).await.unwrap();
    // A renewed server certificate from the same CA connects without a prompt.
    let renewed = k.root.issue("localhost", &mqttpanel_tls::kit::local_sans(), mqttpanel_tls::kit::Usage::Server, mqttpanel_tls::kit::KeyAlg::Ecdsa, mqttpanel_tls::kit::Validity::years(1)).unwrap();
    let b2 = start(FakeConfig { tls: Some(server(vec![renewed.cert_der.clone()], renewed.key_der.clone())), ..Default::default() }).await;
    let mut o2 = tls_opts(b2.port);
    o2.tls.ca = Some(ca);
    test_connection(&o2).await.unwrap();
}

#[tokio::test]
async fn t07_expired() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_expired.cert_der.clone()], k.server_expired.key_der.clone())), ..Default::default() }).await;
    let t = trust_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(t.reason, TrustReason::Expired);
    assert!(t.unsafe_);
    assert_eq!(t.reason_text.text, format!("The broker's certificate expired on {}.", t.leaf.not_after));
}

#[tokio::test]
async fn t08_not_yet_valid() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_not_yet_valid.cert_der.clone()], k.server_not_yet_valid.key_der.clone())), ..Default::default() }).await;
    let t = trust_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(t.reason, TrustReason::NotYetValid);
    assert_eq!(t.reason_text.text, format!("The broker's certificate is not valid before {}. Check the clock on the broker or this computer.", t.leaf.not_before));
}

#[tokio::test]
async fn t09_wrong_name() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_wrong_name.cert_der.clone()], k.server_wrong_name.key_der.clone())), ..Default::default() }).await;
    let m = msg_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(m.text, "The certificate is for other.example, not localhost. If you reach the broker through an alias, set Server name (SNI).");
}

#[tokio::test]
async fn t10_sni_override_with_ip_host() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_broker_lab.cert_der.clone()], k.server_broker_lab.key_der.clone())), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.host = "127.0.0.1".into();
    o.tls.sni = Some("broker.lab".into());
    test_connection(&o).await.unwrap();
}

fn mtls_server(revoked: Vec<String>) -> ServerTls {
    let k = kit();
    let mut s = server(vec![k.server.cert_der.clone()], k.server.key_der.clone());
    s.client_ca = Some(k.root.cert_pem.clone().into_bytes());
    s.require_client_cert = true;
    s.revoked_serials = revoked;
    s
}

#[tokio::test]
async fn t11_client_cert_required_none_set() {
    let b = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let m = msg_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(m.text, "The broker requires a client certificate and none is set in this profile.");
}

#[tokio::test]
async fn t12_client_cert_ok() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(k.client_ok.cert_pem.as_bytes(), Some(k.client_ok.key_pem.as_bytes()), None).unwrap());
    test_connection(&o).await.unwrap();
    assert!(b.connects.lock().unwrap().iter().any(|c| c == "cert:client-ok"));
}

#[tokio::test]
async fn t13_client_cert_other_ca() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(k.client_other_ca.cert_pem.as_bytes(), Some(k.client_other_ca.key_pem.as_bytes()), None).unwrap());
    let m = msg_of(test_connection(&o).await);
    assert_eq!(m.text, "The broker rejected the client certificate (not issued by a CA it trusts).");
}

#[tokio::test]
async fn t14_client_cert_revoked() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(mtls_server(vec![k.client_revoked.serial.clone()])), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(k.client_revoked.cert_pem.as_bytes(), Some(k.client_revoked.key_pem.as_bytes()), None).unwrap());
    let m = msg_of(test_connection(&o).await);
    assert_eq!(m.text, "The broker rejected the client certificate client-revoked (revoked).");
}

#[tokio::test]
async fn t15_pkcs12_with_passphrase() {
    let k = kit();
    assert_eq!(parse_pkcs12(&k.pkcs12_with_pass, Some("bad")).unwrap_err().catalog_key(), "tls.wrongPassphrase");
    let b = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(&k.pkcs12_with_pass, None, Some(TEST_PASSPHRASE)).unwrap());
    test_connection(&o).await.unwrap();
    let m = UserMessage::new("tls.wrongPassphrase", &[("file", serde_json::json!("client.p12"))]);
    assert_eq!(m.text, "The passphrase for client.p12 is wrong.");
}

#[tokio::test]
async fn t16_pkcs12_without_passphrase() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(&k.pkcs12_no_pass, None, None).unwrap());
    test_connection(&o).await.unwrap();
}

#[tokio::test]
async fn t17_rsa_server_ecdsa_client_and_reverse() {
    let k = kit();
    let mut s = server(vec![k.server_rsa.cert_der.clone()], k.server_rsa.key_der.clone());
    s.client_ca = Some(k.root.cert_pem.clone().into_bytes());
    s.require_client_cert = true;
    let b = start(FakeConfig { tls: Some(s), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.tls.identity = Some(load_identity(k.client_ok.cert_pem.as_bytes(), Some(k.client_ok.key_pem.as_bytes()), None).unwrap());
    test_connection(&o).await.unwrap();
    let b2 = start(FakeConfig { tls: Some(mtls_server(vec![])), ..Default::default() }).await;
    let mut o2 = with_ca(tls_opts(b2.port));
    o2.tls.identity = Some(load_identity(k.client_rsa.cert_pem.as_bytes(), Some(k.client_rsa.key_pem.as_bytes()), None).unwrap());
    test_connection(&o2).await.unwrap();
}

#[tokio::test]
async fn t18_tls12_only_server() {
    let k = kit();
    let mut s = server(vec![k.server.cert_der.clone()], k.server.key_der.clone());
    s.tls12_only = true;
    let b = start(FakeConfig { tls: Some(s), ..Default::default() }).await;
    let r = test_connection(&with_ca(tls_opts(b.port))).await.unwrap();
    assert_eq!(r.info.transport.tls_version.as_deref(), Some("TLS 1.2"));
}

#[tokio::test]
async fn t19_alpn_required() {
    let k = kit();
    let mut s = server(vec![k.server.cert_der.clone()], k.server.key_der.clone());
    s.require_alpn = Some("mqtt".into());
    let b = start(FakeConfig { tls: Some(s), ..Default::default() }).await;
    let m = msg_of(test_connection(&with_ca(tls_opts(b.port))).await);
    assert_eq!(m.text, "The broker closed the TLS handshake; it may require a Protocol tag (ALPN) such as mqtt.");
    let mut o = with_ca(tls_opts(b.port));
    o.tls.alpn = Some("mqtt".into());
    test_connection(&o).await.unwrap();
}

#[tokio::test]
async fn t20_skip_verification() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server_expired.cert_der.clone()], k.server_expired.key_der.clone())), ..Default::default() }).await;
    let mut o = tls_opts(b.port);
    o.tls.skip_verify = true;
    let r = test_connection(&o).await.unwrap();
    assert_eq!(r.info.transport.verified, Some(false));
}

#[tokio::test]
async fn c04_wss_with_path() {
    let k = kit();
    let b = start(FakeConfig { tls: Some(server(vec![k.server.cert_der.clone()], k.server.key_der.clone())), ws_path: Some("/mqtt".into()), ..Default::default() }).await;
    let mut o = with_ca(tls_opts(b.port));
    o.scheme = Scheme::Wss;
    test_connection(&o).await.unwrap();
}

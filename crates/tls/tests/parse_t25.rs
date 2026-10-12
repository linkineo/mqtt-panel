//! T-25: certificate and key parsing, each file parsed or refused with a specific reason.

use mqttpanel_tls::kit::{self, KeyAlg, TestKit, Validity, TEST_PASSPHRASE};
use mqttpanel_tls::*;
use std::sync::OnceLock;

fn kit() -> &'static TestKit {
    static KIT: OnceLock<TestKit> = OnceLock::new();
    KIT.get_or_init(|| TestKit::generate().expect("kit"))
}

#[test]
fn t25_pem_with_crlf() {
    let pem = kit().server.cert_pem.replace('\n', "\r\n");
    let certs = parse_certs(pem.as_bytes()).unwrap();
    assert_eq!(certs.len(), 1);
}

#[test]
fn t25_pem_bundle_several_certs() {
    let bundle = format!("{}{}", kit().server_from_intermediate.cert_pem, kit().intermediate.cert_pem);
    let certs = parse_certs(bundle.as_bytes()).unwrap();
    assert_eq!(certs.len(), 2);
    let s = inspect(bundle.as_bytes(), FileExpect::Certificate, None).unwrap();
    assert_eq!(s.kind, "bundle");
    assert_eq!(s.count, 2);
}

#[test]
fn t25_der_certificate() {
    let s = inspect(&kit().root.cert_der, FileExpect::Certificate, None).unwrap();
    assert_eq!(s.kind, "certificate");
    assert!(s.is_ca);
    assert!(s.self_signed);
    assert_eq!(s.key_type.as_deref(), Some("ECDSA P-256"));
}

#[test]
fn t25_pkcs8_and_traditional_keys() {
    // PKCS#8 PEM
    let k = parse_private_key(kit().client_ok.key_pem.as_bytes(), None).unwrap();
    assert!(matches!(k, rustls_pki_types::PrivateKeyDer::Pkcs8(_)));
    // PKCS#8 DER
    assert!(parse_private_key(&kit().client_ok.key_der, None).is_ok());
    // Traditional SEC1 "EC PRIVATE KEY" PEM, built from the PKCS#8 inner key
    let info = pkcs8_inner(&kit().client_ok.key_der);
    let sec1 = to_pem("EC PRIVATE KEY", &info);
    let k = parse_private_key(sec1.as_bytes(), None).unwrap();
    assert!(matches!(k, rustls_pki_types::PrivateKeyDer::Sec1(_)));
    // RSA key summary
    let s = inspect(kit().client_rsa.key_pem.as_bytes(), FileExpect::KeyOrPkcs12, None).unwrap();
    assert_eq!(s.key_type.as_deref(), Some("RSA"));
}

fn pkcs8_inner(der: &[u8]) -> Vec<u8> {
    // PrivateKeyInfo.privateKey OCTET STRING content is the SEC1 ECPrivateKey for EC keys.
    use x509_parser::der_parser::ber::parse_ber_sequence;
    let (_, seq) = parse_ber_sequence(der).unwrap();
    let items = seq.as_sequence().unwrap();
    items[2].as_slice().unwrap().to_vec()
}

#[test]
fn t25_encrypted_pkcs8_key() {
    let enc = kit::key_pem_with_passphrase(&kit().client_ok.key_der, "s3cret").unwrap();
    assert!(enc.contains("ENCRYPTED PRIVATE KEY"));
    assert_eq!(parse_private_key(enc.as_bytes(), None).unwrap_err(), TlsFileError::NeedsPassphrase);
    assert_eq!(parse_private_key(enc.as_bytes(), Some("wrong")).unwrap_err(), TlsFileError::WrongPassphrase);
    assert!(parse_private_key(enc.as_bytes(), Some("s3cret")).is_ok());
    let s = inspect(enc.as_bytes(), FileExpect::KeyOrPkcs12, Some("s3cret")).unwrap();
    assert!(s.encrypted);
}

#[test]
fn t25_traditional_encrypted_key_refused_with_reason() {
    let pem = "-----BEGIN EC PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00112233445566778899AABBCCDDEEFF\n\nAAAA\n-----END EC PRIVATE KEY-----\n";
    match parse_private_key(pem.as_bytes(), Some("x")) {
        Err(TlsFileError::UnsupportedEncryption(reason)) => assert!(reason.contains("PKCS#8")),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn t25_garbage_refused() {
    assert_eq!(parse_certs(b"hello world").unwrap_err(), TlsFileError::NotCertificate);
    assert_eq!(inspect(b"just text\n", FileExpect::Certificate, None).unwrap_err().catalog_key(), "file.notCertificate");
    assert_eq!(parse_private_key(b"garbage", None).unwrap_err(), TlsFileError::NotKey);
    // A certificate in the key field is not a key
    assert_eq!(inspect(kit().server.cert_pem.as_bytes(), FileExpect::KeyOrPkcs12, None).unwrap_err(), TlsFileError::NotKey);
}

#[test]
fn t25_pkcs12_with_and_without_passphrase() {
    let k = kit();
    assert!(looks_like_pkcs12(&k.pkcs12_with_pass));
    assert_eq!(parse_pkcs12(&k.pkcs12_with_pass, None).unwrap_err(), TlsFileError::NeedsPassphrase);
    assert_eq!(parse_pkcs12(&k.pkcs12_with_pass, Some("nope")).unwrap_err(), TlsFileError::WrongPassphrase);
    let id = parse_pkcs12(&k.pkcs12_with_pass, Some(TEST_PASSPHRASE)).unwrap();
    assert_eq!(id.certs.len(), 2);
    let d = cert_details(id.certs[0].as_ref()).unwrap();
    assert_eq!(d.cn.as_deref(), Some("client-ok"));
    // T-16: no passphrase imports without a prompt
    assert!(parse_pkcs12(&k.pkcs12_no_pass, None).is_ok());
}

#[test]
fn cert_details_fields() {
    let d = cert_details(&kit().server.cert_der).unwrap();
    assert!(d.san.iter().any(|s| s == "localhost"));
    assert!(d.san.iter().any(|s| s == "127.0.0.1"));
    assert!(d.san.iter().any(|s| s == "::1"));
    assert_eq!(d.fingerprint.len(), 32 * 3 - 1);
    assert!(!d.self_signed);
    assert!(d.issuer.contains("MQTT Panel Test Root CA"));
    let ss = cert_details(&kit().self_signed.cert_der).unwrap();
    assert!(ss.self_signed);
    let exp = cert_details(&kit().server_expired.cert_der).unwrap();
    assert!(exp.not_after_unix < time::OffsetDateTime::now_utc().unix_timestamp());
}

#[test]
fn csr_sign_round_trip() {
    let (csr_pem, key_der) = kit::make_csr("sensor-r206", KeyAlg::Ecdsa).unwrap();
    let issued = kit().root.sign_csr(csr_pem.as_bytes(), None, Validity::years(2)).unwrap();
    assert!(issued.key_der.is_empty());
    let d = cert_details(&issued.cert_der).unwrap();
    assert_eq!(d.cn.as_deref(), Some("sensor-r206"));
    assert!(!key_der.is_empty());
    assert!(kit().root.sign_csr(b"not a csr", None, Validity::years(1)).is_err());
}

#[test]
fn ca_reload_from_pem_issues() {
    let ca = kit::Ca::from_pem(&kit().root.cert_pem, &kit().root.key_pem).unwrap();
    let leaf = ca.issue("x", &[], kit::Usage::Client, KeyAlg::Ecdsa, Validity::years(1)).unwrap();
    let d = cert_details(&leaf.cert_der).unwrap();
    assert!(d.issuer.contains("MQTT Panel Test Root CA"));
}

#[test]
fn fingerprint_normalisation() {
    assert_eq!(normalize_fingerprint("3f9a 12-7c"), "3F:9A:12:7C");
}

//! Change set 11: interface addresses in the server names, extra names, generated client keys and CSRs (B-28, C-33).

use mqttpanel_tls::kit::*;
use std::net::IpAddr;

#[test]
fn local_sans_carry_interface_addresses_without_loopback_or_link_local() {
    let sans = local_sans();
    for n in ["localhost", "127.0.0.1", "::1"] {
        assert!(sans.contains(&n.to_string()), "{n}");
    }
    for ip in interface_addresses() {
        assert!(sans.contains(&ip.to_string()), "interface address {ip}");
        assert!(usable_lan_address(ip));
    }
    let sans_ips: Vec<IpAddr> = sans.iter().filter_map(|s| s.parse().ok()).collect();
    // Only the three fixed loopback names are loopback; nothing link-local.
    assert_eq!(sans_ips.iter().filter(|ip| ip.is_loopback()).count(), 2);
    assert!(!sans.iter().any(|s| s.starts_with("169.254.") || s.starts_with("fe80")));
    let mut dedup = sans.clone();
    dedup.dedup();
    assert_eq!(dedup.len(), sans.len());
}

#[test]
fn usable_lan_address_rules() {
    for (ip, ok) in [
        ("192.168.1.20", true),
        ("10.0.0.5", true),
        ("fd00::12", true),
        ("2001:db8::1", true),
        ("127.0.0.1", false),
        ("::1", false),
        ("169.254.3.4", false),
        ("fe80::1", false),
        ("0.0.0.0", false),
        ("224.0.0.1", false),
    ] {
        assert_eq!(usable_lan_address(ip.parse().unwrap()), ok, "{ip}");
    }
}

#[test]
fn extra_names_are_host_names_or_ip_literals() {
    assert_eq!(parse_extra_names("broker.example.com, 203.0.113.7  2001:db8::5\nMQTT.Lab").unwrap(), vec![
        "broker.example.com".to_string(),
        "203.0.113.7".into(),
        "2001:db8::5".into(),
        "mqtt.lab".into()
    ]);
    assert_eq!(parse_extra_names("").unwrap(), Vec::<String>::new());
    assert_eq!(parse_extra_names("a.b, a.b").unwrap(), vec!["a.b".to_string()]);
    for bad in ["bad_name", "-x.com", "300.1.1.1", "x..y", "http://x", "a/b"] {
        assert_eq!(parse_extra_names(&format!("ok.lab {bad}")).unwrap_err(), bad, "{bad}");
    }
    assert_eq!(valid_extra_name("*.example.com").as_deref(), Some("*.example.com"));
    let sans = server_sans(&["wan.example.com".into(), "localhost".into()]);
    assert_eq!(sans.iter().filter(|s| *s == "localhost").count(), 1);
    assert_eq!(sans.last().map(String::as_str), Some("wan.example.com"));
}

#[test]
fn generated_key_csr_and_signed_certificate_match() {
    for alg in [KeyAlg::Ecdsa, KeyAlg::Rsa] {
        let k = generate_client_key(alg).unwrap();
        assert!(k.key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(k.algorithm, alg.label());
        assert_eq!(k.fingerprint.len(), 32 * 3 - 1);
        assert_eq!(describe_key(&k.key_pem).unwrap(), (k.algorithm.clone(), k.fingerprint.clone()));
        let csr = csr_for_key(&k.key_pem, "sensor-7").unwrap();
        assert!(csr.contains("BEGIN CERTIFICATE REQUEST"));
        let ca = Ca::new_root("Test CA", KeyAlg::Ecdsa, Validity::years(1)).unwrap();
        let signed = ca.sign_csr(csr.as_bytes(), None, Validity::years(1)).unwrap();
        assert_eq!(signed.cn, "sensor-7");
        assert!(cert_matches_key(&signed.cert_der, &k.key_pem));
        let other = ca.issue("other", &[], Usage::Client, alg, Validity::years(1)).unwrap();
        assert!(!cert_matches_key(&other.cert_der, &k.key_pem));
        // The identity built from the generated key and the signed certificate loads for rustls.
        let id = mqttpanel_tls::load_identity(signed.cert_pem.as_bytes(), Some(k.key_pem.as_bytes()), None).unwrap();
        assert_eq!(id.certs.len(), 1);
    }
}

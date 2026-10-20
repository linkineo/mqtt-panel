//! Certificate generation: local CA, server and client certificates, CSR signing, PKCS#12 and encrypted PEM export.
//!
//! Defaults (SPEC 7): ECDSA P-256 keys, CA valid 5 years, server and client certificates 2 years.

use crate::parse::{cert_details, to_pem};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType, SerialNumber,
};
use rustls_pki_types::{CertificateSigningRequestDer, PrivatePkcs8KeyDer};
use std::net::IpAddr;
use time::{Duration, OffsetDateTime};

/// Errors from certificate generation.
#[derive(Debug, thiserror::Error)]
pub enum KitError {
    #[error("certificate generation failed: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("key generation failed: {0}")]
    Key(String),
    #[error("not a certificate request")]
    NotCsr,
    #[error("PKCS#12 export failed: {0}")]
    Pkcs12(String),
}

/// Key algorithm for generated keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyAlg {
    #[default]
    Ecdsa,
    Rsa,
}

impl KeyAlg {
    pub fn label(self) -> &'static str {
        match self {
            KeyAlg::Ecdsa => "ECDSA P-256",
            KeyAlg::Rsa => "RSA 2048",
        }
    }
}

/// Validity window.
#[derive(Debug, Clone, Copy)]
pub struct Validity {
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
}

impl Validity {
    /// From one hour ago (clock skew) for `days` days.
    pub fn days(days: i64) -> Self {
        let now = OffsetDateTime::now_utc();
        Validity { not_before: now - Duration::hours(1), not_after: now + Duration::days(days) }
    }
    /// `years` × 365 days (plus leap days, approximately), starting now.
    pub fn years(years: i64) -> Self {
        Self::days(years * 365 + years / 4)
    }
    /// Expired yesterday.
    pub fn expired() -> Self {
        let now = OffsetDateTime::now_utc();
        Validity { not_before: now - Duration::days(400), not_after: now - Duration::days(1) }
    }
    /// Valid from tomorrow.
    pub fn not_yet_valid() -> Self {
        let now = OffsetDateTime::now_utc();
        Validity { not_before: now + Duration::days(1), not_after: now + Duration::days(400) }
    }
}

/// What a leaf certificate is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Server,
    Client,
}

/// A generated certificate with its key (the key is empty for a signed CSR).
#[derive(Debug, Clone)]
pub struct Issued {
    pub cert_der: Vec<u8>,
    pub cert_pem: String,
    pub key_der: Vec<u8>, // PKCS#8
    pub key_pem: String,
    pub serial: String,
    pub cn: String,
}

/// A certificate authority able to issue certificates.
pub struct Ca {
    pub cert_der: Vec<u8>,
    pub cert_pem: String,
    pub key_der: Vec<u8>, // PKCS#8
    pub key_pem: String,
    pub cn: String,
    issuer: Issuer<'static, KeyPair>,
}

impl std::fmt::Debug for Ca {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ca").field("cn", &self.cn).finish()
    }
}

fn gen_key(alg: KeyAlg) -> Result<KeyPair, KitError> {
    match alg {
        KeyAlg::Ecdsa => Ok(KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?),
        KeyAlg::Rsa => {
            use rsa::pkcs8::EncodePrivateKey;
            let mut rng = rand_core_compat::OsRng;
            let key = rsa::RsaPrivateKey::new(&mut rng, 2048).map_err(|e| KitError::Key(e.to_string()))?;
            let der = key.to_pkcs8_der().map_err(|e| KitError::Key(e.to_string()))?;
            let pk = PrivatePkcs8KeyDer::from(der.as_bytes().to_vec());
            Ok(KeyPair::from_pkcs8_der_and_sign_algo(&pk, &rcgen::PKCS_RSA_SHA256)?)
        }
    }
}

/// rand 0.9's OsRng implements rand_core 0.9; the rsa crate uses rand_core 0.6. Bridge through getrandom-backed bytes.
mod rand_core_compat {
    pub struct OsRng;
    impl rsa::rand_core::RngCore for OsRng {
        fn next_u32(&mut self) -> u32 {
            rand::random()
        }
        fn next_u64(&mut self) -> u64 {
            rand::random()
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            use rand::RngCore;
            rand::rng().fill_bytes(dest)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }
    impl rsa::rand_core::CryptoRng for OsRng {}
}

fn serial() -> SerialNumber {
    let mut bytes: [u8; 16] = rand::random();
    bytes[0] &= 0x7f; // positive
    bytes[0] |= 0x01; // no leading zero byte
    SerialNumber::from_slice(&bytes)
}

fn dn(cn: &str) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    dn.push(DnType::OrganizationName, "MQTT Panel");
    dn
}

fn san_entries(sans: &[String]) -> Vec<SanType> {
    sans.iter()
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else if let Ok(ip) = s.parse::<IpAddr>() {
                Some(SanType::IpAddress(ip))
            } else {
                s.to_string().try_into().ok().map(SanType::DnsName)
            }
        })
        .collect()
}

fn issued_from(cert: rcgen::Certificate, key: Option<&KeyPair>, cn: &str) -> Issued {
    let cert_der = cert.der().to_vec();
    let serial = cert_details(&cert_der).map(|d| d.serial).unwrap_or_default();
    Issued {
        cert_pem: to_pem("CERTIFICATE", &cert_der),
        cert_der,
        key_der: key.map(|k| k.serialize_der()).unwrap_or_default(),
        key_pem: key.map(|k| k.serialize_pem()).unwrap_or_default(),
        serial,
        cn: cn.to_string(),
    }
}

fn ca_params(cn: &str, validity: Validity, path_len: Option<u8>) -> CertificateParams {
    let mut p = CertificateParams::default();
    p.distinguished_name = dn(cn);
    p.is_ca = match path_len {
        None => IsCa::Ca(BasicConstraints::Unconstrained),
        Some(n) => IsCa::Ca(BasicConstraints::Constrained(n)),
    };
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    p.not_before = validity.not_before;
    p.not_after = validity.not_after;
    p.serial_number = Some(serial());
    p.use_authority_key_identifier_extension = true;
    p
}

fn leaf_params(cn: &str, sans: &[String], usage: Usage, validity: Validity, alg: KeyAlg) -> CertificateParams {
    let mut p = CertificateParams::default();
    p.distinguished_name = dn(cn);
    p.subject_alt_names = san_entries(sans);
    p.is_ca = IsCa::ExplicitNoCa;
    p.key_usages = match alg {
        KeyAlg::Ecdsa => vec![KeyUsagePurpose::DigitalSignature],
        KeyAlg::Rsa => vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment],
    };
    p.extended_key_usages = match usage {
        Usage::Server => vec![ExtendedKeyUsagePurpose::ServerAuth],
        Usage::Client => vec![ExtendedKeyUsagePurpose::ClientAuth],
    };
    p.not_before = validity.not_before;
    p.not_after = validity.not_after;
    p.serial_number = Some(serial());
    p.use_authority_key_identifier_extension = true;
    p
}

impl Ca {
    /// A new self-signed root CA.
    pub fn new_root(cn: &str, alg: KeyAlg, validity: Validity) -> Result<Ca, KitError> {
        let key = gen_key(alg)?;
        let params = ca_params(cn, validity, None);
        let cert = params.self_signed(&key)?;
        Ok(Self::assemble(cert, params, key, cn))
    }

    /// A new intermediate CA signed by this CA.
    pub fn intermediate(&self, cn: &str, alg: KeyAlg, validity: Validity) -> Result<Ca, KitError> {
        let key = gen_key(alg)?;
        let params = ca_params(cn, validity, Some(0));
        let cert = params.signed_by(&key, &self.issuer)?;
        Ok(Self::assemble(cert, params, key, cn))
    }

    fn assemble(cert: rcgen::Certificate, params: CertificateParams, key: KeyPair, cn: &str) -> Ca {
        let cert_der = cert.der().to_vec();
        Ca {
            cert_pem: to_pem("CERTIFICATE", &cert_der),
            cert_der,
            key_der: key.serialize_der(),
            key_pem: key.serialize_pem(),
            cn: cn.to_string(),
            issuer: Issuer::new(params, key),
        }
    }

    /// Reloads a CA from its PEM certificate and PEM (PKCS#8) key, for example from the keychain.
    pub fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Ca, KitError> {
        let key = KeyPair::from_pem(key_pem)?;
        let key_der = key.serialize_der();
        let certs = crate::parse::parse_certs(cert_pem.as_bytes()).map_err(|_| KitError::Rcgen(rcgen::Error::CouldNotParseCertificate))?;
        let cert_der = certs[0].as_ref().to_vec();
        let cn = cert_details(&cert_der).ok().and_then(|d| d.cn).unwrap_or_default();
        let issuer = Issuer::from_ca_cert_der(&certs[0], key)?;
        Ok(Ca { cert_pem: to_pem("CERTIFICATE", &cert_der), cert_der, key_der, key_pem: key_pem.to_string(), cn, issuer })
    }

    /// Issues a leaf certificate with a fresh key.
    pub fn issue(&self, cn: &str, sans: &[String], usage: Usage, alg: KeyAlg, validity: Validity) -> Result<Issued, KitError> {
        let key = gen_key(alg)?;
        let cert = leaf_params(cn, sans, usage, validity, alg).signed_by(&key, &self.issuer)?;
        Ok(issued_from(cert, Some(&key), cn))
    }

    /// Signs a certificate request (PEM or DER) as a client certificate; the key stays with the requester.
    /// `cn_override` replaces the CN of the request when given.
    pub fn sign_csr(&self, csr: &[u8], cn_override: Option<&str>, validity: Validity) -> Result<Issued, KitError> {
        let der = crate::parse::parse_csr_pem(csr).ok_or(KitError::NotCsr)?;
        let mut req = CertificateSigningRequestParams::from_der(&CertificateSigningRequestDer::from(der)).map_err(|_| KitError::NotCsr)?;
        let cn = match cn_override {
            Some(cn) => cn.to_string(),
            None => req
                .params
                .distinguished_name
                .get(&DnType::CommonName)
                .map(dn_value_string)
                .unwrap_or_else(|| "device".to_string()),
        };
        let alg = if req.public_key.algorithm() == &rcgen::PKCS_RSA_SHA256 { KeyAlg::Rsa } else { KeyAlg::Ecdsa };
        let mut params = leaf_params(&cn, &[], Usage::Client, validity, alg);
        params.subject_alt_names = req.params.subject_alt_names.clone();
        req.params = params;
        let cert = req.signed_by(&self.issuer)?;
        Ok(issued_from(cert, None, &cn))
    }
}

fn dn_value_string(v: &rcgen::DnValue) -> String {
    match v {
        rcgen::DnValue::Utf8String(s) => s.clone(),
        rcgen::DnValue::PrintableString(s) => s.as_str().to_string(),
        rcgen::DnValue::Ia5String(s) => s.as_str().to_string(),
        rcgen::DnValue::TeletexString(s) => s.as_str().to_string(),
        _ => "device".to_string(),
    }
}

/// A self-signed server certificate (no CA), for tests and lab brokers.
pub fn self_signed_server(cn: &str, sans: &[String], alg: KeyAlg, validity: Validity) -> Result<Issued, KitError> {
    let key = gen_key(alg)?;
    let mut params = leaf_params(cn, sans, Usage::Server, validity, alg);
    params.use_authority_key_identifier_extension = false;
    let cert = params.self_signed(&key)?;
    Ok(issued_from(cert, Some(&key), cn))
}

/// Generates a certificate signing request (as a device would), returning (csr PEM, key PKCS#8 DER).
pub fn make_csr(cn: &str, alg: KeyAlg) -> Result<(String, Vec<u8>), KitError> {
    let key = gen_key(alg)?;
    let mut params = CertificateParams::default();
    params.distinguished_name = dn(cn);
    let csr = params.serialize_request(&key)?;
    Ok((csr.pem()?, key.serialize_der()))
}

/// A client key generated for a profile ("Generate key and request a certificate", change set 11).
#[derive(Debug, Clone)]
pub struct GeneratedKey {
    /// PKCS#8 PEM, for the secret store.
    pub key_pem: String,
    /// "ECDSA P-256" or "RSA 2048".
    pub algorithm: String,
    /// SHA-256 over the SubjectPublicKeyInfo, "3F:9A:…".
    pub fingerprint: String,
}

/// Generates a client key (ECDSA P-256 or RSA 2048).
pub fn generate_client_key(alg: KeyAlg) -> Result<GeneratedKey, KitError> {
    let key = gen_key(alg)?;
    Ok(GeneratedKey { key_pem: key.serialize_pem(), algorithm: alg.label().to_string(), fingerprint: public_key_fingerprint_of(&key) })
}

fn key_pair_from_pem(key_pem: &str) -> Result<KeyPair, KitError> {
    let der = crate::parse::parse_private_key(key_pem.as_bytes(), None).map_err(|e| KitError::Key(e.to_string()))?;
    let pkcs8 = match der {
        rustls_pki_types::PrivateKeyDer::Pkcs8(k) => k.secret_pkcs8_der().to_vec(),
        _ => return Err(KitError::Key("a PKCS#8 key is expected".into())),
    };
    let pk = PrivatePkcs8KeyDer::from(pkcs8);
    KeyPair::try_from(&pk).or_else(|_| KeyPair::from_pkcs8_der_and_sign_algo(&pk, &rcgen::PKCS_RSA_SHA256)).map_err(KitError::from)
}

fn public_key_fingerprint_of(key: &KeyPair) -> String {
    use rcgen::PublicKeyData;
    crate::parse::fingerprint_sha256(&key.subject_public_key_info())
}

/// Algorithm label and public-key fingerprint of a stored key (PKCS#8 PEM).
pub fn describe_key(key_pem: &str) -> Result<(String, String), KitError> {
    let key = key_pair_from_pem(key_pem)?;
    let alg = if key.algorithm() == &rcgen::PKCS_RSA_SHA256 { KeyAlg::Rsa } else { KeyAlg::Ecdsa };
    Ok((alg.label().to_string(), public_key_fingerprint_of(&key)))
}

/// A certificate signing request (PEM) for an existing key (PKCS#8 PEM) with the given common name.
pub fn csr_for_key(key_pem: &str, cn: &str) -> Result<String, KitError> {
    let key = key_pair_from_pem(key_pem)?;
    let mut params = CertificateParams::default();
    let mut d = DistinguishedName::new();
    d.push(DnType::CommonName, cn);
    params.distinguished_name = d;
    Ok(params.serialize_request(&key)?.pem()?)
}

/// True when the certificate (DER) carries the public key of the key (PKCS#8 PEM).
pub fn cert_matches_key(cert_der: &[u8], key_pem: &str) -> bool {
    use rcgen::PublicKeyData;
    let Ok(key) = key_pair_from_pem(key_pem) else { return false };
    match x509_parser::parse_x509_certificate(cert_der) {
        Ok((_, cert)) => cert.public_key().subject_public_key.data.as_ref() == key.der_bytes(),
        Err(_) => false,
    }
}

/// A PKCS#12 bundle with the certificate, its chain (issuer certificates) and the key.
pub fn pkcs12(cert_der: &[u8], chain: &[Vec<u8>], key_der: &[u8], passphrase: &str, alias: &str) -> Result<Vec<u8>, KitError> {
    use p12_keystore::{Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain};
    let key = PrivateKey::from_der(key_der).map_err(|e| KitError::Pkcs12(e.to_string()))?;
    let mut certs = vec![Certificate::from_der(cert_der).map_err(|e| KitError::Pkcs12(e.to_string()))?];
    for c in chain {
        certs.push(Certificate::from_der(c).map_err(|e| KitError::Pkcs12(e.to_string()))?);
    }
    let local_key_id: [u8; 20] = rand::random();
    let chain = PrivateKeyChain::new(local_key_id.to_vec(), key, certs);
    let mut store = KeyStore::new();
    store.add_entry(alias, KeyStoreEntry::PrivateKeyChain(chain));
    store.writer(passphrase).write().map_err(|e| KitError::Pkcs12(e.to_string()))
}

/// A PKCS#8 key as PEM, encrypted with the passphrase (PBES2, scrypt or PBKDF2 + AES-256-CBC) when it is not empty.
pub fn key_pem_with_passphrase(key_der: &[u8], passphrase: &str) -> Result<String, KitError> {
    if passphrase.is_empty() {
        return Ok(to_pem("PRIVATE KEY", key_der));
    }
    let info = pkcs8::PrivateKeyInfo::try_from(key_der).map_err(|e| KitError::Key(e.to_string()))?;
    let mut rng = rand_core_compat::OsRng;
    let enc = info.encrypt(&mut rng, passphrase.as_bytes()).map_err(|e| KitError::Key(e.to_string()))?;
    Ok(to_pem("ENCRYPTED PRIVATE KEY", enc.as_bytes()))
}

/// Names for a local server certificate: localhost, 127.0.0.1, ::1, the machine's host name and its .local name, and
/// every address of the network interfaces at this moment (IPv4 and IPv6; loopback, link-local and unspecified
/// addresses left out), so a client on the LAN verifies the address it connects to (change set 11).
pub fn local_sans() -> Vec<String> {
    let mut sans = vec!["localhost".to_string(), "127.0.0.1".to_string(), "::1".to_string()];
    let push = |sans: &mut Vec<String>, s: String| {
        if !s.is_empty() && !sans.contains(&s) {
            sans.push(s);
        }
    };
    if let Ok(h) = hostname::get() {
        let h = h.to_string_lossy().to_string();
        if !h.is_empty() {
            push(&mut sans, h.clone());
            // The mDNS (Bonjour) name: the first label plus ".local", also when the host name carries a domain such as
            // "name.localdomain" or "name.lan" (change set 11, found by T-28).
            if !h.ends_with(".local") {
                let first = h.split('.').next().unwrap_or(&h);
                if !first.is_empty() {
                    push(&mut sans, format!("{first}.local"));
                }
            }
        }
    }
    for ip in interface_addresses() {
        push(&mut sans, ip.to_string());
    }
    sans
}

/// Addresses of the network interfaces that a client on another machine could use: no loopback, no link-local
/// (169.254/16, fe80::/10), no unspecified or multicast address. Sorted, IPv4 first, without duplicates.
pub fn interface_addresses() -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = if_addrs::get_if_addrs()
        .map(|ifs| ifs.into_iter().map(|i| i.ip()).filter(|ip| usable_lan_address(*ip)).collect())
        .unwrap_or_default();
    out.sort_by_key(|ip| (ip.is_ipv6(), *ip));
    out.dedup();
    out
}

/// True for an address worth naming in a server certificate (see [`interface_addresses`]).
pub fn usable_lan_address(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local() && !v4.is_broadcast(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) != 0xfe80,
    }
}

/// Checks one "extra name" typed by the owner: an IP address or a DNS host name (letters, digits, hyphens, dots, an
/// optional leading "*." wildcard label). Returns the trimmed name, lower-cased for host names.
pub fn valid_extra_name(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('.');
    if s.is_empty() {
        return None;
    }
    if let Ok(ip) = s.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    if s.len() > 253 {
        return None;
    }
    let body = s.strip_prefix("*.").unwrap_or(s);
    let labels: Vec<&str> = body.split('.').collect();
    let ok = labels.iter().all(|l| {
        !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    // An all-numeric dotted name that is not a valid IP address (for example 300.1.1.1) is refused.
    let numeric = labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()));
    (ok && !numeric).then(|| s.to_ascii_lowercase())
}

/// Splits a typed list of extra names (commas, spaces or new lines). Returns the valid names, or the first invalid one.
pub fn parse_extra_names(text: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.trim().is_empty()) {
        match valid_extra_name(part) {
            Some(n) => {
                if !out.contains(&n) {
                    out.push(n)
                }
            }
            None => return Err(part.trim().to_string()),
        }
    }
    Ok(out)
}

/// The names of a generated server certificate: [`local_sans`] followed by the extra names not already in it.
pub fn server_sans(extra: &[String]) -> Vec<String> {
    let mut sans = local_sans();
    for e in extra {
        if !sans.contains(e) {
            sans.push(e.clone());
        }
    }
    sans
}

/// The F-01 local CA kit for tests: every certificate the TLS matrix needs.
pub struct TestKit {
    pub root: Ca,
    pub intermediate: Ca,
    pub other_ca: Ca,
    /// Server certificate from the root, for localhost, 127.0.0.1, ::1 and the host name.
    pub server: Issued,
    /// Server certificate from the intermediate (send chain = server + intermediate).
    pub server_from_intermediate: Issued,
    pub server_expired: Issued,
    pub server_not_yet_valid: Issued,
    pub server_wrong_name: Issued,
    /// Server certificate only valid for "broker.lab" (SNI test).
    pub server_broker_lab: Issued,
    pub server_rsa: Issued,
    pub self_signed: Issued,
    pub client_ok: Issued,
    pub client_revoked: Issued,
    pub client_other_ca: Issued,
    pub client_rsa: Issued,
    pub pkcs12_with_pass: Vec<u8>,
    pub pkcs12_no_pass: Vec<u8>,
}

/// Passphrase of [`TestKit::pkcs12_with_pass`].
pub const TEST_PASSPHRASE: &str = "test-passphrase";

impl TestKit {
    /// Generates the kit (a few hundred milliseconds; RSA keys dominate).
    pub fn generate() -> Result<TestKit, KitError> {
        let root = Ca::new_root("MQTT Panel Test Root CA", KeyAlg::Ecdsa, Validity::years(5))?;
        let intermediate = root.intermediate("MQTT Panel Test Intermediate CA", KeyAlg::Ecdsa, Validity::years(5))?;
        let other_ca = Ca::new_root("Unrelated Test CA", KeyAlg::Ecdsa, Validity::years(5))?;
        let sans = local_sans();
        let v2 = Validity::years(2);
        let server = root.issue("localhost", &sans, Usage::Server, KeyAlg::Ecdsa, v2)?;
        let server_from_intermediate = intermediate.issue("localhost", &sans, Usage::Server, KeyAlg::Ecdsa, v2)?;
        let server_expired = root.issue("localhost", &sans, Usage::Server, KeyAlg::Ecdsa, Validity::expired())?;
        let server_not_yet_valid = root.issue("localhost", &sans, Usage::Server, KeyAlg::Ecdsa, Validity::not_yet_valid())?;
        let server_wrong_name = root.issue("other.example", &["other.example".to_string()], Usage::Server, KeyAlg::Ecdsa, v2)?;
        let server_broker_lab = root.issue("broker.lab", &["broker.lab".to_string()], Usage::Server, KeyAlg::Ecdsa, v2)?;
        let server_rsa = root.issue("localhost", &sans, Usage::Server, KeyAlg::Rsa, v2)?;
        let self_signed = self_signed_server("mosquitto-lab", &sans, KeyAlg::Ecdsa, v2)?;
        let client_ok = root.issue("client-ok", &[], Usage::Client, KeyAlg::Ecdsa, v2)?;
        let client_revoked = root.issue("client-revoked", &[], Usage::Client, KeyAlg::Ecdsa, v2)?;
        let client_other_ca = other_ca.issue("client-other-ca", &[], Usage::Client, KeyAlg::Ecdsa, v2)?;
        let client_rsa = root.issue("client-rsa", &[], Usage::Client, KeyAlg::Rsa, v2)?;
        let chain = vec![root.cert_der.clone()];
        let pkcs12_with_pass = pkcs12(&client_ok.cert_der, &chain, &client_ok.key_der, TEST_PASSPHRASE, "client-ok")?;
        let pkcs12_no_pass = pkcs12(&client_ok.cert_der, &chain, &client_ok.key_der, "", "client-ok")?;
        Ok(TestKit {
            root,
            intermediate,
            other_ca,
            server,
            server_from_intermediate,
            server_expired,
            server_not_yet_valid,
            server_wrong_name,
            server_broker_lab,
            server_rsa,
            self_signed,
            client_ok,
            client_revoked,
            client_other_ca,
            client_rsa,
            pkcs12_with_pass,
            pkcs12_no_pass,
        })
    }

    /// Writes every file of the kit into `dir` (a temporary directory; never commit these).
    pub fn write_to(&self, dir: &std::path::Path) -> std::io::Result<()> {
        let w = |name: &str, data: &[u8]| std::fs::write(dir.join(name), data);
        w("root-ca.pem", self.root.cert_pem.as_bytes())?;
        w("intermediate-ca.pem", self.intermediate.cert_pem.as_bytes())?;
        w("other-ca.pem", self.other_ca.cert_pem.as_bytes())?;
        for (name, i) in [
            ("server", &self.server),
            ("server-intermediate", &self.server_from_intermediate),
            ("server-expired", &self.server_expired),
            ("server-not-yet-valid", &self.server_not_yet_valid),
            ("server-wrong-name", &self.server_wrong_name),
            ("server-broker-lab", &self.server_broker_lab),
            ("server-rsa", &self.server_rsa),
            ("self-signed", &self.self_signed),
            ("client-ok", &self.client_ok),
            ("client-revoked", &self.client_revoked),
            ("client-other-ca", &self.client_other_ca),
            ("client-rsa", &self.client_rsa),
        ] {
            w(&format!("{name}.pem"), i.cert_pem.as_bytes())?;
            w(&format!("{name}.key"), i.key_pem.as_bytes())?;
        }
        w("client.p12", &self.pkcs12_with_pass)?;
        w("client-nopass.p12", &self.pkcs12_no_pass)?;
        Ok(())
    }
}

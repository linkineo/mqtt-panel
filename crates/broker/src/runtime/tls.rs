//! TLS for the TLS and WSS listeners: server identity, client-certificate verification with live revocation,
//! and the reason of a failed handshake for the log.

use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, RootCertStore, ServerConfig, SignatureScheme};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};

/// Normalises a certificate serial ("4A:0b:..." or "4a0b...") to lower-case hex without separators or leading zeros.
pub fn normalize_serial(s: &str) -> String {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_ascii_lowercase();
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

/// Revoked certificate serials, shared with the running broker: a revocation applies at the next handshake.
#[derive(Debug, Clone, Default)]
pub struct RevocationList {
    inner: Arc<RwLock<HashSet<String>>>,
}

impl RevocationList {
    pub fn new<I: IntoIterator<Item = S>, S: AsRef<str>>(serials: I) -> Self {
        let set = serials.into_iter().map(|s| normalize_serial(s.as_ref())).collect();
        RevocationList { inner: Arc::new(RwLock::new(set)) }
    }
    pub fn revoke(&self, serial: &str) {
        if let Ok(mut g) = self.inner.write() {
            g.insert(normalize_serial(serial));
        }
    }
    pub fn is_revoked(&self, serial: &str) -> bool {
        self.inner.read().map(|g| g.contains(&normalize_serial(serial))).unwrap_or(false)
    }
    /// Replaces the whole list.
    pub fn replace<I: IntoIterator<Item = S>, S: AsRef<str>>(&self, serials: I) {
        if let Ok(mut g) = self.inner.write() {
            *g = serials.into_iter().map(|s| normalize_serial(s.as_ref())).collect();
        }
    }
}

/// Server certificate chain, key and the CA certificates that verify client certificates.
#[derive(Debug)]
pub struct TlsMaterial {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
    /// Trust anchors for client certificates (local CA or imported CA). Empty: client certificates cannot be verified.
    pub client_roots: Vec<CertificateDer<'static>>,
}

impl Clone for TlsMaterial {
    fn clone(&self) -> Self {
        TlsMaterial { chain: self.chain.clone(), key: self.key.clone_key(), client_roots: self.client_roots.clone() }
    }
}

/// Information about a verified client certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCert {
    pub cn: Option<String>,
    /// Normalised serial (see [`normalize_serial`]).
    pub serial: String,
}

fn peer_cert_of(der: &[u8]) -> Option<PeerCert> {
    let d = mqttpanel_tls::cert_details(der).ok()?;
    Some(PeerCert { cn: d.cn, serial: normalize_serial(&d.serial) })
}

/// Per-connection record filled by the verifier.
#[derive(Debug, Default)]
pub(crate) struct HandshakeSlot {
    pub verified: Option<PeerCert>,
    pub failure: Option<String>,
}

/// Client-certificate policy of one connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientAuthMode {
    /// Anonymous broker: do not ask for a certificate.
    Off,
    /// Ask; a presented certificate must verify.
    Optional,
    /// Require a valid certificate.
    Required,
}

/// Prepared TLS state of a listener: a new `ServerConfig` per connection (cheap: the key is pre-loaded) so the
/// verifier can report the client certificate and the failure reason of that very handshake.
pub(crate) struct TlsListener {
    provider: Arc<CryptoProvider>,
    resolver: Arc<FixedCert>,
    required: Option<Arc<dyn ClientCertVerifier>>,
    optional: Option<Arc<dyn ClientCertVerifier>>,
    revoked: RevocationList,
}

impl std::fmt::Debug for TlsListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TlsListener")
    }
}

#[derive(Debug)]
struct FixedCert(Arc<CertifiedKey>);

impl ResolvesServerCert for FixedCert {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

impl TlsListener {
    /// Builds the listener state; the error is a short technical reason.
    pub fn new(material: &TlsMaterial, revoked: RevocationList) -> Result<Self, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        if material.chain.is_empty() {
            return Err("no server certificate".into());
        }
        let key = CertifiedKey::from_der(material.chain.clone(), material.key.clone_key(), &provider)
            .map_err(|e| format!("server key: {e}"))?;
        let (required, optional) = if material.client_roots.is_empty() {
            (None, None)
        } else {
            let mut roots = RootCertStore::empty();
            for r in &material.client_roots {
                roots.add(r.clone()).map_err(|e| format!("client CA: {e}"))?;
            }
            let roots = Arc::new(roots);
            let req = WebPkiClientVerifier::builder_with_provider(roots.clone(), provider.clone())
                .build()
                .map_err(|e| format!("client verifier: {e}"))?;
            let opt = WebPkiClientVerifier::builder_with_provider(roots, provider.clone())
                .allow_unauthenticated()
                .build()
                .map_err(|e| format!("client verifier: {e}"))?;
            (Some(req), Some(opt))
        };
        Ok(TlsListener { provider, resolver: Arc::new(FixedCert(Arc::new(key))), required, optional, revoked })
    }

    /// A server config for one connection, with its handshake slot.
    pub fn config_for(&self, mode: ClientAuthMode) -> Result<(Arc<ServerConfig>, Arc<Mutex<HandshakeSlot>>), String> {
        let slot = Arc::new(Mutex::new(HandshakeSlot::default()));
        let inner = match mode {
            ClientAuthMode::Off => None,
            ClientAuthMode::Optional => self.optional.clone(),
            ClientAuthMode::Required => match &self.required {
                Some(v) => Some(v.clone()),
                // Required but no CA to verify against: refuse every client certificate.
                None => return Err("no CA configured to verify client certificates".into()),
            },
        };
        let verifier: Arc<dyn ClientCertVerifier> = match inner {
            None => WebPkiClientVerifier::no_client_auth(),
            Some(inner) => Arc::new(SlotVerifier { inner, slot: slot.clone(), revoked: self.revoked.clone() }),
        };
        let mut cfg = ServerConfig::builder_with_provider(self.provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_client_cert_verifier(verifier)
            .with_cert_resolver(self.resolver.clone());
        cfg.alpn_protocols = Vec::new();
        Ok((Arc::new(cfg), slot))
    }
}

#[derive(Debug)]
struct SlotVerifier {
    inner: Arc<dyn ClientCertVerifier>,
    slot: Arc<Mutex<HandshakeSlot>>,
    revoked: RevocationList,
}

/// Short reason of a certificate error for the log ("unknown CA", "certificate expired", ...).
pub(crate) fn describe_cert_error(e: &rustls::Error) -> String {
    match e {
        rustls::Error::InvalidCertificate(ce) => match ce {
            CertificateError::UnknownIssuer => "unknown CA".into(),
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => "certificate expired".into(),
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => "certificate not valid yet".into(),
            CertificateError::Revoked => "revoked certificate".into(),
            CertificateError::BadSignature => "bad certificate signature".into(),
            CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
                "certificate not meant for clients".into()
            }
            other => format!("invalid certificate ({other:?})"),
        },
        rustls::Error::NoCertificatesPresented => "no client certificate".into(),
        rustls::Error::AlertReceived(a) => format!("client sent alert {a:?}"),
        rustls::Error::PeerIncompatible(_) => "incompatible TLS client".into(),
        rustls::Error::InvalidMessage(_) => "not a TLS client".into(),
        other => crate::error::short_detail(&other.to_string()),
    }
}

impl ClientCertVerifier for SlotVerifier {
    fn offer_client_auth(&self) -> bool {
        self.inner.offer_client_auth()
    }
    fn client_auth_mandatory(&self) -> bool {
        self.inner.client_auth_mandatory()
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        let info = peer_cert_of(end_entity.as_ref());
        match self.inner.verify_client_cert(end_entity, intermediates, now) {
            Ok(v) => {
                if let Some(pc) = &info {
                    if self.revoked.is_revoked(&pc.serial) {
                        if let Ok(mut s) = self.slot.lock() {
                            s.failure = Some(format!("revoked certificate {}", pc.cn.clone().unwrap_or_else(|| pc.serial.clone())));
                        }
                        return Err(rustls::Error::InvalidCertificate(CertificateError::Revoked));
                    }
                }
                if let Ok(mut s) = self.slot.lock() {
                    s.verified = info;
                }
                Ok(v)
            }
            Err(e) => {
                if let Ok(mut s) = self.slot.lock() {
                    let mut why = describe_cert_error(&e);
                    if let Some(cn) = info.and_then(|i| i.cn) {
                        why = format!("{why} (certificate {cn})");
                    }
                    s.failure = Some(why);
                }
                Err(e)
            }
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

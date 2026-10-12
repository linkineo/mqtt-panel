//! rustls client configuration with a verifier that records the presented chain and honours fingerprint trust.

use crate::options::TlsOptions;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{ring, CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::{DigitallySignedStruct, Error as TlsError, RootCertStore, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use std::sync::{Arc, Mutex, OnceLock};

/// The chain the server presented during the last handshake (leaf first).
#[derive(Debug, Default)]
pub struct Captured {
    pub chain: Vec<CertificateDer<'static>>,
    /// The verification outcome of our verifier: Some(error) when it refused.
    pub verify_error: Option<TlsError>,
    /// True when accepted only because the fingerprint was trusted or skip-verify was on.
    pub accepted_by_trust: bool,
}

pub fn provider() -> Arc<CryptoProvider> {
    static P: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    P.get_or_init(|| Arc::new(ring::default_provider())).clone()
}

/// System roots, loaded once.
pub fn system_roots() -> Arc<Vec<CertificateDer<'static>>> {
    static R: OnceLock<Arc<Vec<CertificateDer<'static>>>> = OnceLock::new();
    R.get_or_init(|| Arc::new(rustls_native_certs::load_native_certs().certs)).clone()
}

#[derive(Debug)]
struct CapturingVerifier {
    inner: Arc<WebPkiServerVerifier>,
    algs: WebPkiSupportedAlgorithms,
    captured: Arc<Mutex<Captured>>,
    trusted: Vec<String>,
    accept_any: bool,
    name_override: Option<ServerName<'static>>,
}

impl ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let mut cap = self.captured.lock().unwrap_or_else(|e| e.into_inner());
        cap.chain = std::iter::once(end_entity.clone().into_owned()).chain(intermediates.iter().map(|c| c.clone().into_owned())).collect();
        let fp = mqttpanel_tls::fingerprint_sha256(end_entity.as_ref());
        if self.accept_any || self.trusted.iter().any(|t| mqttpanel_tls::normalize_fingerprint(t) == fp) {
            cap.accepted_by_trust = true;
            return Ok(ServerCertVerified::assertion());
        }
        let name = self.name_override.as_ref().unwrap_or(server_name);
        match self.inner.verify_server_cert(end_entity, intermediates, name, ocsp_response, now) {
            Ok(v) => Ok(v),
            Err(e) => {
                cap.verify_error = Some(e.clone());
                Err(e)
            }
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// Errors building the configuration (bad CA file, bad identity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    BadCa,
    BadIdentity,
}

/// Builds the client configuration and returns the capture slot for the presented chain.
pub fn client_config(opts: &TlsOptions) -> Result<(Arc<rustls::ClientConfig>, Arc<Mutex<Captured>>), ConfigError> {
    let mut roots = RootCertStore::empty();
    for c in system_roots().iter() {
        let _ = roots.add(c.clone());
    }
    if let Some(ca) = &opts.ca {
        let certs = mqttpanel_tls::parse_certs(ca).map_err(|_| ConfigError::BadCa)?;
        for c in certs {
            roots.add(c).map_err(|_| ConfigError::BadCa)?;
        }
    }
    let provider = provider();
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map_err(|_| ConfigError::BadCa)?;
    let captured = Arc::new(Mutex::new(Captured::default()));
    let name_override = opts
        .sni
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| ServerName::try_from(s.trim().to_string()).ok());
    let verifier = CapturingVerifier {
        inner,
        algs: provider.signature_verification_algorithms,
        captured: captured.clone(),
        trusted: opts.trusted_fingerprints.clone(),
        accept_any: opts.skip_verify || !opts.verify_server,
        name_override,
    };
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| ConfigError::BadCa)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier));
    let mut config = match &opts.identity {
        Some(id) => builder.with_client_auth_cert(id.certs.clone(), id.key.clone_key()).map_err(|_| ConfigError::BadIdentity)?,
        None => builder.with_no_client_auth(),
    };
    if let Some(alpn) = opts.alpn.as_deref().filter(|a| !a.trim().is_empty()) {
        config.alpn_protocols = alpn.split(',').map(|p| p.trim().as_bytes().to_vec()).collect();
    }
    Ok((Arc::new(config), captured))
}

/// A configuration that accepts anything, for probing whether a port speaks TLS.
pub fn probe_config() -> Arc<rustls::ClientConfig> {
    let opts = TlsOptions { verify_server: false, skip_verify: true, ..Default::default() };
    client_config(&opts).map(|(c, _)| c).unwrap_or_else(|_| {
        Arc::new(
            rustls::ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("default versions")
                .with_root_certificates(RootCertStore::empty())
                .with_no_client_auth(),
        )
    })
}

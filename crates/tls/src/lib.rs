//! Certificate and key handling shared by the MQTT Panel client, broker, CLI and desktop app.
//!
//! - [`parse`]: PEM/DER certificates, private keys (PKCS#8, SEC1, PKCS#1, encrypted PKCS#8), PKCS#12 bundles,
//!   certificate details for the trust prompt and file summaries for the connection form.
//! - [`kit`]: certificate generation (local CA, server and client certificates, CSR signing, PKCS#12 export),
//!   used by the embedded broker and by the test kit (F-01).

pub mod kit;
pub mod parse;

pub use parse::{
    cert_details, fingerprint_sha256, inspect, load_identity, looks_like_pkcs12, normalize_fingerprint, parse_certs, parse_csr_pem,
    parse_pkcs12, parse_private_key, to_pem, CertDetails, CertSummary, FileExpect, Identity, TlsFileError,
};

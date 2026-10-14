//! From a stored profile and the keychain to client options.

use crate::model::{secret_keys, StoredProfile};
use crate::secrets::SecretStore;
use base64::Engine as _;
use mqttpanel_client::{msg, ConnectOptions, Scheme, UserMessage, VersionPref, Will};
use std::time::Duration;

/// Builds the options for a profile. Secrets come from the store. Verification is skipped when the profile
/// says so (`tls.skipVerify`) or for this one attempt (`skip_once`: "only this time", or the CLI's `--insecure`).
pub fn connect_options(p: &StoredProfile, secrets: &dyn SecretStore, skip_once: bool) -> Result<ConnectOptions, UserMessage> {
    let pr = &p.profile;
    let mut o = ConnectOptions::new(&pr.host, pr.port);
    o.scheme = Scheme::parse(&pr.scheme).unwrap_or(Scheme::Mqtt);
    o.ws_path = if pr.ws_path.is_empty() { "/mqtt".into() } else { pr.ws_path.clone() };
    o.version = match pr.mqtt_version.as_str() {
        "3.1.1" => VersionPref::V311,
        "5" => VersionPref::V5,
        _ => VersionPref::Auto,
    };
    o.client_id = if pr.client_id.trim().is_empty() { mqttpanel_client::random_client_id() } else { pr.client_id.clone() };
    if !pr.username.is_empty() {
        o.username = Some(pr.username.clone());
    }
    let get = |k: String| secrets.get(&k).map_err(|e| msg!(e.catalog_key(), detail = e.to_string()));
    o.password = get(secret_keys::password(&pr.id))?;
    if o.password.is_some() && o.username.is_none() {
        o.username = Some(String::new());
    }
    o.keep_alive = pr.keep_alive;
    o.clean_start = pr.clean_start;
    o.session_expiry = pr.session_expiry;
    o.last_will = pr.last_will.as_ref().filter(|w| !w.topic.trim().is_empty()).map(|w| Will {
        topic: w.topic.clone(),
        payload: w.payload.clone().into_bytes(),
        qos: w.qos,
        retain: w.retain,
    });
    o.subscriptions = pr.subscriptions.iter().map(|s| (s.filter.clone(), s.qos)).collect();
    o.reconnect = pr.reconnect.enabled;
    o.reconnect_min = Duration::from_secs(pr.reconnect.min_delay_s.max(1) as u64);
    o.reconnect_max = Duration::from_secs(pr.reconnect.max_delay_s.max(pr.reconnect.min_delay_s).max(1) as u64);
    o.tls.verify_server = true;
    o.tls.skip_verify = pr.tls.skip_verify || skip_once;
    o.tls.trusted_fingerprints = pr.tls.trusted_fingerprints.clone();
    o.tls.sni = Some(pr.tls.sni.clone()).filter(|s| !s.trim().is_empty());
    o.tls.alpn = Some(pr.tls.alpn.clone()).filter(|s| !s.trim().is_empty());
    o.tls.ca = p.blobs.ca_pem.clone().map(String::into_bytes);
    let passphrase = get(secret_keys::passphrase(&pr.id))?;
    if let Some(p12) = get(secret_keys::pkcs12(&pr.id))? {
        let bytes = base64::engine::general_purpose::STANDARD.decode(p12).map_err(|_| msg!("file.notPkcs12"))?;
        let name = pr.tls.client_cert.as_ref().map(|f| f.file_name.clone()).unwrap_or_else(|| "client.p12".into());
        o.tls.identity = Some(mqttpanel_tls::parse_pkcs12(&bytes, passphrase.as_deref()).map_err(|e| match e {
            mqttpanel_tls::TlsFileError::WrongPassphrase | mqttpanel_tls::TlsFileError::NeedsPassphrase => msg!("tls.wrongPassphrase", file = name),
            other => msg!(other.catalog_key()),
        })?);
    } else if let Some(cert) = &p.blobs.client_cert_pem {
        if let Some(key) = get(secret_keys::client_key(&pr.id))? {
            o.tls.identity = Some(mqttpanel_tls::load_identity(cert.as_bytes(), Some(key.as_bytes()), passphrase.as_deref()).map_err(|e| {
                let name = pr.tls.client_key.as_ref().map(|f| f.file_name.clone()).unwrap_or_default();
                match e {
                    mqttpanel_tls::TlsFileError::WrongPassphrase | mqttpanel_tls::TlsFileError::NeedsPassphrase => msg!("tls.wrongPassphrase", file = name),
                    other => msg!(other.catalog_key()),
                }
            })?);
        }
    }
    Ok(o)
}

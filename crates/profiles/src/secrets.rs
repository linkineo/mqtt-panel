//! Secrets (passwords, private keys, PKCS#12 bundles and passphrases) never sit in profile files. The app and the CLI
//! keep them in the encrypted secrets file ([`crate::secrets_file::FileSecrets`]) whose master key lives where the build
//! says (SPEC section 6, change set 10: the owner-only key file on unsigned builds, the OS store on signed ones; see
//! [`crate::keystore`]). [`MemorySecrets`] serves tests and `MQTTPANEL_SECRETS=memory`.
//! Keys are strings such as "profile/<id>/password" or "broker/ca-key"; values are text (binary is base64).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Keychain service name for every MQTT Panel secret.
pub const SERVICE: &str = "MQTT Panel";

#[derive(Debug, Clone, thiserror::Error)]
pub enum SecretError {
    /// The OS keychain is not available or refused access; the detail is for the log only.
    #[error("keychain unavailable: {0}")]
    Unavailable(String),
    /// The keychain did not give the master key (Deny, locked keychain, any error) earlier in this session. The
    /// refusal is remembered and never retried until the next start (SPEC 6, change set 3).
    #[error("keychain refused the master key: {0}")]
    Refused(String),
    /// The key file `master.key` cannot be used safely (other users can read it); it is not used until its permissions
    /// are fixed (change set 10). Remembered for the session like a refusal.
    #[error("{0}")]
    KeyFile(String),
}

impl SecretError {
    /// Catalog key for the user-facing sentence.
    pub fn catalog_key(&self) -> &'static str {
        match self {
            SecretError::Unavailable(_) => "keychain.unavailable",
            SecretError::Refused(_) => "keychain.refused",
            SecretError::KeyFile(_) => "keychain.keyFile",
        }
    }
}

/// A store for secrets: the encrypted secrets file and, for tests, memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
    /// True when a secret is stored under `key`. Stores that can answer without decrypting (the secrets file) do so
    /// without touching the keychain.
    fn contains(&self, key: &str) -> Result<bool, SecretError> {
        Ok(self.get(key)?.is_some())
    }
}

/// In-memory secrets, for tests and for sessions where the keychain is unavailable.
#[derive(Debug, Clone, Default)]
pub struct MemorySecrets {
    map: Arc<Mutex<HashMap<String, String>>>,
}

impl MemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }
    /// Number of stored secrets (tests).
    pub fn len(&self) -> usize {
        self.map.lock().map(|m| m.len()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.map.lock().map_err(|e| SecretError::Unavailable(e.to_string()))?.get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.map.lock().map_err(|e| SecretError::Unavailable(e.to_string()))?.insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.map.lock().map_err(|e| SecretError::Unavailable(e.to_string()))?.remove(key);
        Ok(())
    }
}

impl<T: SecretStore + ?Sized> SecretStore for Arc<T> {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        (**self).get(key)
    }
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        (**self).set(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        (**self).delete(key)
    }
    fn contains(&self, key: &str) -> Result<bool, SecretError> {
        (**self).contains(key)
    }
}

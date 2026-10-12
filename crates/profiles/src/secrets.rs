//! Secrets (passwords, private keys, PKCS#12 bundles and passphrases) live in the OS keychain, never in files.
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
}

impl SecretError {
    /// Catalog key for the user-facing sentence.
    pub fn catalog_key(&self) -> &'static str {
        "keychain.unavailable"
    }
}

/// A store for secrets. Implemented by the OS keychain and, for tests, by memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// The OS keychain (macOS Keychain, Windows Credential Manager, Secret Service on Linux).
#[derive(Debug, Clone, Default)]
pub struct KeychainSecrets {
    service: Option<String>,
}

impl KeychainSecrets {
    pub fn new() -> Self {
        Self { service: None }
    }
    /// A separate service name, for tests that must not touch real entries.
    pub fn with_service(service: &str) -> Self {
        Self { service: Some(service.to_string()) }
    }
    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(self.service.as_deref().unwrap_or(SERVICE), key).map_err(|e| SecretError::Unavailable(e.to_string()))
    }
}

impl SecretStore for KeychainSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.entry(key)?.set_password(value).map_err(|e| SecretError::Unavailable(e.to_string()))
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
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
}

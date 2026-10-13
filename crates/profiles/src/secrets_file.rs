//! The secrets file (SPEC section 6, change set 2): every secret lives in `secrets.enc` in the data folder, each record
//! encrypted and authenticated with XChaCha20-Poly1305 under one random 256-bit master key. The OS keychain holds
//! exactly one item for MQTT Panel, that master key.
//!
//! - The master key is read lazily, the first time a secret value is read or written, never when the store is created,
//!   and cached for the session. Listing, `contains` and `delete` never need it, nor does reading a secret that is not
//!   there.
//! - Each record has its own random 24-byte nonce; the record name is bound as associated data, so a record cannot be
//!   moved to another name.
//! - The file is versioned (`version`, `cipher`); names are visible, values are not.
//! - Existing per-secret keychain entries are moved into the file once ([`FileSecrets::migrate_from`]) and deleted.
//!
//! File format (JSON):
//! ```json
//! { "format": "mqttpanel-secrets", "version": 1, "cipher": "xchacha20poly1305", "migrated": true,
//!   "records": { "profile/<id>/password": { "nonce": "<base64, 24 bytes>", "data": "<base64 ciphertext + tag>" } } }
//! ```

use crate::secrets::{SecretError, SecretStore, SERVICE};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// File name of the secrets file inside the data folder.
pub const SECRETS_FILE: &str = "secrets.enc";
/// Keychain account of the master key (service [`SERVICE`]).
pub const MASTER_KEY_ACCOUNT: &str = "master-key";
const FORMAT: &str = "mqttpanel-secrets";
const VERSION: u32 = 1;
const CIPHER: &str = "xchacha20poly1305";

/// Where the master key comes from.
pub trait MasterKeySource: Send + Sync {
    /// Reads the master key, creating and storing a random one when none exists yet.
    fn load_or_create(&self) -> Result<[u8; 32], SecretError>;
}

/// The master key as the single OS keychain item of MQTT Panel (base64 text).
#[derive(Debug, Clone)]
pub struct KeychainMasterKey {
    service: String,
}

impl Default for KeychainMasterKey {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainMasterKey {
    pub fn new() -> Self {
        Self { service: SERVICE.to_string() }
    }
    /// A separate service name, for tests that must not touch the real item.
    pub fn with_service(service: &str) -> Self {
        Self { service: service.to_string() }
    }
}

impl MasterKeySource for KeychainMasterKey {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        let entry = keyring::Entry::new(&self.service, MASTER_KEY_ACCOUNT).map_err(|e| SecretError::Unavailable(e.to_string()))?;
        let b64 = base64::engine::general_purpose::STANDARD;
        match entry.get_password() {
            Ok(text) => {
                let bytes = b64.decode(text.trim()).map_err(|_| SecretError::Unavailable("master key damaged".into()))?;
                bytes.try_into().map_err(|_| SecretError::Unavailable("master key damaged".into()))
            }
            Err(keyring::Error::NoEntry) => {
                let key: [u8; 32] = rand::random();
                entry.set_password(&b64.encode(key)).map_err(|e| SecretError::Unavailable(e.to_string()))?;
                Ok(key)
            }
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }
}

/// A fixed master key that counts how often it is loaded (tests).
#[derive(Debug, Default)]
pub struct FixedMasterKey {
    key: [u8; 32],
    loads: std::sync::atomic::AtomicUsize,
}

impl FixedMasterKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, loads: Default::default() }
    }
    /// How many times the key was loaded (the keychain would have been asked this often).
    pub fn loads(&self) -> usize {
        self.loads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl MasterKeySource for FixedMasterKey {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        self.loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.key)
    }
}

impl<T: MasterKeySource + ?Sized> MasterKeySource for Arc<T> {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        (**self).load_or_create()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    nonce: String,
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileData {
    format: String,
    version: u32,
    cipher: String,
    #[serde(default)]
    migrated: bool,
    #[serde(default)]
    records: BTreeMap<String, Record>,
}

impl Default for FileData {
    fn default() -> Self {
        FileData { format: FORMAT.into(), version: VERSION, cipher: CIPHER.into(), migrated: false, records: BTreeMap::new() }
    }
}

/// What [`FileSecrets::migrate_from`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Secrets moved into the file (and deleted from the keychain).
    pub moved: usize,
    /// Keys whose keychain entry could not be read; migration is retried at the next start.
    pub failed: usize,
}

/// Secrets in an encrypted file, keyed by one master key (see the module documentation).
pub struct FileSecrets {
    path: PathBuf,
    source: Arc<dyn MasterKeySource>,
    key: Mutex<Option<[u8; 32]>>,
    io: Mutex<()>,
}

impl std::fmt::Debug for FileSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSecrets").field("path", &self.path).finish()
    }
}

fn unavailable(e: impl std::fmt::Display) -> SecretError {
    SecretError::Unavailable(e.to_string())
}

fn aad(name: &str) -> Vec<u8> {
    format!("{FORMAT}/v{VERSION}/{name}").into_bytes()
}

impl FileSecrets {
    /// A store on `path` (usually `<data folder>/secrets.enc`). Nothing is read and the master key is not touched.
    pub fn new(path: PathBuf, source: Arc<dyn MasterKeySource>) -> Self {
        FileSecrets { path, source, key: Mutex::new(None), io: Mutex::new(()) }
    }

    /// The store of the data folder `dir` with the master key in the OS keychain.
    pub fn in_dir(dir: &Path) -> Self {
        Self::new(dir.join(SECRETS_FILE), Arc::new(KeychainMasterKey::new()))
    }

    /// The file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True once the master key was loaded in this session.
    pub fn key_loaded(&self) -> bool {
        self.key.lock().map(|k| k.is_some()).unwrap_or(false)
    }

    fn master(&self) -> Result<[u8; 32], SecretError> {
        let mut k = self.key.lock().map_err(unavailable)?;
        if let Some(key) = *k {
            return Ok(key);
        }
        let key = self.source.load_or_create()?;
        *k = Some(key);
        Ok(key)
    }

    fn read(&self) -> Result<Option<FileData>, SecretError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let d: FileData = serde_json::from_slice(&bytes).map_err(|e| unavailable(format!("{SECRETS_FILE}: {e}")))?;
                if d.format != FORMAT || d.version != VERSION || d.cipher != CIPHER {
                    return Err(unavailable(format!("{SECRETS_FILE}: unsupported version {} ({})", d.version, d.cipher)));
                }
                Ok(Some(d))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(unavailable(e)),
        }
    }

    fn write(&self, d: &FileData) -> Result<(), SecretError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(unavailable)?;
        }
        let tmp = self.path.with_extension("enc.tmp");
        let text = serde_json::to_vec_pretty(d).map_err(unavailable)?;
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(unavailable)?;
            f.write_all(&text).map_err(unavailable)?;
            f.sync_all().map_err(unavailable)?;
        }
        std::fs::rename(&tmp, &self.path).map_err(unavailable)
    }

    fn modify<T>(&self, f: impl FnOnce(&mut FileData) -> Result<T, SecretError>) -> Result<T, SecretError> {
        let _g = self.io.lock().map_err(unavailable)?;
        let mut d = self.read()?.unwrap_or_default();
        let out = f(&mut d)?;
        self.write(&d)?;
        Ok(out)
    }

    fn encrypt(&self, name: &str, value: &str) -> Result<Record, SecretError> {
        let key = self.master()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let nonce: [u8; 24] = rand::random();
        let a = aad(name);
        let ct = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: value.as_bytes(), aad: &a })
            .map_err(|_| unavailable("encryption failed"))?;
        let b64 = base64::engine::general_purpose::STANDARD;
        Ok(Record { nonce: b64.encode(nonce), data: b64.encode(ct) })
    }

    fn decrypt(&self, name: &str, r: &Record) -> Result<String, SecretError> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let nonce = b64.decode(&r.nonce).map_err(|_| unavailable("damaged record"))?;
        let ct = b64.decode(&r.data).map_err(|_| unavailable("damaged record"))?;
        if nonce.len() != 24 {
            return Err(unavailable("damaged record"));
        }
        let key = self.master()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let a = aad(name);
        let plain = cipher
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad: &a })
            .map_err(|_| unavailable("the secrets file does not match the keychain master key"))?;
        String::from_utf8(plain).map_err(|_| unavailable("damaged record"))
    }

    /// Names of the stored secrets (no master key needed).
    pub fn names(&self) -> Result<Vec<String>, SecretError> {
        Ok(self.read()?.map(|d| d.records.keys().cloned().collect()).unwrap_or_default())
    }

    /// True when the per-secret keychain entries of earlier builds have not been moved into the file yet.
    pub fn needs_migration(&self) -> bool {
        !matches!(self.read(), Ok(Some(d)) if d.migrated)
    }

    /// Moves the given per-secret keychain entries (`legacy`) into the file and deletes them from the keychain.
    /// Secrets already in the file win. When every entry could be read the file is marked as migrated; otherwise the
    /// next start tries again. With nothing to move the master key is not touched.
    pub fn migrate_from(&self, legacy: &dyn SecretStore, keys: &[String]) -> MigrationReport {
        let mut report = MigrationReport::default();
        let present: std::collections::HashSet<String> = self.names().unwrap_or_default().into_iter().collect();
        for k in keys {
            match legacy.get(k) {
                Ok(Some(v)) => {
                    if present.contains(k) || self.set(k, &v).is_ok() {
                        let _ = legacy.delete(k);
                        report.moved += 1;
                    } else {
                        report.failed += 1;
                    }
                }
                Ok(None) => {}
                Err(_) => report.failed += 1,
            }
        }
        if report.failed == 0 {
            let _ = self.modify(|d| {
                d.migrated = true;
                Ok(())
            });
        }
        report
    }
}

impl SecretStore for FileSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let rec = {
            let _g = self.io.lock().map_err(unavailable)?;
            self.read()?.and_then(|mut d| d.records.remove(key))
        };
        match rec {
            Some(r) => self.decrypt(key, &r).map(Some),
            None => Ok(None),
        }
    }
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let rec = self.encrypt(key, value)?;
        self.modify(|d| {
            d.records.insert(key.to_string(), rec);
            Ok(())
        })
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let _g = self.io.lock().map_err(unavailable)?;
        let Some(mut d) = self.read()? else { return Ok(()) };
        if d.records.remove(key).is_some() {
            self.write(&d)?;
        }
        Ok(())
    }
    fn contains(&self, key: &str) -> Result<bool, SecretError> {
        Ok(self.read()?.is_some_and(|d| d.records.contains_key(key)))
    }
}

/// Opens the secrets file of the data folder `dir` (master key in the OS keychain) and, once, moves the per-secret
/// keychain entries of earlier builds (`legacy_keys`, from the profiles and the broker state) into it. Without legacy
/// keys nothing touches the keychain here; the master key is read on first secret use.
pub fn open_app_secrets(dir: &Path, legacy_keys: &[String]) -> Arc<FileSecrets> {
    let fs = Arc::new(FileSecrets::in_dir(dir));
    if fs.needs_migration() {
        let report = fs.migrate_from(&crate::secrets::KeychainSecrets::new(), legacy_keys);
        if std::env::var_os("MQTTPANEL_LOG").is_some() {
            eprintln!("[mqttpanel] secrets migration: {} moved, {} failed", report.moved, report.failed);
        }
    }
    fs
}

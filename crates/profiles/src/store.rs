//! On-disk store: profiles.json, settings.json and templates.json in the app data folder.
//! Secrets go to the [`SecretStore`]; files never contain passwords or keys.

use crate::model::{secret_keys, Profile, Settings, StoredProfile, Template};
use crate::secrets::SecretStore;
use base64::Engine as _;
use mqttpanel_client::{msg, UserMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Identifier of the desktop app; its data folder is shared with the CLI.
pub const APP_ID: &str = "com.remora-labs.mqttpanel";

/// The app data folder (`MQTTPANEL_HOME` overrides it, for tests and portable use).
pub fn default_dir() -> PathBuf {
    if let Ok(h) = std::env::var("MQTTPANEL_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(APP_ID)
}

pub struct Store {
    dir: PathBuf,
    secrets: Arc<dyn SecretStore>,
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(tmp, path)
}

fn io_msg(path: &Path, e: std::io::Error) -> UserMessage {
    msg!("file.writeFailed", file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(), detail = e.kind().to_string())
}

/// Profile export file (secrets stripped, or encrypted with a passphrase).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    pub profiles: Vec<StoredProfile>,
    #[serde(default)]
    pub templates: HashMap<String, Vec<Template>>,
    #[serde(default)]
    pub secrets: Option<EncryptedSecrets>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedSecrets {
    pub kdf: String,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

pub const EXPORT_FORMAT: &str = "mqttpanel-profiles";

fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    let _ = argon2::Argon2::default().hash_password_into(passphrase.as_bytes(), salt, &mut key);
    key
}

/// Encrypts a secrets map with a passphrase (Argon2id + XChaCha20-Poly1305).
pub fn encrypt_secrets(map: &HashMap<String, String>, passphrase: &str) -> EncryptedSecrets {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    let salt: [u8; 16] = rand::random();
    let nonce: [u8; 24] = rand::random();
    let key = derive_key(passphrase, &salt);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let plain = serde_json::to_vec(map).unwrap_or_default();
    let ct = cipher.encrypt(XNonce::from_slice(&nonce), plain.as_ref()).unwrap_or_default();
    let b64 = base64::engine::general_purpose::STANDARD;
    EncryptedSecrets { kdf: "argon2id".into(), salt: b64.encode(salt), nonce: b64.encode(nonce), ciphertext: b64.encode(ct) }
}

/// Decrypts; None when the passphrase is wrong or the data damaged.
pub fn decrypt_secrets(e: &EncryptedSecrets, passphrase: &str) -> Option<HashMap<String, String>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    let b64 = base64::engine::general_purpose::STANDARD;
    let salt = b64.decode(&e.salt).ok()?;
    let nonce = b64.decode(&e.nonce).ok()?;
    let ct = b64.decode(&e.ciphertext).ok()?;
    if nonce.len() != 24 {
        return None;
    }
    let key = derive_key(passphrase, &salt);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let plain = cipher.decrypt(XNonce::from_slice(&nonce), ct.as_ref()).ok()?;
    serde_json::from_slice(&plain).ok()
}

impl Store {
    pub fn new(dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Store { dir, secrets }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.secrets
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn read_json<T: for<'de> Deserialize<'de> + Default>(&self, name: &str) -> T {
        std::fs::read(self.path(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_json<T: Serialize>(&self, name: &str, v: &T) -> Result<(), UserMessage> {
        let p = self.path(name);
        let data = serde_json::to_vec_pretty(v).map_err(|e| msg!("error.internal", detail = e.to_string()))?;
        write_atomic(&p, &data).map_err(|e| io_msg(&p, e))
    }

    // ------------------------------------------------------------ profiles

    pub fn list(&self) -> Vec<StoredProfile> {
        self.read_json::<Vec<StoredProfile>>("profiles.json")
    }

    fn write_all(&self, all: &[StoredProfile]) -> Result<(), UserMessage> {
        self.write_json("profiles.json", &all)
    }

    pub fn profiles(&self) -> Vec<Profile> {
        self.list().into_iter().map(|s| s.profile).collect()
    }

    pub fn get(&self, id: &str) -> Option<StoredProfile> {
        self.list().into_iter().find(|p| p.profile.id == id)
    }

    /// Finds a profile by id, exact name, case-insensitive name, or slug ("Plant broker" = "plant-broker").
    pub fn find(&self, needle: &str) -> Option<StoredProfile> {
        let all = self.list();
        let slug = |s: &str| s.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-");
        all.iter()
            .find(|p| p.profile.id == needle || p.profile.name == needle)
            .or_else(|| all.iter().find(|p| p.profile.name.eq_ignore_ascii_case(needle)))
            .or_else(|| all.iter().find(|p| slug(&p.profile.name) == slug(needle)))
            .cloned()
    }

    /// Creates or updates; keeps stored blobs; `has_password` reflects the keychain.
    pub fn save(&self, mut profile: Profile) -> Result<Profile, UserMessage> {
        let mut all = self.list();
        profile.has_password = self.secrets.get(&secret_keys::password(&profile.id)).ok().flatten().is_some();
        match all.iter_mut().find(|p| p.profile.id == profile.id) {
            Some(existing) => existing.profile = profile.clone(),
            None => all.push(StoredProfile { profile: profile.clone(), blobs: Default::default() }),
        }
        self.write_all(&all)?;
        Ok(profile)
    }

    pub fn save_stored(&self, stored: StoredProfile) -> Result<(), UserMessage> {
        let mut all = self.list();
        match all.iter_mut().find(|p| p.profile.id == stored.profile.id) {
            Some(existing) => *existing = stored,
            None => all.push(stored),
        }
        self.write_all(&all)
    }

    pub fn update<F: FnOnce(&mut StoredProfile)>(&self, id: &str, f: F) -> Result<StoredProfile, UserMessage> {
        let mut all = self.list();
        let p = all.iter_mut().find(|p| p.profile.id == id).ok_or_else(|| msg!("profile.notFound"))?;
        f(p);
        let out = p.clone();
        self.write_all(&all)?;
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<(), UserMessage> {
        let mut all = self.list();
        all.retain(|p| p.profile.id != id);
        self.write_all(&all)?;
        for k in secret_keys::all(id) {
            let _ = self.secrets.delete(&k);
        }
        let mut t = self.all_templates();
        if t.remove(id).is_some() {
            self.write_json("templates.json", &t)?;
        }
        Ok(())
    }

    pub fn duplicate(&self, id: &str) -> Result<Profile, UserMessage> {
        let src = self.get(id).ok_or_else(|| msg!("profile.notFound"))?;
        let mut copy = src.clone();
        copy.profile.id = uuid::Uuid::new_v4().to_string();
        copy.profile.name = format!("{} (copy)", src.profile.name);
        copy.profile.is_local_broker = false;
        copy.profile.last_used = None;
        copy.profile.last_test = None;
        copy.profile.client_id = mqttpanel_client::random_client_id();
        for (from, to) in secret_keys::all(id).into_iter().zip(secret_keys::all(&copy.profile.id)) {
            if let Ok(Some(v)) = self.secrets.get(&from) {
                let _ = self.secrets.set(&to, &v);
            }
        }
        self.save_stored(copy.clone())?;
        Ok(copy.profile)
    }

    pub fn reorder(&self, ids: &[String]) -> Result<(), UserMessage> {
        let mut all = self.list();
        all.sort_by_key(|p| ids.iter().position(|i| *i == p.profile.id).unwrap_or(usize::MAX));
        self.write_all(&all)
    }

    pub fn set_password(&self, id: &str, password: &str) -> Result<Profile, UserMessage> {
        let key = secret_keys::password(id);
        let r = if password.is_empty() { self.secrets.delete(&key) } else { self.secrets.set(&key, password) };
        r.map_err(|e| msg!("keychain.unavailable", detail = e.to_string()))?;
        let p = self.update(id, |p| p.profile.has_password = !password.is_empty())?;
        Ok(p.profile)
    }

    pub fn password(&self, id: &str) -> Option<String> {
        self.secrets.get(&secret_keys::password(id)).ok().flatten()
    }

    // ------------------------------------------------------------ settings

    pub fn settings(&self) -> Settings {
        let mut s: Settings = self.read_json("settings.json");
        s.skip_verify = false;
        s.clamped()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<Settings, UserMessage> {
        let mut persisted = s.clone().clamped();
        let session_skip = persisted.skip_verify;
        persisted.skip_verify = false;
        self.write_json("settings.json", &persisted)?;
        persisted.skip_verify = session_skip;
        Ok(persisted)
    }

    // ------------------------------------------------------------ templates

    fn all_templates(&self) -> HashMap<String, Vec<Template>> {
        self.read_json("templates.json")
    }

    pub fn templates(&self, profile_id: &str) -> Vec<Template> {
        self.all_templates().remove(profile_id).unwrap_or_default()
    }

    pub fn save_template(&self, profile_id: &str, mut t: Template) -> Result<Template, UserMessage> {
        if t.id.is_empty() {
            t.id = uuid::Uuid::new_v4().to_string();
        }
        let mut all = self.all_templates();
        let list = all.entry(profile_id.to_string()).or_default();
        match list.iter_mut().find(|x| x.id == t.id) {
            Some(x) => *x = t.clone(),
            None => list.push(t.clone()),
        }
        self.write_json("templates.json", &all)?;
        Ok(t)
    }

    pub fn delete_template(&self, profile_id: &str, template_id: &str) -> Result<(), UserMessage> {
        let mut all = self.all_templates();
        if let Some(list) = all.get_mut(profile_id) {
            list.retain(|t| t.id != template_id);
        }
        self.write_json("templates.json", &all)
    }

    // ------------------------------------------------------------ export and import

    /// Export of all profiles and templates; secrets only with a passphrase, encrypted.
    pub fn export(&self, passphrase: Option<&str>) -> Vec<u8> {
        let profiles = self.list();
        let mut secrets = None;
        if let Some(pass) = passphrase.filter(|p| !p.is_empty()) {
            let mut map = HashMap::new();
            for p in &profiles {
                for k in secret_keys::all(&p.profile.id) {
                    if let Ok(Some(v)) = self.secrets.get(&k) {
                        map.insert(k, v);
                    }
                }
            }
            secrets = Some(encrypt_secrets(&map, pass));
        }
        let mut profiles = profiles;
        for p in &mut profiles {
            p.profile.has_password = false;
            p.profile.last_test = None;
        }
        let file = ExportFile { format: EXPORT_FORMAT.into(), version: 1, profiles, templates: self.all_templates(), secrets };
        serde_json::to_vec_pretty(&file).unwrap_or_default()
    }

    /// Adds imported profiles (new ids) and, when given, their secrets.
    pub fn add_imported(&self, profiles: Vec<StoredProfile>, secrets: HashMap<String, String>) -> Result<Vec<String>, UserMessage> {
        let mut names = vec![];
        let mut all = self.list();
        for mut p in profiles {
            let old_id = p.profile.id.clone();
            p.profile.id = uuid::Uuid::new_v4().to_string();
            p.profile.is_local_broker = false;
            let mut has_pw = false;
            for (from, to) in secret_keys::all(&old_id).into_iter().zip(secret_keys::all(&p.profile.id)) {
                if let Some(v) = secrets.get(&from) {
                    if self.secrets.set(&to, v).is_ok() && to.ends_with("/password") {
                        has_pw = true;
                    }
                }
            }
            p.profile.has_password = has_pw;
            names.push(p.profile.name.clone());
            all.push(p);
        }
        self.write_all(&all)?;
        Ok(names)
    }
}

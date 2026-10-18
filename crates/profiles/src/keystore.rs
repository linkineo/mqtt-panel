//! Where the master key lives, by build (SPEC section 6, change set 10). There is no settings toggle.
//!
//! - **Selection** ([`choose_store`], one place for the app and the CLI): `MQTTPANEL_KEYSTORE=file|keychain` wins.
//!   Otherwise, on macOS, a build signed with a real identity (a code signature with a certificate chain: Developer ID,
//!   Apple Development or a local self-signed identity) keeps the key in the keychain; an ad-hoc signature or none
//!   (dev builds, `npm run tauri dev`, CI, an unsigned beta) uses the key file. On Windows and Linux the OS store is
//!   silent (DPAPI through the credential manager, Secret Service), so the keychain store stays; on Linux without a
//!   Secret Service the key file is the fallback.
//! - **Key file** ([`FileMasterKey`]): `<data folder>/master.key`, one line of base64 text holding the 32 key bytes.
//!   On Unix the file is created with mode 0600 and the data folder is set to 0700; a key file that the group or
//!   others can read is refused with [`SecretError::KeyFile`] instead of being used. On Windows the file inherits the
//!   ACL of the per-user profile folder (`%APPDATA%`), which only the user and administrators can read.
//! - **Move** ([`MigratingMasterKey`]): when the chosen store holds no key but the other one does, the key is read once
//!   from the old place, written to the new one and the old item deleted, with one log line. If the old store refuses
//!   (Deny, locked keychain), the error is returned as a refusal for the session and nothing changes, so the move is
//!   tried again at the next start. A fresh key is created only when neither store holds one, so `secrets.enc` always
//!   stays readable.
//! - The macOS Data Protection keychain for signed builds comes with the signing step (phase 5); until then a signed
//!   build uses the login keychain through [`KeychainMasterKey`].

use crate::secrets::SecretError;
use crate::secrets_file::{KeychainMasterKey, MasterKeySource, MASTER_KEY_ACCOUNT};
use base64::Engine as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// File name of the master key file inside the data folder.
pub const KEY_FILE: &str = "master.key";
/// Environment variable that forces the store: `file` or `keychain` (tests, support).
pub const KEYSTORE_ENV: &str = "MQTTPANEL_KEYSTORE";
/// The plain message for a key file that other users can read.
pub const KEY_FILE_READABLE: &str = "The key file master.key can be read by other users; fix its permissions.";

fn log(line: &str) {
    if std::env::var_os("MQTTPANEL_LOG").is_some() {
        eprintln!("[mqttpanel] {line}");
    }
}

// ---------------------------------------------------------------- selection

/// Where the master key lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStoreKind {
    /// The owner-only key file `master.key` in the data folder.
    File,
    /// The OS secret store (macOS keychain, Windows credential manager, Linux Secret Service).
    Keychain,
}

impl KeyStoreKind {
    /// "file" or "keychain", as the UI query `key_store_kind` returns it.
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStoreKind::File => "file",
            KeyStoreKind::Keychain => "keychain",
        }
    }
}

/// The code signature of the running program (macOS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signature {
    /// No signature at all.
    None,
    /// An ad-hoc signature (no certificate), as the linker and `tauri dev` produce.
    AdHoc,
    /// A signature by a real identity (a certificate chain); `team_id` is set for Apple-issued identities
    /// (Developer ID, Apple Development) and absent for a local self-signed identity.
    Identity { team_id: Option<String> },
}

/// The platform rules of [`choose_store`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// The store rule (SPEC 6, change set 10). `override_value` is `MQTTPANEL_KEYSTORE` (`file` or `keychain`, any other
/// value is ignored); `signature` is asked only on macOS and only without an override. The Linux fallback to the file
/// when no Secret Service exists is applied by [`selected_store`].
pub fn choose_store(override_value: Option<&str>, platform: Platform, signature: impl FnOnce() -> Signature) -> KeyStoreKind {
    match override_value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("file") => return KeyStoreKind::File,
        Some("keychain") => return KeyStoreKind::Keychain,
        _ => {}
    }
    match platform {
        Platform::MacOs => match signature() {
            Signature::Identity { .. } => KeyStoreKind::Keychain,
            Signature::AdHoc | Signature::None => KeyStoreKind::File,
        },
        Platform::Windows | Platform::Linux => KeyStoreKind::Keychain,
    }
}

/// True when the OS secret store could be initialised (on Linux: a Secret Service answers on the session bus). This
/// opens no item and shows no dialog.
pub fn keychain_backend_available() -> bool {
    keyring::Entry::store_status().is_ok()
}

/// The store of this process: [`choose_store`] with the environment and the program's own signature, plus the Linux
/// fallback (no Secret Service → file, logged once). Decided once and cached for the process; reads no key.
pub fn selected_store() -> KeyStoreKind {
    static SELECTED: OnceLock<KeyStoreKind> = OnceLock::new();
    *SELECTED.get_or_init(|| {
        let env = std::env::var(KEYSTORE_ENV).ok();
        let kind = choose_store(env.as_deref(), Platform::current(), own_signature);
        let forced = matches!(env.as_deref().map(str::trim), Some("file" | "keychain"));
        if kind == KeyStoreKind::Keychain && Platform::current() == Platform::Linux && !forced && !keychain_backend_available() {
            log("no Secret Service on this session: the master key is kept in the key file master.key");
            return KeyStoreKind::File;
        }
        log(&format!("master key store: {}", kind.as_str()));
        kind
    })
}

/// The code signature of the running program, read once per process (macOS; `Signature::None` elsewhere).
pub fn own_signature() -> Signature {
    static SIG: OnceLock<Signature> = OnceLock::new();
    SIG.get_or_init(read_own_signature).clone()
}

#[cfg(target_os = "macos")]
fn read_own_signature() -> Signature {
    macos::own_signature()
}

#[cfg(not(target_os = "macos"))]
fn read_own_signature() -> Signature {
    Signature::None
}

/// The signature through the Security framework: `SecCodeCopySelf` and `SecCodeCopySigningInformation`. A signature
/// with a certificate chain (`certificates`) is a real identity; `teamid` names its team; ad-hoc signatures carry the
/// `kSecCodeSignatureAdhoc` flag and no certificates.
#[cfg(target_os = "macos")]
mod macos {
    use super::Signature;
    use std::ffi::{c_char, c_void, CStr};

    type CFTypeRef = *const c_void;
    const UTF8: u32 = 0x0800_0100;
    const SIGNING_INFORMATION: u32 = 1 << 1;
    const ADHOC: u32 = 0x0002;
    const NUMBER_SINT32: isize = 3;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const c_char, encoding: u32) -> CFTypeRef;
        fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn CFArrayGetCount(array: CFTypeRef) -> isize;
        fn CFNumberGetValue(number: CFTypeRef, kind: isize, out: *mut c_void) -> u8;
        fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, len: isize, encoding: u32) -> u8;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFNumberGetTypeID() -> usize;
        fn CFRelease(cf: CFTypeRef);
    }

    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecCodeCopySelf(flags: u32, code: *mut CFTypeRef) -> i32;
        fn SecCodeCopySigningInformation(code: CFTypeRef, flags: u32, info: *mut CFTypeRef) -> i32;
    }

    unsafe fn get(dict: CFTypeRef, key: &CStr) -> CFTypeRef {
        let k = CFStringCreateWithCString(std::ptr::null(), key.as_ptr(), UTF8);
        if k.is_null() {
            return std::ptr::null();
        }
        let v = CFDictionaryGetValue(dict, k);
        CFRelease(k);
        v
    }

    pub fn own_signature() -> Signature {
        // SAFETY: plain Core Foundation calls; every object created or copied here is released once, values read
        // from the dictionary are borrowed and only used while it is alive.
        unsafe {
            let mut code: CFTypeRef = std::ptr::null();
            if SecCodeCopySelf(0, &mut code) != 0 || code.is_null() {
                return Signature::None;
            }
            let mut info: CFTypeRef = std::ptr::null();
            let status = SecCodeCopySigningInformation(code, SIGNING_INFORMATION, &mut info);
            CFRelease(code);
            if status != 0 || info.is_null() {
                return Signature::None;
            }
            let flags_ref = get(info, c"flags");
            let certs = get(info, c"certificates");
            let team = get(info, c"teamid");
            let mut flags: i32 = 0;
            let has_flags = !flags_ref.is_null()
                && CFGetTypeID(flags_ref) == CFNumberGetTypeID()
                && CFNumberGetValue(flags_ref, NUMBER_SINT32, &mut flags as *mut i32 as *mut c_void) != 0;
            let has_certs = !certs.is_null() && CFGetTypeID(certs) == CFArrayGetTypeID() && CFArrayGetCount(certs) > 0;
            let team_id = if !team.is_null() && CFGetTypeID(team) == CFStringGetTypeID() {
                let mut buf = [0 as c_char; 64];
                if CFStringGetCString(team, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0 {
                    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
                } else {
                    None
                }
            } else {
                None
            };
            CFRelease(info);
            if has_certs {
                Signature::Identity { team_id }
            } else if has_flags {
                // Ad-hoc (`kSecCodeSignatureAdhoc` in the flags), or any other signature without a certificate chain.
                let _adhoc = (flags as u32) & ADHOC != 0;
                Signature::AdHoc
            } else {
                Signature::None
            }
        }
    }
}

// ---------------------------------------------------------------- key slots

/// One place a master key can live, as the move between stores sees it.
pub trait KeySlot: Send + Sync {
    /// The key, or `None` when this place holds none.
    fn read(&self) -> Result<Option<[u8; 32]>, SecretError>;
    /// Stores `key` when this place holds none yet and returns the key now stored (another process may have been
    /// quicker; its key wins).
    fn store_new(&self, key: &[u8; 32]) -> Result<[u8; 32], SecretError>;
    /// Removes the key from this place (nothing to remove is fine).
    fn remove(&self) -> Result<(), SecretError>;
    /// A short name for the log line ("key file", "keychain").
    fn label(&self) -> &'static str;
}

/// No old store (Linux without Secret Service, or a test).
#[derive(Debug, Default)]
pub struct NoSlot;

impl KeySlot for NoSlot {
    fn read(&self) -> Result<Option<[u8; 32]>, SecretError> {
        Ok(None)
    }
    fn store_new(&self, key: &[u8; 32]) -> Result<[u8; 32], SecretError> {
        Ok(*key)
    }
    fn remove(&self) -> Result<(), SecretError> {
        Ok(())
    }
    fn label(&self) -> &'static str {
        "nothing"
    }
}

impl KeySlot for KeychainMasterKey {
    fn read(&self) -> Result<Option<[u8; 32]>, SecretError> {
        let entry = keyring::Entry::new(self.service(), MASTER_KEY_ACCOUNT).map_err(|e| SecretError::Unavailable(e.to_string()))?;
        match entry.get_password() {
            Ok(text) => decode_key(&text).map(Some).ok_or_else(|| SecretError::Unavailable("master key damaged".into())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }
    fn store_new(&self, key: &[u8; 32]) -> Result<[u8; 32], SecretError> {
        let entry = keyring::Entry::new(self.service(), MASTER_KEY_ACCOUNT).map_err(|e| SecretError::Unavailable(e.to_string()))?;
        entry.set_password(&encode_key(key)).map_err(|e| SecretError::Unavailable(e.to_string()))?;
        Ok(*key)
    }
    fn remove(&self) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(self.service(), MASTER_KEY_ACCOUNT).map_err(|e| SecretError::Unavailable(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }
    fn label(&self) -> &'static str {
        "keychain"
    }
}

fn encode_key(key: &[u8; 32]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key)
}

fn decode_key(text: &str) -> Option<[u8; 32]> {
    base64::engine::general_purpose::STANDARD.decode(text.trim()).ok()?.try_into().ok()
}

// ---------------------------------------------------------------- key file

/// The master key in the owner-only file `<data folder>/master.key` (see the module documentation for the format and
/// the permissions).
#[derive(Debug, Clone)]
pub struct FileMasterKey {
    dir: PathBuf,
}

impl FileMasterKey {
    /// The key file of the data folder `dir`.
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }
    /// The file path.
    pub fn path(&self) -> PathBuf {
        self.dir.join(KEY_FILE)
    }
}

fn io_err(e: impl std::fmt::Display) -> SecretError {
    SecretError::Unavailable(format!("{KEY_FILE}: {e}"))
}

impl KeySlot for FileMasterKey {
    fn read(&self) -> Result<Option<[u8; 32]>, SecretError> {
        let path = self.path();
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err(e)),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(SecretError::KeyFile(KEY_FILE_READABLE.into()));
            }
        }
        #[cfg(not(unix))]
        let _ = meta;
        let text = std::fs::read_to_string(&path).map_err(io_err)?;
        decode_key(&text).map(Some).ok_or_else(|| io_err("the key file is damaged"))
    }

    fn store_new(&self, key: &[u8; 32]) -> Result<[u8; 32], SecretError> {
        std::fs::create_dir_all(&self.dir).map_err(io_err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700)).map_err(io_err)?;
        }
        let path = self.path();
        let tmp = self.dir.join(format!("{KEY_FILE}.{}.tmp", std::process::id()));
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(io_err)?;
            f.write_all(format!("{}\n", encode_key(key)).as_bytes()).map_err(io_err)?;
            f.sync_all().map_err(io_err)?;
        }
        // Never replace a key file another process wrote meanwhile: link the new file in only when none exists.
        let linked = std::fs::hard_link(&tmp, &path);
        let result = match linked {
            Ok(()) => Ok(*key),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                self.read().and_then(|k| k.ok_or_else(|| io_err("the key file disappeared")))
            }
            // File systems without hard links: a plain rename when the file is still missing.
            Err(_) if !path.exists() => std::fs::rename(&tmp, &path).map(|_| *key).map_err(io_err),
            Err(_) => self.read().and_then(|k| k.ok_or_else(|| io_err("the key file disappeared"))),
        };
        let _ = std::fs::remove_file(&tmp);
        result
    }

    fn remove(&self) -> Result<(), SecretError> {
        match std::fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err(e)),
        }
    }

    fn label(&self) -> &'static str {
        "key file"
    }
}

impl MasterKeySource for FileMasterKey {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        match self.read()? {
            Some(k) => Ok(k),
            None => self.store_new(&rand::random()),
        }
    }
}

// ---------------------------------------------------------------- the move

/// The master key in `target`, moved once from `old` when only the old store holds it (see the module documentation).
pub struct MigratingMasterKey {
    target: Arc<dyn KeySlot>,
    old: Arc<dyn KeySlot>,
}

impl MigratingMasterKey {
    pub fn new(target: Arc<dyn KeySlot>, old: Arc<dyn KeySlot>) -> Self {
        Self { target, old }
    }
}

impl MasterKeySource for MigratingMasterKey {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        if let Some(k) = self.target.read()? {
            return Ok(k);
        }
        // The new place is empty: the old one is asked once. A refusal keeps everything in place for the next start.
        match self.old.read()? {
            Some(k) => {
                let stored = self.target.store_new(&k)?;
                if stored != k {
                    // Another process created a different key meanwhile: keep the old one where it is.
                    return Err(SecretError::Unavailable("two master keys found; the old one was left in place".into()));
                }
                match self.old.remove() {
                    Ok(()) => log(&format!("master key moved from the {} to the {}", self.old.label(), self.target.label())),
                    Err(e) => log(&format!(
                        "master key copied from the {} to the {}; the old item could not be deleted: {e}",
                        self.old.label(),
                        self.target.label()
                    )),
                }
                Ok(k)
            }
            None => self.target.store_new(&rand::random()),
        }
    }
}

/// The master key source of the app and the CLI for the data folder `dir`: the store of [`selected_store`], with the
/// other store as the place a key is moved from. Decided on the first secret use, never at open.
pub struct AppMasterKey {
    dir: PathBuf,
    inner: OnceLock<MigratingMasterKey>,
}

impl AppMasterKey {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf(), inner: OnceLock::new() }
    }

    fn source(&self) -> &MigratingMasterKey {
        self.inner.get_or_init(|| {
            let file: Arc<dyn KeySlot> = Arc::new(FileMasterKey::new(&self.dir));
            let keychain: Arc<dyn KeySlot> =
                if keychain_backend_available() { Arc::new(KeychainMasterKey::new()) } else { Arc::new(NoSlot) };
            match selected_store() {
                KeyStoreKind::File => MigratingMasterKey::new(file, keychain),
                KeyStoreKind::Keychain => MigratingMasterKey::new(keychain, file),
            }
        })
    }
}

impl MasterKeySource for AppMasterKey {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        self.source().load_or_create()
    }
}

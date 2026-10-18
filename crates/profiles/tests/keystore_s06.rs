//! S-06 (change set 10): where the master key lives, by build. The key file store, the selection rule with an
//! injected signature, and the one-time move between stores with a fake keychain (no test touches a real keychain
//! item).

use mqttpanel_profiles::keystore::{choose_store, KeySlot, Platform, Signature, KEY_FILE_READABLE};
use mqttpanel_profiles::{
    FileMasterKey, FileSecrets, FixedMasterKey, KeyStoreKind, MasterKeySource, MigratingMasterKey, SecretError, SecretStore, KEY_FILE,
    SECRETS_FILE,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------- key file

#[test]
fn s06_the_key_file_is_created_owner_only_and_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let f = FileMasterKey::new(&dir);
    assert_eq!(f.read().unwrap(), None, "no file, no key");
    let k1 = f.load_or_create().unwrap();
    assert!(dir.join(KEY_FILE).exists());
    let k2 = FileMasterKey::new(&dir).load_or_create().unwrap();
    assert_eq!(k1, k2, "the same key on the next start");
    assert_eq!(f.read().unwrap(), Some(k1));
    // One line of base64 text, never the raw key next to other files.
    let text = std::fs::read_to_string(dir.join(KEY_FILE)).unwrap();
    assert_eq!(text.trim().len(), 44);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    }
    // No temporary file is left behind.
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec![KEY_FILE.to_string()]);
    // A second creation never replaces the key that is there.
    assert_eq!(f.store_new(&[9; 32]).unwrap(), k1);
    f.remove().unwrap();
    f.remove().unwrap();
    assert_eq!(f.read().unwrap(), None);
}

#[test]
fn s06_a_damaged_key_file_is_an_error_and_is_never_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let f = FileMasterKey::new(tmp.path());
    f.store_new(&[1; 32]).unwrap();
    std::fs::write(tmp.path().join(KEY_FILE), "not a key").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path().join(KEY_FILE), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(f.load_or_create().is_err());
    assert_eq!(std::fs::read_to_string(tmp.path().join(KEY_FILE)).unwrap(), "not a key");
}

#[cfg(unix)]
#[test]
fn s06_a_key_file_other_users_can_read_is_refused_with_a_plain_message() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let f = FileMasterKey::new(tmp.path());
    f.store_new(&[3; 32]).unwrap();
    for mode in [0o644, 0o640, 0o604] {
        std::fs::set_permissions(tmp.path().join(KEY_FILE), std::fs::Permissions::from_mode(mode)).unwrap();
        let e = f.load_or_create().unwrap_err();
        assert!(matches!(e, SecretError::KeyFile(_)), "{mode:o}: {e:?}");
        assert_eq!(e.to_string(), KEY_FILE_READABLE);
        assert_eq!(e.catalog_key(), "keychain.keyFile");
    }
    // The secrets file keeps that sentence (not "the keychain refused") and does not ask again in the session.
    let fs = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(f.clone()));
    assert_eq!(fs.set("a", "b").unwrap_err().catalog_key(), "keychain.keyFile");
    std::fs::set_permissions(tmp.path().join(KEY_FILE), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fs.set("a", "b").unwrap_err().catalog_key(), "keychain.keyFile", "remembered for the session");
    // After the fix and a restart the key is used again.
    assert_eq!(f.load_or_create().unwrap(), [3; 32]);
}

// ---------------------------------------------------------------- selection

fn never() -> Signature {
    panic!("the signature must not be read here")
}

#[test]
fn s06_the_store_follows_the_signature_on_macos() {
    let mac = Platform::MacOs;
    assert_eq!(choose_store(None, mac, || Signature::None), KeyStoreKind::File);
    assert_eq!(choose_store(None, mac, || Signature::AdHoc), KeyStoreKind::File);
    assert_eq!(choose_store(None, mac, || Signature::Identity { team_id: Some("ABCDE12345".into()) }), KeyStoreKind::Keychain);
    // A local self-signed identity (no team) counts as signed, so the keychain path can still be tested.
    assert_eq!(choose_store(None, mac, || Signature::Identity { team_id: None }), KeyStoreKind::Keychain);
}

#[test]
fn s06_windows_and_linux_keep_the_os_store() {
    assert_eq!(choose_store(None, Platform::Windows, never), KeyStoreKind::Keychain);
    assert_eq!(choose_store(None, Platform::Linux, never), KeyStoreKind::Keychain);
}

#[test]
fn s06_the_environment_override_wins() {
    for p in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        assert_eq!(choose_store(Some("file"), p, never), KeyStoreKind::File);
        assert_eq!(choose_store(Some("KEYCHAIN"), p, never), KeyStoreKind::Keychain);
        assert_eq!(choose_store(Some(" file "), p, never), KeyStoreKind::File);
    }
    // Anything else is ignored and the build decides.
    assert_eq!(choose_store(Some("vault"), Platform::MacOs, || Signature::AdHoc), KeyStoreKind::File);
    assert_eq!(choose_store(Some(""), Platform::MacOs, || Signature::Identity { team_id: None }), KeyStoreKind::Keychain);
    assert_eq!(KeyStoreKind::File.as_str(), "file");
    assert_eq!(KeyStoreKind::Keychain.as_str(), "keychain");
}

// ---------------------------------------------------------------- the move

/// A keychain stand-in: counts reads, writes and deletes; can refuse like Deny or a locked keychain.
#[derive(Default)]
struct FakeKeychain {
    key: Mutex<Option<[u8; 32]>>,
    refuse: AtomicBool,
    reads: AtomicUsize,
    writes: AtomicUsize,
    removes: AtomicUsize,
}

impl FakeKeychain {
    fn holding(key: [u8; 32]) -> Arc<Self> {
        let f = Self::default();
        *f.key.lock().unwrap() = Some(key);
        Arc::new(f)
    }
    fn count(&self) -> (usize, usize, usize) {
        (self.reads.load(Ordering::SeqCst), self.writes.load(Ordering::SeqCst), self.removes.load(Ordering::SeqCst))
    }
}

impl KeySlot for FakeKeychain {
    fn read(&self) -> Result<Option<[u8; 32]>, SecretError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.refuse.load(Ordering::SeqCst) {
            return Err(SecretError::Unavailable("user denied access".into()));
        }
        Ok(*self.key.lock().unwrap())
    }
    fn store_new(&self, key: &[u8; 32]) -> Result<[u8; 32], SecretError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        *self.key.lock().unwrap() = Some(*key);
        Ok(*key)
    }
    fn remove(&self) -> Result<(), SecretError> {
        self.removes.fetch_add(1, Ordering::SeqCst);
        *self.key.lock().unwrap() = None;
        Ok(())
    }
    fn label(&self) -> &'static str {
        "keychain"
    }
}

fn to_file(dir: &std::path::Path, old: &Arc<FakeKeychain>) -> MigratingMasterKey {
    MigratingMasterKey::new(Arc::new(FileMasterKey::new(dir)), old.clone())
}

#[test]
fn s06_the_key_moves_once_from_the_keychain_to_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let kc = FakeKeychain::holding([42; 32]);
    assert_eq!(to_file(tmp.path(), &kc).load_or_create().unwrap(), [42; 32]);
    assert_eq!(kc.count(), (1, 0, 1), "read once, never written, deleted");
    assert_eq!(*kc.key.lock().unwrap(), None);
    assert_eq!(FileMasterKey::new(tmp.path()).read().unwrap(), Some([42; 32]), "the file holds the same key");
    // The next start finds the file and never asks the keychain again.
    assert_eq!(to_file(tmp.path(), &kc).load_or_create().unwrap(), [42; 32]);
    assert_eq!(kc.count(), (1, 0, 1));
}

#[test]
fn s06_a_refused_keychain_leaves_everything_in_place_and_the_move_is_retried() {
    let tmp = tempfile::tempdir().unwrap();
    // secrets.enc written by an earlier build with the keychain key.
    let fs0 = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(FixedMasterKey::new([42; 32])));
    fs0.set("profile/a/password", "pw").unwrap();

    let kc = FakeKeychain::holding([42; 32]);
    kc.refuse.store(true, Ordering::SeqCst);
    let fs = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(to_file(tmp.path(), &kc)));
    for _ in 0..3 {
        let e = fs.get("profile/a/password").unwrap_err();
        assert_eq!(e.catalog_key(), "keychain.refused");
    }
    assert_eq!(kc.count(), (1, 0, 0), "asked once in the session, nothing deleted");
    assert!(!tmp.path().join(KEY_FILE).exists(), "no fresh key while the keychain still holds one");
    assert_eq!(*kc.key.lock().unwrap(), Some([42; 32]));

    // Next start, the user allows: the key moves and the old secrets still decrypt.
    kc.refuse.store(false, Ordering::SeqCst);
    let fs = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(to_file(tmp.path(), &kc)));
    assert_eq!(fs.get("profile/a/password").unwrap().as_deref(), Some("pw"));
    assert_eq!(kc.count(), (2, 0, 1));
    // And a later start reads secrets.enc through the key file alone.
    let fs = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(FileMasterKey::new(tmp.path())));
    assert_eq!(fs.get("profile/a/password").unwrap().as_deref(), Some("pw"));
}

#[test]
fn s06_the_key_moves_back_to_the_keychain_on_the_first_signed_start() {
    let tmp = tempfile::tempdir().unwrap();
    let file = FileMasterKey::new(tmp.path());
    file.store_new(&[7; 32]).unwrap();
    let fs0 = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(file.clone()));
    fs0.set("broker/ca-key", "KEY").unwrap();

    let kc = Arc::new(FakeKeychain::default());
    let src = MigratingMasterKey::new(kc.clone(), Arc::new(file.clone()));
    let fs = FileSecrets::new(tmp.path().join(SECRETS_FILE), Arc::new(src));
    assert_eq!(fs.get("broker/ca-key").unwrap().as_deref(), Some("KEY"));
    assert_eq!(*kc.key.lock().unwrap(), Some([7; 32]));
    assert_eq!(kc.count(), (1, 1, 0));
    assert!(!tmp.path().join(KEY_FILE).exists(), "the key file is deleted after the move");
}

#[test]
fn s06_a_fresh_key_only_when_no_store_holds_one() {
    let tmp = tempfile::tempdir().unwrap();
    let kc = Arc::new(FakeKeychain::default());
    let k = to_file(tmp.path(), &kc).load_or_create().unwrap();
    assert_eq!(kc.count(), (1, 0, 0), "the keychain was only looked at");
    assert_eq!(FileMasterKey::new(tmp.path()).read().unwrap(), Some(k));

    // A file already there wins; the keychain is not read at all, even if it holds another key.
    let kc2 = FakeKeychain::holding([5; 32]);
    assert_eq!(to_file(tmp.path(), &kc2).load_or_create().unwrap(), k);
    assert_eq!(kc2.count(), (0, 0, 0));
}

/// The real probe on this test program: cargo builds are ad-hoc signed by the linker (Apple silicon) or not signed at
/// all, so a dev build always lands on the key file.
#[cfg(target_os = "macos")]
#[test]
fn s06_a_cargo_build_reads_as_unsigned() {
    let sig = mqttpanel_profiles::keystore::own_signature();
    assert!(matches!(sig, Signature::AdHoc | Signature::None), "{sig:?}");
    assert_eq!(choose_store(None, Platform::MacOs, || sig.clone()), KeyStoreKind::File);
}

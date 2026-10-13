//! The secrets file (SPEC 6, change set 2, TESTPLAN V-09 at crate level): one master key, every secret encrypted in
//! `secrets.enc`, the key read lazily and once per session, a refusal remembered for the session (change set 3), no
//! migration of per-secret keychain entries.

use mqttpanel_profiles::{FileSecrets, FixedMasterKey, MasterKeySource, SecretError, SecretStore, Store, SECRETS_FILE};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn open(dir: &std::path::Path, key: [u8; 32]) -> (FileSecrets, Arc<FixedMasterKey>) {
    let src = Arc::new(FixedMasterKey::new(key));
    (FileSecrets::new(dir.join(SECRETS_FILE), src.clone()), src)
}

#[test]
fn v09_values_are_encrypted_with_a_nonce_per_record_and_the_key_is_read_once() {
    let dir = tempfile::tempdir().unwrap();
    let (fs, src) = open(dir.path(), [7; 32]);
    // Nothing touches the key before a secret value is needed.
    assert_eq!(fs.get("profile/a/password").unwrap(), None);
    assert!(!fs.contains("profile/a/password").unwrap());
    fs.delete("profile/a/password").unwrap();
    assert!(fs.names().unwrap().is_empty());
    assert_eq!(src.loads(), 0, "no master key for missing secrets, contains, delete or names");
    assert!(!fs.key_loaded());

    fs.set("profile/a/password", "s3cret-one").unwrap();
    fs.set("profile/b/password", "s3cret-one").unwrap();
    assert_eq!(src.loads(), 1);
    assert_eq!(fs.get("profile/a/password").unwrap().as_deref(), Some("s3cret-one"));
    assert_eq!(fs.get("profile/b/password").unwrap().as_deref(), Some("s3cret-one"));
    assert!(fs.contains("profile/a/password").unwrap());
    assert_eq!(src.loads(), 1, "cached for the session");

    let text = std::fs::read_to_string(dir.path().join(SECRETS_FILE)).unwrap();
    assert!(!text.contains("s3cret-one"), "values never in clear");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["cipher"], "xchacha20poly1305");
    let (a, b) = (&v["records"]["profile/a/password"], &v["records"]["profile/b/password"]);
    assert_ne!(a["nonce"], b["nonce"], "a nonce per record");
    assert_ne!(a["data"], b["data"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join(SECRETS_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "readable by the owner only");
    }

    // A second session (the CLI, or the app after a restart) reads the same file with the same key.
    let (fs2, src2) = open(dir.path(), [7; 32]);
    assert_eq!(fs2.get("profile/a/password").unwrap().as_deref(), Some("s3cret-one"));
    assert_eq!(src2.loads(), 1);
    fs2.delete("profile/a/password").unwrap();
    assert_eq!(fs.get("profile/a/password").unwrap(), None);
}

#[test]
fn records_are_authenticated_and_bound_to_their_name() {
    let dir = tempfile::tempdir().unwrap();
    let (fs, _) = open(dir.path(), [1; 32]);
    fs.set("broker/ca-key", "KEY-A").unwrap();
    fs.set("broker/server-key", "KEY-B").unwrap();
    let path = dir.path().join(SECRETS_FILE);
    // Swapping two records is detected.
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let a = v["records"]["broker/ca-key"].clone();
    v["records"]["broker/ca-key"] = v["records"]["broker/server-key"].clone();
    v["records"]["broker/server-key"] = a;
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(fs.get("broker/ca-key").is_err());
    // Another master key cannot read the file.
    fs.set("x", "y").unwrap();
    let (other, _) = open(dir.path(), [2; 32]);
    assert!(other.get("x").is_err());
    // An unknown version is refused, not misread.
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["version"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(fs.get("x").is_err());
}

/// A master key source that always fails, as when the user clicks Deny or the keychain is locked.
#[derive(Default)]
struct Refusing {
    calls: AtomicUsize,
}

impl MasterKeySource for Refusing {
    fn load_or_create(&self) -> Result<[u8; 32], SecretError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(SecretError::Unavailable("user denied access".into()))
    }
}

#[test]
fn v09_a_refused_master_key_is_asked_once_per_session() {
    let dir = tempfile::tempdir().unwrap();
    // A file with one record, written in an earlier session.
    let (fs0, _) = open(dir.path(), [5; 32]);
    fs0.set("profile/a/password", "pw").unwrap();

    let src = Arc::new(Refusing::default());
    let fs = FileSecrets::new(dir.path().join(SECRETS_FILE), src.clone());
    let attempts: Vec<Result<(), SecretError>> = vec![
        fs.get("profile/a/password").map(|_| ()),
        fs.set("profile/b/password", "x"),
        fs.get("profile/a/password").map(|_| ()),
        fs.set("broker/server-key", "y"),
        fs.get("profile/a/password").map(|_| ()),
    ];
    for r in attempts {
        let e = r.unwrap_err();
        assert!(matches!(e, SecretError::Refused(_)), "{e:?}");
        assert_eq!(e.catalog_key(), "keychain.refused");
    }
    assert_eq!(src.calls.load(Ordering::SeqCst), 1, "the keychain is asked exactly once");
    assert!(fs.key_refused());
    assert!(!fs.key_loaded());
    // Paths that need no key keep working and nothing was written.
    assert!(fs.contains("profile/a/password").unwrap());
    assert!(!fs.contains("profile/b/password").unwrap());
    assert_eq!(fs.get("profile/missing").unwrap(), None);
    assert_eq!(src.calls.load(Ordering::SeqCst), 1);
    // Other store errors keep their own sentence.
    assert_eq!(SecretError::Unavailable("x".into()).catalog_key(), "keychain.unavailable");
}

#[test]
fn v09_a_file_written_with_the_old_migrated_field_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let (fs, _) = open(dir.path(), [4; 32]);
    fs.set("profile/a/password", "pw").unwrap();
    let path = dir.path().join(SECRETS_FILE);
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["migrated"] = true.into();
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();

    let (fs2, src2) = open(dir.path(), [4; 32]);
    assert_eq!(fs2.names().unwrap(), vec!["profile/a/password".to_string()]);
    assert_eq!(fs2.get("profile/a/password").unwrap().as_deref(), Some("pw"));
    fs2.set("profile/b/password", "pw2").unwrap();
    assert_eq!(src2.loads(), 1);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("migrated"), "the field is no longer written");
}

#[test]
fn v09_opening_the_app_store_touches_no_keychain_item() {
    // `open_app_secrets` takes the data folder only: no legacy key list, no read at open.
    let dir = tempfile::tempdir().unwrap();
    let fs = mqttpanel_profiles::open_app_secrets(dir.path());
    assert!(!fs.key_loaded());
    assert!(!fs.key_refused());
    assert!(fs.names().unwrap().is_empty());
    assert!(!dir.path().join(SECRETS_FILE).exists(), "nothing written at open");
}

#[test]
fn the_profile_store_works_on_the_secrets_file_without_reading_the_key_for_listing() {
    let dir = tempfile::tempdir().unwrap();
    let src = Arc::new(FixedMasterKey::new([9; 32]));
    let fs: Arc<dyn SecretStore> = Arc::new(FileSecrets::new(dir.path().join(SECRETS_FILE), src.clone()));
    let store = Store::new(dir.path().to_path_buf(), fs.clone());
    let mut p = mqttpanel_profiles::Profile::new_default();
    p.name = "Plant".into();
    let p = store.save(p).unwrap();
    store.set_password(&p.id, "pw").unwrap();
    assert_eq!(src.loads(), 1);
    // A new session: listing and saving profiles never read the key; the password does, once.
    let src2 = Arc::new(FixedMasterKey::new([9; 32]));
    let fs2: Arc<dyn SecretStore> = Arc::new(FileSecrets::new(dir.path().join(SECRETS_FILE), src2.clone()));
    let store2 = Store::new(dir.path().to_path_buf(), fs2);
    let list = store2.profiles();
    assert!(list[0].has_password);
    store2.save(list[0].clone()).unwrap();
    assert!(store2.save(list[0].clone()).unwrap().has_password);
    assert_eq!(src2.loads(), 0, "start-up paths do not touch the keychain");
    assert_eq!(store2.password(&p.id).as_deref(), Some("pw"));
    assert_eq!(src2.loads(), 1);
}

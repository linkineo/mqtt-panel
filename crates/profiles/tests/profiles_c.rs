use mqttpanel_profiles::import::parse_import;
use mqttpanel_profiles::*;
use std::sync::Arc;

fn store() -> (tempfile::TempDir, Store, MemorySecrets) {
    let dir = tempfile::tempdir().unwrap();
    let secrets = MemorySecrets::new();
    let s = Store::new(dir.path().to_path_buf(), Arc::new(secrets.clone()));
    (dir, s, secrets)
}

#[test]
fn c01_round_trip_and_export_without_secrets() {
    let (dir, s, secrets) = store();
    let mut p = Profile::new_default();
    p.name = "Plant broker".into();
    p.username = "commissioning".into();
    let p = s.save(p).unwrap();
    assert_eq!(p.subscriptions, vec![Subscription { filter: "#".into(), qos: 0 }]);
    let p = s.set_password(&p.id, "s3cret-pw").unwrap();
    assert!(p.has_password);
    let reloaded = s.get(&p.id).unwrap().profile;
    assert_eq!(reloaded, p);
    // No secret on disk
    for f in std::fs::read_dir(dir.path()).unwrap() {
        let text = std::fs::read_to_string(f.unwrap().path()).unwrap();
        assert!(!text.contains("s3cret-pw"));
    }
    // Export without secrets, import back
    let exported = s.export(None);
    assert!(!String::from_utf8_lossy(&exported).contains("s3cret-pw"));
    let (_d2, s2, sec2) = store();
    let r = parse_import(&exported, None).unwrap();
    let names = s2.add_imported(r.profiles, r.secrets).unwrap();
    assert_eq!(names, vec!["Plant broker".to_string()]);
    assert!(sec2.is_empty());
    let imported = &s2.profiles()[0];
    assert_eq!(imported.username, "commissioning");
    assert!(!imported.has_password);
    // Export with passphrase carries the secret, encrypted
    let enc = s.export(Some("pass phrase"));
    assert!(!String::from_utf8_lossy(&enc).contains("s3cret-pw"));
    let (_d3, s3, _sec3) = store();
    let r = parse_import(&enc, Some("pass phrase")).unwrap();
    s3.add_imported(r.profiles, r.secrets).unwrap();
    let id = s3.profiles()[0].id.clone();
    assert_eq!(s3.password(&id).as_deref(), Some("s3cret-pw"));
    let r = parse_import(&enc, Some("wrong")).unwrap();
    assert!(r.secrets.is_empty());
    assert_eq!(r.notes[0].key, "import.passwordsSkipped");
    let _ = secrets;
}

#[test]
fn find_by_slug_duplicate_delete() {
    let (_d, s, secrets) = store();
    let mut p = Profile::new_default();
    p.name = "Plant broker".into();
    let p = s.save(p).unwrap();
    s.set_password(&p.id, "x").unwrap();
    assert_eq!(s.find("plant-broker").unwrap().profile.id, p.id);
    assert_eq!(s.find("PLANT BROKER").unwrap().profile.id, p.id);
    let copy = s.duplicate(&p.id).unwrap();
    assert_eq!(s.password(&copy.id).as_deref(), Some("x"));
    s.delete(&p.id).unwrap();
    assert!(s.get(&p.id).is_none());
    assert_eq!(secrets.len(), 1);
}

#[test]
fn settings_skip_verify_never_persisted() {
    let (_d, s, _) = store();
    let mut set = s.settings();
    set.skip_verify = true;
    set.buffer_per_topic = 5;
    let saved = s.save_settings(&set).unwrap();
    assert!(saved.skip_verify);
    assert_eq!(saved.buffer_per_topic, 10);
    assert!(!s.settings().skip_verify);
}

#[test]
fn templates_per_profile() {
    let (_d, s, _) = store();
    let t = s.save_template("p1", Template { id: String::new(), name: "Set room setpoint".into(), folder: "Commissioning".into(), request: serde_json::json!({"topic":"building/f2/{room}/setpoint"}), repeat_s: None }).unwrap();
    assert_eq!(s.templates("p1").len(), 1);
    assert!(s.templates("p2").is_empty());
    s.delete_template("p1", &t.id).unwrap();
    assert!(s.templates("p1").is_empty());
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import").join(name)).unwrap()
}

#[test]
fn c10_import_predecessor_settings() {
    let r = parse_import(&fixture("predecessor-settings.json"), None).unwrap();
    assert_eq!(r.profiles.len(), 2);
    let plant = r.profiles.iter().find(|p| p.profile.name == "Plant broker").unwrap();
    assert_eq!(plant.profile.scheme, "mqtts");
    assert_eq!(plant.profile.port, 8883);
    assert_eq!(plant.profile.subscriptions.len(), 2);
    assert_eq!(plant.profile.subscriptions[0].qos, 1);
    let ws = r.profiles.iter().find(|p| p.profile.name == "Dashboard WS").unwrap();
    assert_eq!(ws.profile.scheme, "ws");
    assert_eq!(ws.profile.ws_path, "/mqtt");
    assert!(r.secrets.is_empty());
    assert_eq!(r.notes[0].key, "import.passwordsSkipped");
    assert_eq!(r.skipped.len(), 1);
}

#[test]
fn c10_import_mqttx() {
    let r = parse_import(&fixture("mqttx-export.json"), None).unwrap();
    assert_eq!(r.profiles.len(), 2);
    let emqx = &r.profiles[0].profile;
    assert_eq!(emqx.scheme, "wss");
    assert_eq!(emqx.mqtt_version, "5");
    assert_eq!(emqx.last_will.as_ref().unwrap().topic, "clients/mqttx/status");
    let lab = &r.profiles[1].profile;
    assert_eq!(lab.subscriptions[0].filter, "#");
    assert!(r.secrets.is_empty());
}

#[test]
fn unknown_import_file() {
    assert_eq!(parse_import(b"{\"hello\":1}", None).unwrap_err().key, "file.unknownImport");
    assert_eq!(parse_import(b"not json", None).unwrap_err().key, "file.unknownImport");
}

#[test]
fn connect_options_from_profile() {
    let (_d, s, _) = store();
    let mut p = Profile::new_default();
    p.scheme = "mqtts".into();
    p.port = 8883;
    p.username = "u".into();
    p.mqtt_version = "5".into();
    let p = s.save(p).unwrap();
    s.set_password(&p.id, "pw").unwrap();
    let stored = s.get(&p.id).unwrap();
    let o = connect_options(&stored, s.secrets().as_ref(), false).unwrap();
    assert_eq!(o.scheme, mqttpanel_client::Scheme::Mqtts);
    assert_eq!(o.password.as_deref(), Some("pw"));
    assert_eq!(o.subscriptions, vec![("#".to_string(), 0)]);
}

/// C-02 against the real OS keychain: store, read and delete within one process (no access prompt).
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "needs an OS keychain (Secret Service on Linux); run with --ignored")]
fn c02_os_keychain_round_trip() {
    let k = KeychainSecrets::with_service("MQTT Panel test");
    let key = format!("test/{}", uuid_like());
    k.set(&key, "secret-value").unwrap();
    assert_eq!(k.get(&key).unwrap().as_deref(), Some("secret-value"));
    k.delete(&key).unwrap();
    assert_eq!(k.get(&key).unwrap(), None);
}

fn uuid_like() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

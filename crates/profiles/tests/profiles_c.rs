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

/// S-05: an old settings file with the app-wide `skipVerify` flag loads; the flag is ignored and not written back.
#[test]
fn s05_old_settings_skip_verify_ignored() {
    let (d, s, _) = store();
    std::fs::write(d.path().join("settings.json"), br#"{"theme":"dark","skipVerify":true,"bufferPerTopic":5}"#).unwrap();
    let set = s.settings();
    assert_eq!(set.theme, "dark");
    assert_eq!(set.buffer_per_topic, 10);
    s.save_settings(&set).unwrap();
    let raw = std::fs::read_to_string(d.path().join("settings.json")).unwrap();
    assert!(!raw.contains("skipVerify"));
}

/// S-05: an old profile with `verifyServer: false` loads with the per-connection skip switch on;
/// `verifyServer: true` (or absent) loads with it off; the file is written back with `skipVerify` only.
#[test]
fn s05_old_profile_verify_server_migrates() {
    let (d, s, _) = store();
    let old = r#"[
      {"id":"a","name":"Lab","host":"10.20.4.17","port":8883,"scheme":"mqtts","clientId":"c1","tls":{"verifyServer":false,"caCert":null,"clientCert":null,"clientKey":null}},
      {"id":"b","name":"Plant","host":"broker.plant.example","port":8883,"scheme":"mqtts","clientId":"c2","tls":{"verifyServer":true,"caCert":null,"clientCert":null,"clientKey":null}},
      {"id":"c","name":"Cloud","host":"demo.example","port":8883,"scheme":"mqtts","clientId":"c3"}
    ]"#;
    std::fs::write(d.path().join("profiles.json"), old).unwrap();
    assert!(s.get("a").unwrap().profile.tls.skip_verify);
    assert!(!s.get("b").unwrap().profile.tls.skip_verify);
    assert!(!s.get("c").unwrap().profile.tls.skip_verify);
    let p = s.get("a").unwrap().profile;
    s.save(p).unwrap();
    let raw = std::fs::read_to_string(d.path().join("profiles.json")).unwrap();
    assert!(raw.contains("\"skipVerify\": true"));
    assert!(!raw.contains("verifyServer"));
    // skipVerify wins over a stale verifyServer.
    let both: mqttpanel_profiles::model::TlsSettings = serde_json::from_str(r#"{"skipVerify":false,"verifyServer":false}"#).unwrap();
    assert!(!both.skip_verify);
}

/// Change set 11: `tls.clientKeyGenerated` defaults to false for profiles saved before it and round-trips.
#[test]
fn c33_client_key_generated_defaults_and_round_trips() {
    let old: mqttpanel_profiles::model::TlsSettings = serde_json::from_str(r#"{"skipVerify":false,"caCert":null,"clientCert":null,"clientKey":null}"#).unwrap();
    assert!(!old.client_key_generated);
    let mut t = old.clone();
    t.client_key_generated = true;
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"clientKeyGenerated\":true"), "{json}");
    let back: mqttpanel_profiles::model::TlsSettings = serde_json::from_str(&json).unwrap();
    assert!(back.client_key_generated);
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
    assert!(!o.tls.skip_verify && o.tls.verify_server);
    // "Only this time": skipped for this attempt, the profile unchanged.
    let once = connect_options(&stored, s.secrets().as_ref(), true).unwrap();
    assert!(once.tls.skip_verify);
    assert!(!s.get(&p.id).unwrap().profile.tls.skip_verify);
    // "Always for this connection".
    let mut always = stored.clone();
    always.profile.tls.skip_verify = true;
    assert!(connect_options(&always, s.secrets().as_ref(), false).unwrap().tls.skip_verify);
}

/// C-02 against the real OS keychain: the master key item (under a test service name) is created once, read back
/// unchanged, and a secret written with it is read by a second store; the test item is removed afterwards.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "needs an OS keychain (Secret Service on Linux); run with --ignored")]
fn c02_os_keychain_round_trip() {
    let service = format!("MQTT Panel test {}", uuid_like());
    let dir = tempfile::tempdir().unwrap();
    let fs = FileSecrets::new(dir.path().join(SECRETS_FILE), Arc::new(KeychainMasterKey::with_service(&service)));
    fs.set("test/secret", "secret-value").unwrap();
    let fs2 = FileSecrets::new(dir.path().join(SECRETS_FILE), Arc::new(KeychainMasterKey::with_service(&service)));
    let read = fs2.get("test/secret");
    let _ = keyring::Entry::new(&service, mqttpanel_profiles::secrets_file::MASTER_KEY_ACCOUNT).and_then(|e| e.delete_credential());
    assert_eq!(read.unwrap().as_deref(), Some("secret-value"));
}

fn uuid_like() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

/// Change set 8: the per-topic sparkline pick and topic chart survive a save and reload; old files load with none.
#[test]
fn c01_topics_round_trip() {
    let (_d, s, _secrets) = store();
    let mut p = Profile::new_default();
    p.topics.insert(
        "building/f2/ahu01/state".into(),
        TopicPrefs {
            sparkline_path: Some("$.supply.temp".into()),
            chart_fields: Some(vec![ChartField { path: "$.supply.fan".into(), colour: "clay".into(), style: "line".into(), axis: "right".into(), unit: "%".into() }]),
            sparkline_off: false,
        },
    );
    p.topics.insert("building/f2/r204/setpoint".into(), TopicPrefs { sparkline_path: Some("$.value".into()), chart_fields: None, sparkline_off: false });
    let p = s.save(p).unwrap();
    let reloaded = s.get(&p.id).unwrap().profile;
    assert_eq!(reloaded, p);
    assert_eq!(reloaded.sparkline_paths().get("building/f2/r204/setpoint").map(String::as_str), Some("$.value"));
    let json = serde_json::to_value(&reloaded).unwrap();
    assert_eq!(json["topics"]["building/f2/ahu01/state"]["sparklinePath"], "$.supply.temp");
    assert_eq!(json["topics"]["building/f2/ahu01/state"]["chartFields"][0]["colour"], "clay");
    assert!(json["topics"]["building/f2/r204/setpoint"].get("chartFields").is_none());
    // A profile saved before change set 8 has no `topics`.
    let mut old = json.clone();
    old.as_object_mut().unwrap().remove("topics");
    let back: Profile = serde_json::from_value(old).unwrap();
    assert!(back.topics.is_empty());
    // A chart field written with only its path takes the defaults.
    let f: ChartField = serde_json::from_str(r#"{"path":"$.value"}"#).unwrap();
    assert_eq!((f.colour.as_str(), f.style.as_str(), f.axis.as_str()), ("sage", "line", "left"));
}

/// Change set 9: a hidden sparkline (`sparklineOff`) survives a save and reload; it is left out of the file when off,
/// and an entry written without it loads with the sparkline shown.
#[test]
fn c01_sparkline_off_round_trip() {
    let (_d, s, _secrets) = store();
    let mut p = Profile::new_default();
    p.topics.insert("building/f2/r204/temperature".into(), TopicPrefs { sparkline_path: None, chart_fields: None, sparkline_off: true });
    p.topics.insert("building/f2/r204/setpoint".into(), TopicPrefs { sparkline_path: Some("$.value".into()), chart_fields: None, sparkline_off: false });
    let p = s.save(p).unwrap();
    let reloaded = s.get(&p.id).unwrap().profile;
    assert_eq!(reloaded, p);
    assert!(reloaded.topics["building/f2/r204/temperature"].sparkline_off);
    let json = serde_json::to_value(&reloaded).unwrap();
    assert_eq!(json["topics"]["building/f2/r204/temperature"]["sparklineOff"], true);
    assert!(json["topics"]["building/f2/r204/setpoint"].get("sparklineOff").is_none());
    let old: TopicPrefs = serde_json::from_str(r#"{"sparklinePath":"$.value"}"#).unwrap();
    assert!(!old.sparkline_off);
}

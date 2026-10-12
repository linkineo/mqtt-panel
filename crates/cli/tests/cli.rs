//! Integration tests of the command line (docs/TESTPLAN.md section 10, L-01 to L-06, and E-18).
//!
//! Brokers: the embedded broker through `mqttpanel broker --config` (always), and Mosquitto (F-03) when installed;
//! without it the Mosquitto parts print "SKIP: mosquitto not found". All data lives in temporary folders
//! (`MQTTPANEL_HOME`) and secrets in memory (`MQTTPANEL_SECRETS=memory`).

mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

const T: Duration = Duration::from_secs(20);

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn json_lines(out: &Out) -> Vec<serde_json::Value> {
    out.text()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}")))
        .collect()
}

fn ok(out: &Out) {
    assert_eq!(out.code, Some(0), "exit code; stderr: {}", out.stderr);
}

// ------------------------------------------------------------------ L-01

fn is_text_line(line: &str, topic: &str, payload: &str) -> bool {
    // HH:MM:SS.mmm topic payload
    let b = line.as_bytes();
    line.len() > 13
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b'.'
        && b[12] == b' '
        && line[..12].chars().filter(|c| c.is_ascii_digit()).count() == 9
        && line[13..] == format!("{topic} {payload}")
}

/// L-01 checks against a broker on 127.0.0.1:`port` (extra global args, e.g. a WebSocket URL, via `conn`).
fn l01_checks(port: u16, conn: &[&str]) {
    let h = home();
    let h = h.path();
    let p = prefix("l01");
    let port_s = port.to_string();
    let base: Vec<&str> = if conn.is_empty() {
        vec!["-h", "127.0.0.1", "-p", &port_s]
    } else {
        conn.to_vec()
    };
    let with = |extra: &[&str]| -> Vec<String> {
        base.iter()
            .chain(extra.iter())
            .map(|s| s.to_string())
            .collect()
    };
    let run_v = |v: Vec<String>| {
        let refs: Vec<&str> = v.iter().map(String::as_str).collect();
        run_args(h, &refs)
    };

    // Two retained messages: text and binary.
    let t1 = format!("{p}/a");
    let t2 = format!("{p}/b");
    ok(&run_v(with(&[
        "pub",
        "-t",
        &t1,
        "-m",
        "hello world",
        "-r",
        "-q",
        "1",
    ])));
    let bin_file = h.join("bin.dat");
    std::fs::write(&bin_file, [0u8, 159, 146, 150]).unwrap();
    ok(&run_v(with(&[
        "pub",
        "-t",
        &t2,
        "--file",
        bin_file.to_str().unwrap(),
        "-r",
        "-q",
        "1",
    ])));

    // text
    let out = run_v(with(&["sub", "-t", &t1, "--count", "1"]));
    ok(&out);
    let text = out.text();
    assert!(
        text.ends_with('\n') && text.lines().count() == 1,
        "{text:?}"
    );
    assert!(
        is_text_line(text.trim_end(), &t1, "hello world"),
        "{text:?}"
    );
    let out = run_v(with(&["sub", "-t", &t2, "--count", "1"]));
    ok(&out);
    assert!(
        is_text_line(out.text().trim_end(), &t2, "0x009f9296"),
        "{}",
        out.text()
    );

    // json: one pretty object per message
    let out = run_v(with(&[
        "--format", "json", "sub", "-t", &t1, "--count", "1",
    ]));
    ok(&out);
    assert!(out.text().starts_with("{\n  \"time\": "), "{}", out.text());
    let v: serde_json::Value = serde_json::from_str(&out.text()).unwrap();
    assert_eq!(v["topic"], t1.as_str());
    assert_eq!(v["payload"], "hello world");
    assert_eq!(v["retain"], true);

    // ndjson: one compact object per line, fields in order
    let out = run_v(with(&[
        "sub",
        "-t",
        &format!("{p}/#"),
        "--count",
        "2",
        "--format",
        "ndjson",
    ]));
    ok(&out);
    let lines = json_lines(&out);
    assert_eq!(lines.len(), 2, "{}", out.text());
    for l in out.text().lines() {
        assert!(l.starts_with("{\"time\":\""), "{l}");
        let keys: Vec<&str> = [
            "\"time\"",
            "\"topic\"",
            "\"payload\"",
            "\"qos\"",
            "\"retain\"",
        ]
        .to_vec();
        let mut at = 0;
        for k in keys {
            let i = l[at..]
                .find(k)
                .unwrap_or_else(|| panic!("{k} missing or out of order in {l}"));
            at += i;
        }
    }
    let text_msg = lines
        .iter()
        .find(|v| v["topic"] == t1.as_str())
        .expect("text message");
    assert_eq!(text_msg["payload"], "hello world");
    assert_eq!(text_msg["retain"], true);
    assert!(text_msg.get("payloadBase64").is_none());
    let bin_msg = lines
        .iter()
        .find(|v| v["topic"] == t2.as_str())
        .expect("binary message");
    assert!(bin_msg["payload"].is_null());
    assert_eq!(bin_msg["payloadBase64"], "AJ+Slg==");
    let time = text_msg["time"].as_str().unwrap();
    assert!(time.len() == 24 && time.ends_with('Z'), "{time}");

    // raw: payload bytes only
    let out = run_v(with(&["sub", "-t", &t2, "--count", "1", "--format", "raw"]));
    ok(&out);
    assert_eq!(out.stdout, vec![0u8, 159, 146, 150]);

    // --retained-only: both retained messages, then exit on its own
    let out = run_v(with(&[
        "sub",
        "-t",
        &format!("{p}/#"),
        "--retained-only",
        "--format",
        "ndjson",
    ]));
    ok(&out);
    assert_eq!(json_lines(&out).len(), 2);
    assert!(out.elapsed < Duration::from_secs(5), "{:?}", out.elapsed);
    // --retained-only on an empty topic: nothing, exit 0 after about a second
    let out = run_v(with(&[
        "sub",
        "-t",
        &format!("{p}/empty/#"),
        "--retained-only",
    ]));
    ok(&out);
    assert!(out.stdout.is_empty());

    // --timeout: exits after s seconds without a message
    let out = run_v(with(&[
        "sub",
        "-t",
        &format!("{p}/quiet"),
        "--timeout",
        "0.5",
    ]));
    ok(&out);
    assert!(out.stdout.is_empty());
    assert!(
        out.elapsed >= Duration::from_millis(450) && out.elapsed < Duration::from_secs(5),
        "{:?}",
        out.elapsed
    );

    // live messages and --count: exactly n messages, then exit
    let live = format!("{p}/live");
    let mut c = mp(h);
    c.env("MQTTPANEL_READY_HINT", "1").args(&base).args([
        "--format", "ndjson", "sub", "-t", &live, "--count", "2", "-q", "1",
    ]);
    let bg = Bg::spawn(c, false);
    bg.wait_stderr_line("ready", Duration::from_secs(10));
    for m in ["one", "two", "three"] {
        ok(&run_v(with(&["pub", "-t", &live, "-m", m, "-q", "1"])));
    }
    let out = bg.finish(T);
    ok(&out);
    let lines = json_lines(&out);
    assert_eq!(
        lines
            .iter()
            .map(|v| v["payload"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
    assert!(lines.iter().all(|v| v["retain"] == false && v["qos"] == 1));

    // Ctrl-C ends a subscription cleanly with exit 0.
    let mut c = mp(h);
    c.env("MQTTPANEL_READY_HINT", "1")
        .args(&base)
        .args(["sub", "-t", &format!("{p}/forever")]);
    let bg = Bg::spawn(c, false);
    bg.wait_stderr_line("ready", Duration::from_secs(10));
    bg.interrupt();
    let out = bg.finish(Duration::from_secs(10));
    assert_eq!(out.code, Some(0));
    assert_eq!(out.stderr, "ready\n");

    // cleanup
    ok(&run_v(with(&["pub", "-t", &t1, "--delete-retained"])));
    ok(&run_v(with(&["pub", "-t", &t2, "--delete-retained"])));
}

#[test]
fn l01_sub_formats() {
    let e = embedded_broker();
    l01_checks(e.port, &[]);
    e.bg.interrupt();
    let out = e.bg.finish(T);
    ok(&out);
    assert!(
        out.text()
            .contains(&format!("TCP listener on 127.0.0.1:{}", e.port)),
        "{}",
        out.text()
    );
    assert!(out.text().contains("Broker stopped."), "{}", out.text());

    if let Some(m) = mosquitto(false) {
        l01_checks(m.plain, &[]);
        // Same checks over WebSocket, host given as a URL.
        let url = format!("ws://127.0.0.1:{}/mqtt", m.ws);
        l01_checks(m.ws, &["-h", &url]);
    }
}

// ------------------------------------------------------------------ L-02

fn l02_checks(port: u16) {
    let h = home();
    let h = h.path();
    let p = prefix("l02");
    let port_s = port.to_string();
    let conn = ["-h", "127.0.0.1", "-p", port_s.as_str()];
    let pub_ = |args: &[&str], stdin: Option<&[u8]>| {
        let mut c = mp(h);
        c.args(conn).arg("pub").args(args);
        run(c, stdin, T)
    };
    let sub_bg = |args: &[&str]| {
        let mut all: Vec<&str> = conn.to_vec();
        all.extend_from_slice(args);
        let mut c = mp(h);
        c.env("MQTTPANEL_READY_HINT", "1")
            .args(&all[..4])
            .arg("sub")
            .args(&all[4..]);
        let bg = Bg::spawn(c, false);
        bg.wait_stderr_line("ready", Duration::from_secs(10));
        bg
    };

    // -m: exact bytes, printed nothing on success
    let t = format!("{p}/m");
    let bg = sub_bg(&["-t", &t, "--count", "1", "--format", "raw"]);
    let out = pub_(&["-t", &t, "-m", "{\"value\":21.5} ü"], None);
    ok(&out);
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(bg.finish(T).stdout, "{\"value\":21.5} ü".as_bytes());

    // --file: binary content with newlines, byte for byte
    let t = format!("{p}/file");
    let data: Vec<u8> = (0u8..=255).chain(b"\nline 2\r\n".iter().copied()).collect();
    let f = h.join("payload.bin");
    std::fs::write(&f, &data).unwrap();
    let bg = sub_bg(&["-t", &t, "--count", "1", "--format", "raw"]);
    ok(&pub_(
        &["-t", &t, "--file", f.to_str().unwrap(), "-q", "2"],
        None,
    ));
    assert_eq!(bg.finish(T).stdout, data);

    // --stdin: one message per line (LF and CRLF; empty lines skipped)
    let t = format!("{p}/stdin");
    let bg = sub_bg(&["-t", &t, "--count", "3", "--format", "ndjson"]);
    ok(&pub_(
        &["-t", &t, "--stdin", "-q", "1"],
        Some(b"first\nsecond line\r\n\nthird"),
    ));
    let out = bg.finish(T);
    let payloads: Vec<String> = json_lines(&out)
        .iter()
        .map(|v| v["payload"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(payloads, vec!["first", "second line", "third"]);

    // -r: a fresh subscriber gets the retained message
    let t = format!("{p}/retained");
    ok(&pub_(&["-t", &t, "-m", "kept", "-r", "-q", "1"], None));
    let mut c = mp(h);
    c.args(conn)
        .args(["sub", "-t", &t, "--retained-only", "--format", "ndjson"]);
    let out = run(c, None, T);
    ok(&out);
    let v = json_lines(&out);
    assert_eq!(v.len(), 1);
    assert_eq!(
        (v[0]["payload"].as_str(), v[0]["retain"].as_bool()),
        (Some("kept"), Some(true))
    );

    // --delete-retained (with --format json: status object), then a fresh subscriber sees nothing
    let out = pub_(&["-t", &t, "--delete-retained", "--format", "json"], None);
    ok(&out);
    let st: serde_json::Value = serde_json::from_str(&out.text()).unwrap();
    assert_eq!(st["ok"], true);
    assert_eq!(st["qos"], 1);
    assert_eq!(st["retain"], true);
    assert_eq!(st["acknowledged"], 1);
    let mut c = mp(h);
    c.args(conn).args(["sub", "-t", &t, "--retained-only"]);
    let out = run(c, None, T);
    ok(&out);
    assert!(out.stdout.is_empty(), "{}", out.text());

    // --repeat until SIGINT
    let t = format!("{p}/repeat");
    let bg = sub_bg(&["-t", &t, "--count", "3", "--format", "raw"]);
    let mut c = mp(h);
    c.args(conn)
        .args(["pub", "-t", &t, "-m", "tick;", "--repeat", "0.2", "-q", "1"]);
    let publisher = Bg::spawn(c, false);
    let got = bg.finish(T);
    ok(&got);
    assert_eq!(got.stdout, b"tick;tick;tick;");
    publisher.interrupt();
    let out = publisher.finish(Duration::from_secs(10));
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);

    // Bad arguments: no payload, two payload sources, wildcard topic
    let out = pub_(&["-t", &t], None);
    assert_eq!(
        (out.code, out.err()),
        (
            Some(2),
            "Give a payload with -m, --file or --stdin, or use --delete-retained."
        )
    );
    let out = pub_(&["-t", &t, "-m", "x", "--stdin"], None);
    assert_eq!(out.code, Some(2));
    assert_eq!(out.err().lines().count(), 1);
    let out = pub_(&["-t", "a/#", "-m", "x"], None);
    assert_eq!(out.code, Some(2));
}

#[test]
fn l02_pub_variants() {
    let e = embedded_broker();
    l02_checks(e.port);
    e.bg.interrupt();
    ok(&e.bg.finish(T));
    if let Some(m) = mosquitto(false) {
        l02_checks(m.plain);
    }
}

// ------------------------------------------------------------------ L-03

#[test]
fn l03_profile() {
    let e = embedded_broker();
    let login = embedded_broker_with_login();
    let h = home();
    let hp = h.path();
    let store = mqttpanel_profiles::Store::new(
        hp.to_path_buf(),
        std::sync::Arc::new(mqttpanel_profiles::MemorySecrets::new()),
    );
    let mut plant = mqttpanel_profiles::Profile::new_default();
    plant.name = "Plant broker".into();
    plant.host = "127.0.0.1".into();
    plant.port = e.port;
    plant.subscriptions = vec![];
    let plant = store.save(plant).unwrap();
    let mut secure = mqttpanel_profiles::Profile::new_default();
    secure.name = "Secure line".into();
    secure.host = "127.0.0.1".into();
    secure.port = login.port;
    secure.username = USER.into();
    secure.mqtt_version = "3.1.1".into();
    let secure = store.save(secure).unwrap();
    let p = prefix("l03");
    let t = format!("{p}/setpoint");

    // By slug and by name.
    ok(&run_args(
        hp,
        &[
            "pub",
            "--profile",
            "plant-broker",
            "-t",
            &t,
            "-m",
            "{\"value\":21.5}",
            "-q",
            "1",
            "-r",
        ],
    ));
    let out = run_args(
        hp,
        &[
            "sub",
            "--profile",
            "Plant broker",
            "-t",
            &t,
            "--retained-only",
            "--format",
            "ndjson",
        ],
    );
    ok(&out);
    assert_eq!(json_lines(&out)[0]["payload"], "{\"value\":21.5}");
    // --profile after the command, by id; explicit flags override profile fields (-p to a closed port).
    let closed = free_port();
    let out = run_args(
        hp,
        &[
            "sub",
            "-t",
            &t,
            "--retained-only",
            "--profile",
            &plant.id,
            "-h",
            "localhost",
            "-p",
            &closed.to_string(),
        ],
    );
    assert_eq!(out.code, Some(1));
    assert_eq!(
        out.err(),
        format!("Nothing is listening on localhost:{closed}. Is the broker running on that port?")
    );

    // Keychain password (seeded memory store): works without entering the secret.
    let seed =
        serde_json::json!({ format!("profile/{}/password", secure.id): PASSWORD }).to_string();
    let mut c = mp(hp);
    c.env("MQTTPANEL_SECRETS_SEED", &seed).args([
        "pub",
        "--profile",
        "secure-line",
        "-t",
        &t,
        "-m",
        "x",
        "-q",
        "1",
    ]);
    ok(&run(c, None, T));
    // Without the secret the broker refuses: exit 3.
    let out = run_args(
        hp,
        &["pub", "--profile", "secure-line", "-t", &t, "-m", "x"],
    );
    assert_eq!(out.code, Some(3), "{}", out.stderr);
    // -u with a different user asks for a password (MQTTPANEL_PASSWORD in scripts).
    let mut c = mp(hp);
    c.env("MQTTPANEL_PASSWORD", PASSWORD).args([
        "pub",
        "--profile",
        "secure-line",
        "-u",
        USER,
        "-t",
        &t,
        "-m",
        "y",
    ]);
    ok(&run(c, None, T));

    // Unknown profile: exit 2 with the catalog sentence.
    let out = run_args(hp, &["sub", "--profile", "nope", "-t", "#"]);
    assert_eq!(
        (out.code, out.err()),
        (
            Some(2),
            "No saved profile named nope. Run mqttpanel profiles list to see them."
        )
    );

    // profiles list (text and json)
    let out = run_args(hp, &["profiles", "list"]);
    ok(&out);
    let text = out.text();
    assert!(
        text.contains(&format!("Plant broker  mqtt://127.0.0.1:{}  auto", e.port)),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "Secure line   mqtt://127.0.0.1:{}  3.1.1",
            login.port
        )),
        "{text}"
    );
    let out = run_args(hp, &["profiles", "list", "--format", "json"]);
    ok(&out);
    let list: serde_json::Value = serde_json::from_str(&out.text()).unwrap();
    assert_eq!(list.as_array().map(Vec::len), Some(2));
    assert_eq!(list[0]["name"], "Plant broker");
    assert_eq!(list[0]["url"], format!("mqtt://127.0.0.1:{}", e.port));
    assert_eq!(list[0]["version"], "auto");

    // export without secrets, import into another home
    let out = run_args(hp, &["profiles", "export", "--without-secrets"]);
    ok(&out);
    assert!(!out.text().contains(PASSWORD));
    let export = hp.join("export.json");
    std::fs::write(&export, &out.stdout).unwrap();
    let h2 = home();
    let out = run_args(h2.path(), &["profiles", "import", export.to_str().unwrap()]);
    ok(&out);
    assert_eq!(out.text(), "Imported Plant broker\nImported Secure line\n");
    let out = run_args(h2.path(), &["profiles", "list"]);
    assert_eq!(out.text().lines().count(), 2);
    // export with a passphrase carries the encrypted password; import with it restores the secret
    let mut c = mp(hp);
    c.env("MQTTPANEL_SECRETS_SEED", &seed).args([
        "profiles",
        "export",
        "--passphrase",
        "correct horse",
    ]);
    let out = run(c, None, T);
    ok(&out);
    assert!(!out.text().contains(PASSWORD));
    assert!(out.text().contains("\"secrets\""));
    std::fs::write(&export, &out.stdout).unwrap();
    let out = run_args(h2.path(), &["profiles", "import", export.to_str().unwrap()]);
    ok(&out);
    assert_eq!(
        out.err(),
        "Passwords and keys were not imported. Enter them in each profile."
    );
    // Not an export file: exit 2.
    let junk = hp.join("junk.json");
    std::fs::write(&junk, b"[1,2,3]").unwrap();
    let out = run_args(h2.path(), &["profiles", "import", junk.to_str().unwrap()]);
    assert_eq!(
        (out.code, out.err()),
        (
            Some(2),
            "This file is not a connection export MQTT Panel can read."
        )
    );

    ok(&run_args(
        hp,
        &[
            "pub",
            "--profile",
            "plant-broker",
            "-t",
            &t,
            "--delete-retained",
        ],
    ));
    e.bg.interrupt();
    login.bg.interrupt();
}

// ------------------------------------------------------------------ L-04

#[test]
fn l04_tls_flags() {
    let Some(m) = mosquitto(true) else { return };
    let h = home();
    let hp = h.path();
    let d = m.dir.path();
    let f = |n: &str| d.join(n).display().to_string();
    let kit = m.kit.as_ref().unwrap();
    let ca_port = m.tls_ca.unwrap().to_string();
    let self_port = m.tls_self.unwrap().to_string();
    let mtls_port = m.mtls.unwrap().to_string();
    let t = prefix("l04");

    // --cafile: verified connection (TLS implied by the flag, as in the SPEC example).
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &ca_port,
            "--cafile",
            &f("root-ca.pem"),
            "-t",
            &t,
            "-m",
            "secure",
            "-q",
            "1",
        ],
    );
    ok(&out);
    // Without the CA: the trust summary, exit 1.
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &ca_port,
            "--tls",
            "-t",
            &t,
            "-m",
            "x",
        ],
    );
    assert_eq!(out.code, Some(1));
    assert!(
        out.stderr
            .starts_with("This broker presents a certificate MQTT Panel cannot verify.\n"),
        "{}",
        out.stderr
    );

    // Self-signed: certificate summary, fingerprint and the hint; then --trust-fingerprint and --insecure work.
    let fp = mqttpanel_tls::fingerprint_sha256(&kit.self_signed.cert_der);
    let out = run_args(
        hp,
        &[
            "sub",
            "-h",
            "localhost",
            "-p",
            &self_port,
            "--tls",
            "-t",
            &t,
            "--timeout",
            "0.3",
        ],
    );
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    let err = out.err();
    assert!(err.starts_with("This broker presents a certificate MQTT Panel cannot verify.\nReason: the certificate is self-signed."), "{err}");
    assert!(
        err.contains(&format!("  Fingerprint (SHA-256): {fp}")),
        "{err}"
    );
    assert!(
        err.contains("  Subject: ") && err.contains("mosquitto-lab"),
        "{err}"
    );
    assert!(
        err.ends_with(&format!(
            "run again with --trust-fingerprint {fp}, or with --insecure in a lab."
        )),
        "{err}"
    );
    let out = run_args(
        hp,
        &[
            "sub",
            "-h",
            "localhost",
            "-p",
            &self_port,
            "--trust-fingerprint",
            &fp,
            "-t",
            &t,
            "--timeout",
            "0.3",
        ],
    );
    ok(&out);
    let compact = fp.replace(':', "").to_lowercase();
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &self_port,
            "--tls",
            "--trust-fingerprint",
            &compact,
            "-t",
            &t,
            "-m",
            "x",
            "-q",
            "1",
        ],
    );
    ok(&out);
    let wrong = "00:".repeat(31) + "00";
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &self_port,
            "--trust-fingerprint",
            &wrong,
            "-t",
            &t,
            "-m",
            "x",
        ],
    );
    assert_eq!(out.code, Some(1));
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &self_port,
            "--insecure",
            "-t",
            &t,
            "-m",
            "x",
            "-q",
            "1",
        ],
    );
    ok(&out);

    // Mutual TLS: PEM certificate and key, PKCS#12 with passphrase, wrong passphrase (exit 2).
    let base = [
        "pub",
        "-h",
        "localhost",
        "-p",
        mtls_port.as_str(),
        "--cafile",
    ];
    let ca = f("root-ca.pem");
    let out = run_args(
        hp,
        &[
            &base[..],
            &[
                ca.as_str(),
                "--cert",
                &f("client-ok.pem"),
                "--key",
                &f("client-ok.key"),
                "-t",
                &t,
                "-m",
                "x",
                "-q",
                "1",
            ],
        ]
        .concat(),
    );
    ok(&out);
    let out = run_args(
        hp,
        &[
            &base[..],
            &[
                ca.as_str(),
                "--cert",
                &f("client.p12"),
                "--key-passphrase",
                "test-passphrase",
                "-t",
                &t,
                "-m",
                "x",
                "-q",
                "1",
            ],
        ]
        .concat(),
    );
    ok(&out);
    let out = run_args(
        hp,
        &[
            &base[..],
            &[
                ca.as_str(),
                "--cert",
                &f("client.p12"),
                "--key-passphrase",
                "wrong",
                "-t",
                &t,
                "-m",
                "x",
            ],
        ]
        .concat(),
    );
    assert_eq!(
        (out.code, out.err()),
        (Some(2), "The passphrase for client.p12 is wrong.")
    );
    let out = run_args(
        hp,
        &[&base[..], &[ca.as_str(), "-t", &t, "-m", "x"]].concat(),
    );
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    assert_eq!(out.err().lines().count(), 1, "{}", out.stderr);
    // --key without --cert, unreadable CA file: bad arguments.
    let out = run_args(
        hp,
        &["pub", "--key", &f("client-ok.key"), "-t", &t, "-m", "x"],
    );
    assert_eq!(
        (out.code, out.err()),
        (
            Some(2),
            "--key needs --cert. Run mqttpanel pub --help for the options."
        )
    );
    let out = run_args(
        hp,
        &["pub", "--cafile", &f("missing.pem"), "-t", &t, "-m", "x"],
    );
    assert_eq!((out.code, out.err()), (Some(2), "Cannot read missing.pem."));
    let out = run_args(
        hp,
        &["pub", "--cafile", &f("client-ok.key"), "-t", &t, "-m", "x"],
    );
    assert_eq!(
        (out.code, out.err()),
        (Some(2), "This file is not a certificate.")
    );
}

// ------------------------------------------------------------------ L-05

fn spec_help(command: &str) -> String {
    let spec = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/SPEC.md"))
        .expect("docs/SPEC.md");
    let block = spec
        .split("### Help\n```text\n")
        .nth(1)
        .and_then(|s| s.split("\n```").next())
        .expect("help block");
    let marker = format!("$ {command}\n");
    let start = block.find(&marker).expect("command in help block") + marker.len();
    let rest = &block[start..];
    let end = rest.find("\n$ ").map(|i| i + 1).unwrap_or(rest.len());
    let mut text = rest[..end].trim_end_matches('\n').to_string();
    text.push('\n');
    text
}

#[test]
fn l05_help_texts() {
    let h = home();
    let hp = h.path();
    for (cmd, args) in [
        ("mqttpanel --help", vec!["--help"]),
        ("mqttpanel sub --help", vec!["sub", "--help"]),
        ("mqttpanel pub --help", vec!["pub", "--help"]),
    ] {
        let out = run_args(hp, &args);
        ok(&out);
        assert_eq!(out.text(), spec_help(cmd), "{cmd}");
        assert!(out.stderr.is_empty());
    }
    // --help after any command and its options, even incomplete ones.
    assert_eq!(
        run_args(hp, &["sub", "-t", "x", "--help"]).text(),
        spec_help("mqttpanel sub --help")
    );
    assert_eq!(
        run_args(hp, &["-h", "10.0.0.1", "pub", "-t", "x", "--help"]).text(),
        spec_help("mqttpanel pub --help")
    );
    assert_eq!(
        run_args(hp, &["--help", "sub"]).text(),
        spec_help("mqttpanel sub --help")
    );
    let tls = run_args(hp, &["sub", "--help", "tls"]);
    ok(&tls);
    for opt in [
        "--cafile",
        "--cert",
        "--key",
        "--insecure",
        "--trust-fingerprint",
        "--sni",
        "--alpn",
        "PKCS#12",
    ] {
        assert!(tls.text().contains(opt), "{opt}");
    }
    let broker = run_args(hp, &["broker", "--help"]);
    ok(&broker);
    assert!(broker
        .text()
        .starts_with("Run the embedded broker\n\nUsage: mqttpanel broker [options]\n"));
    let profiles = run_args(hp, &["profiles", "export", "--help"]);
    ok(&profiles);
    assert!(profiles.text().starts_with(
        "List, export and import saved connection profiles\n\nUsage: mqttpanel profiles"
    ));
    // No arguments: the main help.
    assert_eq!(run_args(hp, &[]).text(), spec_help("mqttpanel --help"));
    // Version.
    let out = run_args(hp, &["-V"]);
    assert_eq!(
        out.text(),
        format!("mqttpanel {}\n", env!("CARGO_PKG_VERSION"))
    );

    // Unknown flag or bad value: exit 2, one line with the hint.
    let cases: [(&[&str], &str); 6] = [
        (&["sub", "--bogus"], "Unknown option --bogus. Run mqttpanel sub --help for the options."),
        (&["pub", "-q", "3", "-t", "x", "-m", "y"], "Invalid value 3 for --qos. Run mqttpanel pub --help for the options."),
        (&["--format", "xml", "sub", "-t", "x"], "Invalid value xml for --format. Run mqttpanel sub --help for the options."),
        (&["frobnicate"], "Unknown command frobnicate. Run mqttpanel --help for the options."),
        (&["sub"], "Give at least one topic filter with -t. Run mqttpanel sub --help for the options."),
        (&["sub", "-t", "x", "--timeout", "0"], "--timeout needs a number of seconds greater than 0. Run mqttpanel sub --help for the options."),
    ];
    for (args, expected) in cases {
        let out = run_args(hp, args);
        assert_eq!(out.code, Some(2), "{args:?}");
        assert_eq!(out.stderr, format!("{expected}\n"), "{args:?}");
        assert!(out.stdout.is_empty());
    }
    // In French and German too (catalog cli.badArgs).
    let mut c = mp(hp);
    c.env("LC_ALL", "fr_FR.UTF-8").args(["sub", "--bogus"]);
    let out = run(c, None, T);
    assert_eq!(
        out.err(),
        "Option inconnue\u{202f}: --bogus. Lancez mqttpanel sub --help pour voir les options."
    );
    let mut c = mp(hp);
    c.env("LC_ALL", "de_DE.UTF-8").args(["sub", "--bogus"]);
    let out = run(c, None, T);
    assert_eq!(
        out.err(),
        "Unbekannte Option --bogus. Führe mqttpanel sub --help aus, um die Optionen zu sehen."
    );
}

// ------------------------------------------------------------------ E-18

/// A fake broker that answers the first CONNECT with the given CONNACK bytes.
fn fake_broker(connack: &'static [u8]) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().take(4) {
            let Ok(mut s) = s else { continue };
            let mut buf = [0u8; 512];
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            if s.read(&mut buf).unwrap_or(0) > 0 {
                let _ = s.write_all(connack);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    });
    port
}

#[test]
fn e18_exit_codes() {
    let h = home();
    let hp = h.path();
    // E-01: DNS failure.
    let out = run_args(hp, &["sub", "-h", "no-such-host.invalid", "-t", "#"]);
    assert_eq!(
        (out.code, out.stderr.as_str()),
        (
            Some(1),
            "Cannot find the host no-such-host.invalid. Check the name or your network.\n"
        )
    );
    // E-02: closed port.
    let closed = free_port();
    let out = run_args(
        hp,
        &[
            "pub",
            "-h",
            "localhost",
            "-p",
            &closed.to_string(),
            "-t",
            "x",
            "-m",
            "y",
        ],
    );
    assert_eq!(out.code, Some(1));
    assert_eq!(
        out.stderr,
        format!(
            "Nothing is listening on localhost:{closed}. Is the broker running on that port?\n"
        )
    );
    // E-07: not authorised (MQTT 5 CONNACK 0x87 from a fake broker).
    let port = fake_broker(&[0x20, 0x03, 0x00, 0x87, 0x00]);
    let out = run_args(
        hp,
        &["sub", "-h", "127.0.0.1", "-p", &port.to_string(), "-t", "#"],
    );
    assert_eq!(out.code, Some(3));
    assert_eq!(
        out.stderr,
        "The broker does not allow this client to connect (not authorized). Reason code 0x87.\n"
    );

    // E-06: bad password against the embedded broker (MQTT 5 and 3.1.1 via auto) and Mosquitto.
    let e = embedded_broker_with_login();
    let mut c = mp(hp);
    c.env("MQTTPANEL_PASSWORD", "wrong").args([
        "sub",
        "-h",
        "127.0.0.1",
        "-p",
        &e.port.to_string(),
        "-u",
        USER,
        "-t",
        "#",
    ]);
    let out = run(c, None, T);
    assert_eq!(out.code, Some(3), "{}", out.stderr);
    assert_eq!(out.stderr, "The broker refused the login for user commissioning (bad username or password). Reason code 0x86.\n");
    let mut c = mp(hp);
    c.env("MQTTPANEL_PASSWORD", PASSWORD).args([
        "pub",
        "-h",
        "127.0.0.1",
        "-p",
        &e.port.to_string(),
        "-u",
        USER,
        "-t",
        "x",
        "-m",
        "y",
        "-q",
        "1",
    ]);
    ok(&run(c, None, T));
    e.bg.interrupt();

    if let Some(m) = mosquitto(false) {
        if let Some(p) = m.passwd {
            let mut c = mp(hp);
            c.env("MQTTPANEL_PASSWORD", "wrong").args([
                "sub",
                "-h",
                "127.0.0.1",
                "-p",
                &p.to_string(),
                "-u",
                USER,
                "-t",
                "#",
            ]);
            let out = run(c, None, T);
            assert_eq!(out.code, Some(3), "{}", out.stderr);
            // Mosquitto answers a wrong password on MQTT 5 with 0x87 (not authorized), on 3.1.1 with 0x05.
            assert!(
                out.stderr.starts_with("The broker refused the login for user commissioning (bad username or password). Reason code 0x")
                    || out.stderr == "The broker does not allow this client to connect (not authorized). Reason code 0x87.\n",
                "{}",
                out.stderr
            );
            assert_eq!(out.stderr.lines().count(), 1);
            // Anonymous on a password listener: refused, exit 3.
            let out = run_args(
                hp,
                &["sub", "-h", "127.0.0.1", "-p", &p.to_string(), "-t", "#"],
            );
            assert_eq!(out.code, Some(3), "{}", out.stderr);
            assert_eq!(out.stderr.lines().count(), 1);
            let mut c = mp(hp);
            c.env("MQTTPANEL_PASSWORD", PASSWORD).args([
                "pub",
                "-h",
                "127.0.0.1",
                "-p",
                &p.to_string(),
                "-u",
                USER,
                "-t",
                "x",
                "-m",
                "y",
                "-q",
                "1",
            ]);
            ok(&run(c, None, T));
        }
    }
}

// ------------------------------------------------------------------ L-06

/// Public broker (F-05): sub/pub on test.mosquitto.org:1883 under mqttpanel-test/<random>/, at most 5 messages,
/// no retained message left behind. Runs only with MQTTPANEL_PUBLIC_BROKERS=1 (`cargo test -- --ignored`).
#[test]
#[ignore = "public broker; set MQTTPANEL_PUBLIC_BROKERS=1 and run with --ignored"]
fn l06_public_brokers() {
    if std::env::var("MQTTPANEL_PUBLIC_BROKERS").as_deref() != Ok("1") {
        println!("SKIP: MQTTPANEL_PUBLIC_BROKERS is not 1");
        return;
    }
    let h = home();
    let hp = h.path();
    let p = prefix("l06");
    let conn = ["-h", "test.mosquitto.org", "-p", "1883"];
    // Reachability: a broker that does not answer is a SKIP, not a failure.
    let probe = run_args(
        hp,
        &[
            &conn[..],
            &["sub", "-t", &format!("{p}/probe"), "--timeout", "0.5"],
        ]
        .concat(),
    );
    if probe.code != Some(0) {
        println!(
            "SKIP: test.mosquitto.org unreachable: {}",
            probe.stderr.trim_end()
        );
        return;
    }
    let t = format!("{p}/data");
    let mut c = mp(hp);
    c.env("MQTTPANEL_READY_HINT", "1").args(conn).args([
        "sub",
        "-t",
        &format!("{p}/#"),
        "--count",
        "3",
        "--format",
        "ndjson",
        "--timeout",
        "10",
    ]);
    let bg = Bg::spawn(c, false);
    bg.wait_stderr_line("ready", Duration::from_secs(15));
    ok(&run(
        {
            let mut c = mp(hp);
            c.args(conn).args(["pub", "-t", &t, "--stdin", "-q", "1"]);
            c
        },
        Some(b"m1\nm2\n"),
        T,
    ));
    // One retained message (counts as the third), deleted right after (fifth message in total).
    let r = format!("{p}/retained");
    ok(&run_args(
        hp,
        &[&conn[..], &["pub", "-t", &r, "-m", "m3", "-r", "-q", "1"]].concat(),
    ));
    let out = bg.finish(Duration::from_secs(30));
    ok(&run_args(
        hp,
        &[&conn[..], &["pub", "-t", &r, "--delete-retained"]].concat(),
    ));
    ok(&out);
    let payloads: Vec<String> = json_lines(&out)
        .iter()
        .map(|v| v["payload"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(payloads, vec!["m1", "m2", "m3"]);
}

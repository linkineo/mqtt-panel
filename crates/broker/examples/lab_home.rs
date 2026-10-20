//! Test lab helper (T-31, change set 11): prepares and changes a temporary MQTT Panel home for `mqttpanel broker`, the
//! way the desktop app would, and writes out what an external device gets (CA.pem, issued certificates).
//!
//! The CLI cannot issue certificates, so `tools/testlab/external_clients.py` calls this helper between broker runs.
//! Secrets live in `<home>/lab-secrets.json` (lab only, a temporary folder) and reach `mqttpanel broker` through
//! `MQTTPANEL_SECRETS=memory` and `MQTTPANEL_SECRETS_SEED`.
//!
//! ```text
//! cargo run -p mqttpanel-broker --example lab_home -- <home> setup <tls port> <wss port>
//! cargo run -p mqttpanel-broker --example lab_home -- <home> require on|off
//! cargo run -p mqttpanel-broker --example lab_home -- <home> issue <name> pem|p12 [passphrase]
//! cargo run -p mqttpanel-broker --example lab_home -- <home> revoke <name>
//! cargo run -p mqttpanel-broker --example lab_home -- <home> rotate
//! cargo run -p mqttpanel-broker --example lab_home -- <home> regen <extra name>...
//! cargo run -p mqttpanel-broker --example lab_home -- <home> other-ca
//! ```
//! Every command prints one JSON object (the automatic and extra names, the server names, the files written).

use mqttpanel_broker::config::*;
use mqttpanel_broker::BrokerManager;
use mqttpanel_profiles::{MemorySecrets, SecretStore};
use mqttpanel_tls::kit::KeyAlg;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SECRET_KEYS: [&str; 2] = ["broker/ca-key", "broker/server-key"];

fn load_secrets(home: &Path) -> MemorySecrets {
    let s = MemorySecrets::new();
    if let Ok(text) = std::fs::read_to_string(home.join("lab-secrets.json")) {
        let map: BTreeMap<String, String> = serde_json::from_str(&text).unwrap_or_default();
        for (k, v) in map {
            let _ = s.set(&k, &v);
        }
    }
    s
}

fn save_secrets(home: &Path, s: &MemorySecrets) {
    let mut map = BTreeMap::new();
    for k in SECRET_KEYS {
        if let Ok(Some(v)) = s.get(k) {
            map.insert(k.to_string(), v);
        }
    }
    std::fs::write(home.join("lab-secrets.json"), serde_json::to_string(&map).unwrap()).unwrap();
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("lab_home: {msg}");
    std::process::exit(2)
}

fn write(home: &Path, name: &str, data: &[u8], written: &mut Vec<String>) {
    std::fs::write(home.join(name), data).unwrap_or_else(|e| fail(format!("{name}: {e}")));
    written.push(name.to_string());
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(home), Some(cmd)) = (args.first(), args.get(1)) else { fail("usage: lab_home <home> <command> [args]") };
    let home = PathBuf::from(home);
    std::fs::create_dir_all(&home).unwrap_or_else(|e| fail(e));
    let secrets = load_secrets(&home);
    let m = BrokerManager::open(&home.join("broker"), Arc::new(secrets.clone())).unwrap_or_else(|e| fail(e.user_message().text.clone()));
    let rest = &args[2..];
    let mut written = Vec::new();
    let err = |e: mqttpanel_broker::BrokerError| -> ! { fail(e.user_message().text.clone()) };
    match cmd.as_str() {
        "setup" => {
            let port = |i: usize| rest.get(i).and_then(|p| p.parse::<u16>().ok()).unwrap_or_else(|| fail("setup <tls port> <wss port>"));
            m.set_bind(Bind::Network).await.unwrap_or_else(|e| err(e));
            for mut l in m.config().listeners {
                match l.kind {
                    ListenerKind::Tls => {
                        l.port = port(0);
                        l.enabled = true;
                    }
                    ListenerKind::Wss => {
                        l.port = port(1);
                        l.enabled = true;
                    }
                    _ => l.enabled = false,
                }
                m.update_listener(l).await.unwrap_or_else(|e| err(e));
            }
            m.set_anonymous(true).unwrap_or_else(|e| err(e));
            m.set_require_client_cert(false).unwrap_or_else(|e| err(e));
        }
        "require" => {
            let on = rest.first().map(String::as_str) == Some("on");
            m.set_anonymous(!on).unwrap_or_else(|e| err(e));
            m.set_require_client_cert(on).unwrap_or_else(|e| err(e));
        }
        "issue" => {
            let name = rest.first().unwrap_or_else(|| fail("issue <name> pem|p12 [passphrase]"));
            let format = if rest.get(1).map(String::as_str) == Some("p12") { CertFormat::Pkcs12 } else { CertFormat::Pem };
            let passphrase = rest.get(2).cloned().unwrap_or_default();
            let files = m
                .issue_client_cert(IssueCertRequest { name: name.clone(), valid_years: 1, key_type: KeyAlg::Ecdsa, format, passphrase, save_ca: false })
                .unwrap_or_else(|e| err(e));
            for (n, data) in files {
                write(&home, &n, &data, &mut written);
            }
        }
        "revoke" => {
            m.revoke_client_cert(rest.first().unwrap_or_else(|| fail("revoke <name>"))).unwrap_or_else(|e| err(e));
        }
        "rotate" => {
            m.rotate_ca().unwrap_or_else(|e| err(e));
        }
        "regen" => {
            m.regenerate_with_extra(rest, KeyAlg::Ecdsa).unwrap_or_else(|e| err(e));
        }
        "other-ca" => {
            let ca = mqttpanel_tls::kit::Ca::new_root("Unrelated Lab CA", KeyAlg::Ecdsa, mqttpanel_tls::kit::Validity::years(1)).unwrap();
            write(&home, "other-CA.pem", ca.cert_pem.as_bytes(), &mut written);
        }
        other => fail(format!("unknown command {other}")),
    }
    let (ca_name, ca) = m.export_ca();
    write(&home, &ca_name, &ca, &mut written);
    let (chain_name, chain) = m.export_server_chain();
    write(&home, &chain_name, &chain, &mut written);
    save_secrets(&home, &secrets);
    let certs = m.broker_certs().unwrap_or_else(|e| err(e));
    let seed: BTreeMap<String, String> = SECRET_KEYS.iter().filter_map(|k| secrets.get(k).ok().flatten().map(|v| (k.to_string(), v))).collect();
    println!(
        "{}",
        serde_json::json!({
            "autoSans": certs.auto_sans,
            "extraSans": certs.extra_sans,
            "serverSans": certs.server.san,
            "files": written,
            "secretsSeed": serde_json::to_string(&seed).unwrap(),
        })
    );
}

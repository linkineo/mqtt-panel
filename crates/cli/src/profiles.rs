//! `mqttpanel profiles list | export | import`: the desktop app's saved connection profiles.

use crate::args::{Format, Global, ProfilesCommand};
use crate::conn;
use crate::i18n::{t, Failure};
use mqttpanel_client::catalog::Lang;
use mqttpanel_profiles::StoredProfile;
use serde_json::json;
use std::io::Write;

fn url(p: &StoredProfile) -> String {
    let pr = &p.profile;
    let host = if pr.host.contains(':') && !pr.host.starts_with('[') {
        format!("[{}]", pr.host)
    } else {
        pr.host.clone()
    };
    let path = if pr.scheme.starts_with("ws") {
        pr.ws_path.clone()
    } else {
        String::new()
    };
    format!("{}://{}:{}{}", pr.scheme, host, pr.port, path)
}

fn version(p: &StoredProfile) -> String {
    if p.profile.mqtt_version.is_empty() {
        "auto".into()
    } else {
        p.profile.mqtt_version.clone()
    }
}

fn write_out(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes).and_then(|_| out.flush());
}

pub fn run(g: &Global, cmd: Option<&ProfilesCommand>, lang: Lang) -> Result<(), Failure> {
    let store = conn::store();
    match cmd.unwrap_or(&ProfilesCommand::List) {
        ProfilesCommand::List => {
            let all = store.list();
            match g.format() {
                Format::Json => {
                    let list: Vec<_> = all.iter().map(|p| json!({"id": p.profile.id, "name": p.profile.name, "url": url(p), "version": version(p)})).collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&list).unwrap_or_default()
                    );
                }
                Format::Ndjson => {
                    for p in &all {
                        println!(
                            "{}",
                            json!({"id": p.profile.id, "name": p.profile.name, "url": url(p), "version": version(p)})
                        );
                    }
                }
                Format::Text | Format::Raw => {
                    if all.is_empty() {
                        eprintln!("{}", t(lang, "profiles.none", &[]));
                    }
                    let rows: Vec<(String, String, String)> = all
                        .iter()
                        .map(|p| (p.profile.name.clone(), url(p), version(p)))
                        .collect();
                    let w0 = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
                    let w1 = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
                    let mut text = String::new();
                    for (n, u, v) in rows {
                        let pad0 = " ".repeat(w0 - n.chars().count());
                        let pad1 = " ".repeat(w1 - u.chars().count());
                        text.push_str(&format!("{n}{pad0}  {u}{pad1}  {v}\n"));
                    }
                    write_out(text.as_bytes());
                }
            }
            Ok(())
        }
        ProfilesCommand::Export { passphrase, .. } => {
            let mut data = store.export(passphrase.as_deref().filter(|p| !p.is_empty()));
            data.push(b'\n');
            write_out(&data);
            Ok(())
        }
        ProfilesCommand::Import { file, passphrase } => {
            let bytes = conn::read_file(file, lang)?;
            let parsed = mqttpanel_profiles::import::parse_import(&bytes, passphrase.as_deref())
                .map_err(|m| Failure::msg(&m, lang))?;
            let names = store
                .add_imported(parsed.profiles, parsed.secrets)
                .map_err(|m| Failure::msg(&m, lang))?;
            for n in &names {
                println!("{}", t(lang, "profiles.imported", &[("name", n)]));
            }
            for (_, m) in &parsed.skipped {
                eprintln!("{}", m.render(lang));
            }
            for m in &parsed.notes {
                eprintln!("{}", m.render(lang));
            }
            Ok(())
        }
    }
}

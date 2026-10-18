//! From the command line (and an optional saved profile) to client options; connection failures to exit codes.

use crate::args::{Global, TlsArgs};
use crate::i18n::{cat, t, Failure, EXIT_ARGS, EXIT_CONNECTION};
use mqttpanel_client::catalog::Lang;
use mqttpanel_client::{msg, ConnectFailure, ConnectOptions, Scheme, TrustInfo};
use mqttpanel_profiles::{MemorySecrets, SecretStore, Store};
use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

/// The secret store, shared with the desktop app: `secrets.enc` in the data folder with its master key where the build
/// says (change set 10: the key file `master.key` on unsigned builds, the OS keychain on signed ones; read on first
/// secret use only). `MQTTPANEL_KEYSTORE=file|keychain` forces the store, as in the app.
///
/// Test hook: `MQTTPANEL_SECRETS=memory` switches to an in-memory store (nothing touches the keychain), optionally
/// seeded from `MQTTPANEL_SECRETS_SEED`, a JSON object of secret keys and values
/// (for example `{"profile/<id>/password":"…"}`). Both variables are read only when set.
pub fn secret_store() -> Arc<dyn SecretStore> {
    if std::env::var("MQTTPANEL_SECRETS").as_deref() == Ok("memory") {
        let mem = MemorySecrets::new();
        if let Ok(seed) = std::env::var("MQTTPANEL_SECRETS_SEED") {
            if let Ok(map) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(&seed)
            {
                for (k, v) in map {
                    let _ = mem.set(&k, &v);
                }
            }
        }
        return Arc::new(mem);
    }
    let dir = mqttpanel_profiles::default_dir();
    mqttpanel_profiles::open_app_secrets(&dir)
}

/// The profile store of the desktop app (`MQTTPANEL_HOME` overrides the folder).
pub fn store() -> Store {
    Store::new(mqttpanel_profiles::default_dir(), secret_store())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

/// Reads a file named on the command line; unreadable files are a bad argument.
pub fn read_file(p: &Path, lang: Lang) -> Result<Vec<u8>, Failure> {
    std::fs::read(p).map_err(|_| Failure::msg(&msg!("file.unreadable", file = file_name(p)), lang))
}

/// `--host` accepts a host name, an IP address, or a URL such as `mqtts://host:8883` or `ws://host:8083/mqtt`.
struct HostSpec {
    host: String,
    scheme: Option<Scheme>,
    port: Option<u16>,
    path: Option<String>,
}

fn parse_host(s: &str) -> Option<HostSpec> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let Some((scheme, rest)) = s.split_once("://") else {
        return Some(HostSpec {
            host: s.to_string(),
            scheme: None,
            port: None,
            path: None,
        });
    };
    let scheme = Scheme::parse(&scheme.to_ascii_lowercase())?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], Some(rest[i..].to_string()).filter(|p| p != "/")),
        None => (rest, None),
    };
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let (h, tail) = after.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(p) => Some(p.parse::<u16>().ok()?),
            None if tail.is_empty() => None,
            None => return None,
        };
        (h.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p.parse::<u16>().ok()?)),
            None => (authority.to_string(), None),
        }
    };
    if host.is_empty() || port == Some(0) {
        return None;
    }
    Some(HostSpec {
        host,
        scheme: Some(scheme),
        port,
        path,
    })
}

fn default_port(s: Scheme) -> u16 {
    match s {
        Scheme::Mqtt => 1883,
        Scheme::Mqtts => 8883,
        Scheme::Ws => 8083,
        Scheme::Wss => 8084,
    }
}

/// Password for `user`: `MQTTPANEL_PASSWORD` when set (scripts), else a prompt on the terminal (never echoed).
/// Without a terminal the password stays empty.
fn ask_password(user: &str, lang: Lang) -> Option<String> {
    if let Ok(p) = std::env::var("MQTTPANEL_PASSWORD") {
        return Some(p);
    }
    if !(std::io::stdin().is_terminal() || std::io::stderr().is_terminal()) {
        return None;
    }
    let prompt = cat(
        lang,
        "cli.passwordPrompt",
        &[("user", serde_json::json!(user))],
    );
    rpassword::prompt_password(prompt)
        .ok()
        .filter(|p| !p.is_empty())
}

/// Builds the client options: the profile first (when `--profile` is given), then the explicit flags on top.
pub fn build(
    g: &Global,
    tls: &TlsArgs,
    command: &str,
    lang: Lang,
) -> Result<ConnectOptions, Failure> {
    let from_profile = g.profile.is_some();
    let mut o = match &g.profile {
        Some(name) => {
            let st = store();
            let sp = st
                .find(name)
                .ok_or_else(|| Failure::msg(&msg!("cli.profileNotFound", name = name), lang))?;
            mqttpanel_profiles::connect_options(&sp, st.secrets().as_ref(), tls.insecure)
                .map_err(|m| Failure::msg(&m, lang))?
        }
        None => {
            let mut o = ConnectOptions::new("localhost", 1883);
            o.subscriptions.clear();
            o
        }
    };
    let mut explicit_port = false;
    if let Some(h) = &g.host {
        let spec = parse_host(h)
            .ok_or_else(|| Failure::args_t("args.badHost", &[("arg", h)], command, lang))?;
        o.host = spec.host;
        if let Some(s) = spec.scheme {
            o.scheme = s;
            o.port = default_port(s);
        }
        if let Some(p) = spec.port {
            o.port = p;
            explicit_port = true;
        }
        if let Some(path) = spec.path {
            o.ws_path = path;
        }
    }
    if g.tls || tls.any() {
        let was_plain = !o.scheme.is_tls();
        o.scheme = if o.scheme.is_ws() {
            Scheme::Wss
        } else {
            Scheme::Mqtts
        };
        if was_plain && !explicit_port && (o.port == 1883 || o.port == 8083) {
            o.port = default_port(o.scheme);
        }
    }
    if let Some(p) = g.port {
        o.port = p;
    }
    apply_tls(&mut o, tls, command, lang)?;
    if let Some(u) = &g.user {
        let same_as_profile =
            from_profile && o.username.as_deref() == Some(u.as_str()) && o.password.is_some();
        o.username = Some(u.clone());
        if !same_as_profile {
            o.password = ask_password(u, lang);
        }
    }
    if !from_profile {
        o.reconnect = false;
    }
    Ok(o)
}

fn apply_tls(
    o: &mut ConnectOptions,
    a: &TlsArgs,
    command: &str,
    lang: Lang,
) -> Result<(), Failure> {
    if let Some(ca) = &a.cafile {
        let bytes = read_file(ca, lang)?;
        if mqttpanel_tls::parse_certs(&bytes).is_err() {
            return Err(Failure::msg(&msg!("file.notCertificate"), lang));
        }
        o.tls.ca = Some(bytes);
    }
    match (&a.cert, &a.key) {
        (None, Some(_)) => return Err(Failure::args_t("args.keyWithoutCert", &[], command, lang)),
        (Some(cert), key) => {
            let cert_bytes = read_file(cert, lang)?;
            let key_bytes = match key {
                Some(k) => Some(read_file(k, lang)?),
                None => None,
            };
            let named = key
                .as_deref()
                .filter(|_| !mqttpanel_tls::looks_like_pkcs12(&cert_bytes))
                .unwrap_or(cert);
            let id = mqttpanel_tls::load_identity(
                &cert_bytes,
                key_bytes.as_deref(),
                a.key_passphrase.as_deref(),
            )
            .map_err(|e| {
                let m = match e {
                    mqttpanel_tls::TlsFileError::WrongPassphrase
                    | mqttpanel_tls::TlsFileError::NeedsPassphrase => {
                        msg!("tls.wrongPassphrase", file = file_name(named))
                    }
                    other => msg!(other.catalog_key()),
                };
                Failure {
                    code: EXIT_ARGS,
                    text: m.render(lang),
                }
            })?;
            o.tls.identity = Some(id);
        }
        (None, None) => {}
    }
    for fp in &a.trust_fingerprint {
        o.tls
            .trusted_fingerprints
            .push(mqttpanel_tls::normalize_fingerprint(fp));
    }
    if a.insecure {
        o.tls.skip_verify = true;
    }
    if let Some(s) = &a.sni {
        o.tls.sni = Some(s.clone());
    }
    if let Some(s) = &a.alpn {
        o.tls.alpn = Some(s.clone());
    }
    Ok(())
}

/// A failed connection: one sentence, or the trust summary for an unverifiable certificate.
pub fn connect_failure(f: &ConnectFailure, lang: Lang) -> Failure {
    match f {
        ConnectFailure::Message(m) => Failure::msg(m, lang),
        ConnectFailure::Trust(info) => Failure {
            code: EXIT_CONNECTION,
            text: trust_text(info, lang),
        },
    }
}

/// The certificate summary printed when the broker's certificate cannot be verified.
pub fn trust_text(info: &TrustInfo, lang: Lang) -> String {
    let leaf = &info.leaf;
    let fp = leaf.fingerprint.clone();
    let mut lines = vec![
        cat(lang, "tls.untrusted", &[]),
        info.reason_text.render(lang),
    ];
    lines.push(format!(
        "  {}: {}",
        t(lang, "trust.subject", &[]),
        leaf.subject
    ));
    lines.push(format!(
        "  {}: {}",
        t(lang, "trust.issuer", &[]),
        leaf.issuer
    ));
    if !leaf.san.is_empty() {
        lines.push(format!(
            "  {}: {}",
            t(lang, "trust.names", &[]),
            leaf.san.join(", ")
        ));
    }
    lines.push(format!(
        "  {}: {} – {}",
        t(lang, "trust.valid", &[]),
        leaf.not_before,
        leaf.not_after
    ));
    lines.push(format!("  {}: {}", t(lang, "trust.fingerprint", &[]), fp));
    lines.push(t(lang, "trust.hint", &[("fp", &fp)]));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_forms() {
        let h = parse_host("10.20.4.17").unwrap();
        assert_eq!(
            (h.host.as_str(), h.scheme, h.port),
            ("10.20.4.17", None, None)
        );
        let h = parse_host("mqtts://broker.example.net").unwrap();
        assert_eq!(
            (h.host.as_str(), h.scheme, h.port),
            ("broker.example.net", Some(Scheme::Mqtts), None)
        );
        let h = parse_host("ws://[::1]:9001/mqtt").unwrap();
        assert_eq!(
            (h.host.as_str(), h.scheme, h.port, h.path.as_deref()),
            ("::1", Some(Scheme::Ws), Some(9001), Some("/mqtt"))
        );
        assert!(parse_host("gopher://x").is_none());
        assert!(parse_host("mqtt://x:99999").is_none());
        assert!(parse_host("").is_none());
    }

    #[test]
    fn tls_flags_switch_port() {
        let g = Global {
            tls: true,
            ..Default::default()
        };
        let o = build(&g, &TlsArgs::default(), "sub", Lang::En).unwrap();
        assert_eq!((o.scheme, o.port), (Scheme::Mqtts, 8883));
        let g = Global {
            tls: true,
            port: Some(1883),
            ..Default::default()
        };
        let o = build(&g, &TlsArgs::default(), "sub", Lang::En).unwrap();
        assert_eq!(o.port, 1883);
        let a = TlsArgs {
            insecure: true,
            trust_fingerprint: vec!["3f9a".into()],
            ..Default::default()
        };
        let o = build(&Global::default(), &a, "sub", Lang::En).unwrap();
        assert!(o.tls.skip_verify);
        assert_eq!(o.tls.trusted_fingerprints, vec!["3F:9A".to_string()]);
        assert_eq!(o.port, 8883);
    }
}

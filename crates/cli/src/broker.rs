//! `mqttpanel broker`: runs the embedded broker in the foreground until Ctrl-C.
//!
//! Without `--config` it uses the desktop app's broker settings (the `broker` folder of the app data dir and the keychain), created with the
//! defaults on first use; with `--config` it reads a TOML file (format in `mqttpanel_broker::toml_config`).

use crate::args::{BrokerArgs, Format, Global};
use crate::conn;
use crate::i18n::{t, Failure, EXIT_ARGS, EXIT_CONNECTION};
use chrono::{DateTime, Local, SecondsFormat, Utc};
use mqttpanel_broker::{Broker, BrokerError, BrokerManager, LogLevel, LogLine, RuntimeConfig};
use mqttpanel_client::catalog::{render_with, Lang};
use serde_json::json;
use std::io::Write;
use std::net::IpAddr;
use std::time::Duration;

fn broker_failure(e: &BrokerError, lang: Lang) -> Failure {
    let m = e.user_message();
    let code = if m.key.starts_with("file.") || m.key.starts_with("tls.") {
        EXIT_ARGS
    } else {
        EXIT_CONNECTION
    };
    Failure {
        code,
        text: render_with(lang, &m.key, &m.params),
    }
}

fn level_word(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn emit(line: String) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}").and_then(|_| out.flush());
}

fn log_line(format: Format, l: &LogLine) -> String {
    let at: DateTime<Utc> = DateTime::from_timestamp_millis(l.at).unwrap_or_else(Utc::now);
    match format {
        Format::Json | Format::Ndjson => {
            json!({"time": at.to_rfc3339_opts(SecondsFormat::Millis, true), "level": level_word(l.level), "text": l.text}).to_string()
        }
        Format::Text | Format::Raw => {
            let local: DateTime<Local> = at.into();
            format!("{} {:<5} {}", local.format("%H:%M:%S%.3f"), level_word(l.level), l.text)
        }
    }
}

fn parse_bind(s: &str) -> Option<IpAddr> {
    match s.trim() {
        "localhost" => Some(IpAddr::from([127, 0, 0, 1])),
        other => other
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse()
            .ok(),
    }
}

pub async fn run(g: &Global, a: &BrokerArgs, lang: Lang) -> Result<(), Failure> {
    let bind = match &a.bind {
        Some(b) => Some(
            parse_bind(b)
                .ok_or_else(|| Failure::args_t("args.badBind", &[("arg", b)], "broker", lang))?,
        ),
        None => None,
    };
    // The manager stays alive while the broker runs (it owns the log buffer and the revocation list).
    let (cfg, _manager) = match &a.config {
        Some(path) => (
            RuntimeConfig::from_toml_file(path).map_err(|e| broker_failure(&e, lang))?,
            None,
        ),
        None => {
            let m = BrokerManager::open(
                &mqttpanel_profiles::default_dir().join("broker"),
                conn::secret_store(),
            )
            .map_err(|e| broker_failure(&e, lang))?;
            let cfg = RuntimeConfig::from_manager(&m).map_err(|e| broker_failure(&e, lang))?;
            (cfg, Some(m))
        }
    };
    let cfg = match bind {
        Some(ip) => cfg.with_bind(ip),
        None => cfg,
    };
    let format = g.format();
    let handle = Broker::start(cfg)
        .await
        .map_err(|e| broker_failure(&e, lang))?;

    for l in handle.listeners() {
        match &l.error {
            Some(e) => eprintln!("{}", broker_failure(e, lang).text),
            None => {
                for addr in &l.addrs {
                    let line = match format {
                        Format::Json | Format::Ndjson => {
                            json!({"listener": l.id, "kind": l.kind.label(), "address": addr.to_string(), "port": addr.port()}).to_string()
                        }
                        _ => t(lang, "broker.listening", &[("kind", l.kind.label()), ("addr", &addr.to_string())]),
                    };
                    emit(line);
                }
            }
        }
    }
    if matches!(format, Format::Text | Format::Raw) {
        emit(t(lang, "broker.running", &[]));
    }

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let mut last = 0u64;
    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
            _ = tick.tick() => {
                for l in handle.log(LogLevel::Info, last) {
                    last = l.seq;
                    emit(log_line(format, &l));
                }
            }
        }
    }
    for l in handle.log(LogLevel::Info, last) {
        emit(log_line(format, &l));
    }
    handle.stop().await;
    if matches!(format, Format::Text | Format::Raw) {
        emit(t(lang, "broker.stopped", &[]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_bind;

    #[test]
    fn bind_forms() {
        assert_eq!(parse_bind("127.0.0.1"), Some([127, 0, 0, 1].into()));
        assert_eq!(parse_bind("localhost"), Some([127, 0, 0, 1].into()));
        assert!(parse_bind("[::1]").is_some());
        assert!(parse_bind("nope").is_none());
    }
}

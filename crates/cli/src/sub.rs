//! `mqttpanel sub`: subscribe and print messages.

use crate::args::{Global, SubArgs};
use crate::conn;
use crate::i18n::{cat, Failure};
use crate::output;
use mqttpanel_client::catalog::Lang;
use mqttpanel_client::ClientEvent;
use std::io::Write;
use std::time::Duration;
use tokio::time::Instant;

/// With `--retained-only`: exit when no retained message arrived for this long after subscribing.
const RETAINED_QUIET: Duration = Duration::from_secs(1);

fn seconds(v: Option<f64>, arg: &str, lang: Lang) -> Result<Option<Duration>, Failure> {
    match v {
        None => Ok(None),
        Some(s) if s.is_finite() && s > 0.0 && s < 1e9 => Ok(Some(Duration::from_secs_f64(s))),
        Some(_) => Err(Failure::args_t(
            "args.positiveSeconds",
            &[("arg", arg)],
            "sub",
            lang,
        )),
    }
}

/// Writes to stdout; false when the reader went away (closed pipe), which ends the command quietly.
fn emit(bytes: &[u8]) -> bool {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes).and_then(|_| out.flush()).is_ok()
}

pub async fn run(g: &Global, a: &SubArgs, lang: Lang) -> Result<(), Failure> {
    let idle = seconds(a.timeout, "--timeout", lang)?;
    let mut o = conn::build(g, &a.tls, "sub", lang)?;
    if !a.topics.is_empty() {
        o.subscriptions = a.topics.iter().map(|t| (t.clone(), a.qos)).collect();
    }
    if o.subscriptions.is_empty() || o.subscriptions.iter().any(|(f, _)| f.is_empty()) {
        return Err(Failure::args_t("args.noTopic", &[], "sub", lang));
    }
    // A long-running subscription survives network drops; one-shot reads do not retry.
    o.reconnect = !a.retained_only && a.count.is_none() && a.timeout.is_none();
    let reconnect = o.reconnect;
    let wanted = o.subscriptions.len();
    let format = g.format();

    let (handle, _info, mut rx) = mqttpanel_client::connect(o)
        .await
        .map_err(|f| conn::connect_failure(&f, lang))?;
    let ready_hint = std::env::var("MQTTPANEL_READY_HINT").is_ok();

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut idle_deadline = idle.map(|d| Instant::now() + d);
    let mut retained_deadline: Option<Instant> = None;
    let mut acked = 0usize;
    let mut printed = 0u64;

    let result = loop {
        let next = match (idle_deadline, retained_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let sleep = async move {
            match next {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _ = &mut ctrl_c => break Ok(()),
            _ = sleep => break Ok(()),
            ev = rx.recv() => match ev {
                None | Some(ClientEvent::Closed) => break Err(Failure::msg(&mqttpanel_client::msg!("error.notConnected"), lang)),
                Some(ClientEvent::Message(m)) => {
                    if a.retained_only && !m.retain {
                        break Ok(());
                    }
                    if !emit(&output::render(format, &m, chrono::Utc::now())) {
                        break Ok(());
                    }
                    printed += 1;
                    if a.count.is_some_and(|n| printed >= n) {
                        break Ok(());
                    }
                    idle_deadline = idle.map(|d| Instant::now() + d);
                    if a.retained_only {
                        retained_deadline = Some(Instant::now() + RETAINED_QUIET);
                    }
                }
                Some(ClientEvent::Subscribed { result: Err(m), .. }) => break Err(Failure::msg(&m, lang)),
                Some(ClientEvent::Subscribed { result: Ok(_), .. }) => {
                    acked += 1;
                    if acked == wanted {
                        if ready_hint {
                            eprintln!("ready");
                        }
                        if a.retained_only && retained_deadline.is_none() {
                            retained_deadline = Some(Instant::now() + RETAINED_QUIET);
                        }
                    }
                }
                Some(ClientEvent::Lost(m)) => {
                    if !reconnect {
                        break Err(Failure::msg(&m, lang));
                    }
                    eprintln!("{}", m.render(lang));
                }
                Some(ClientEvent::Reconnecting { attempt }) => {
                    eprintln!("{}", cat(lang, "error.reconnecting", &[("attempt", serde_json::json!(attempt))]));
                }
                Some(ClientEvent::Connected(_)) => acked = 0,
                Some(ClientEvent::Failed(f)) => break Err(conn::connect_failure(&f, lang)),
                Some(ClientEvent::Published { .. }) => {}
            }
        }
    };
    handle.disconnect().await;
    result
}

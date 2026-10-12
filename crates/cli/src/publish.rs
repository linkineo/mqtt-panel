//! `mqttpanel pub`: publish from `-m`, `--file` or `--stdin`, optionally repeated; waits for PUBACK/PUBCOMP.

use crate::args::{Format, Global, PubArgs};
use crate::conn;
use crate::i18n::Failure;
use mqttpanel_client::catalog::Lang;
use mqttpanel_client::{msg, ClientEvent, ClientHandle, Outgoing};
use std::collections::HashSet;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;

enum Source {
    Bytes(Vec<u8>),
    Stdin,
}

/// Status object printed with `--format json` (fields in this order).
#[derive(serde::Serialize)]
struct Status<'a> {
    ok: bool,
    topic: &'a str,
    qos: u8,
    retain: bool,
    sent: u64,
    acknowledged: u64,
}

struct Counters {
    sent: u64,
    done: u64,
}

/// Handles one client event; returns an error that ends the command.
fn on_event(
    ev: Option<ClientEvent>,
    pending: &mut HashSet<u64>,
    c: &mut Counters,
    lang: Lang,
) -> Result<(), Failure> {
    match ev {
        None | Some(ClientEvent::Closed) => Err(Failure::msg(&msg!("error.notConnected"), lang)),
        Some(ClientEvent::Published { token, result }) => {
            pending.remove(&token);
            match result {
                Ok(_) => {
                    c.done += 1;
                    Ok(())
                }
                Err(m) => Err(Failure::msg(&m, lang)),
            }
        }
        Some(ClientEvent::Lost(m)) => Err(Failure::msg(&m, lang)),
        Some(ClientEvent::Failed(f)) => Err(conn::connect_failure(&f, lang)),
        Some(_) => Ok(()),
    }
}

/// Waits until every queued message is written (QoS 0) or acknowledged (QoS 1 and 2). False on Ctrl-C.
async fn drain(
    rx: &mut mpsc::Receiver<ClientEvent>,
    pending: &mut HashSet<u64>,
    c: &mut Counters,
    ctrl_c: &mut std::pin::Pin<&mut impl std::future::Future<Output = std::io::Result<()>>>,
    lang: Lang,
) -> Result<bool, Failure> {
    while !pending.is_empty() {
        tokio::select! {
            _ = ctrl_c.as_mut() => return Ok(false),
            ev = rx.recv() => on_event(ev, pending, c, lang)?,
        }
    }
    Ok(true)
}

fn send(
    handle: &ClientHandle,
    topic: &str,
    payload: Vec<u8>,
    qos: u8,
    retain: bool,
    pending: &mut HashSet<u64>,
    c: &mut Counters,
) {
    let token = handle.publish(Outgoing {
        topic: topic.to_string(),
        payload,
        qos,
        retain,
        props: None,
    });
    pending.insert(token);
    c.sent += 1;
}

pub async fn run(g: &Global, a: &PubArgs, lang: Lang) -> Result<(), Failure> {
    let cmd = "pub";
    let topic = match a.topic.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return Err(Failure::args_t("args.noPubTopic", &[], cmd, lang)),
    };
    if topic.contains('+') || topic.contains('#') {
        return Err(Failure::args_t(
            "args.wildcardTopic",
            &[("arg", &topic)],
            cmd,
            lang,
        ));
    }
    let sources = [
        a.message.is_some(),
        a.file.is_some(),
        a.stdin,
        a.delete_retained,
    ]
    .iter()
    .filter(|x| **x)
    .count();
    if sources == 0 {
        return Err(Failure::msg(&msg!("cli.noPayload"), lang));
    }
    if sources > 1 {
        return Err(Failure::args_t("args.onePayload", &[], cmd, lang));
    }
    let repeat = match a.repeat {
        None => None,
        Some(_) if a.stdin => return Err(Failure::args_t("args.repeatStdin", &[], cmd, lang)),
        Some(s) if s.is_finite() && s > 0.0 && s < 1e9 => Some(Duration::from_secs_f64(s)),
        Some(_) => {
            return Err(Failure::args_t(
                "args.positiveSeconds",
                &[("arg", "--repeat")],
                cmd,
                lang,
            ))
        }
    };
    let (source, qos, retain) = if a.delete_retained {
        (Source::Bytes(Vec::new()), a.qos.max(1), true)
    } else if let Some(m) = &a.message {
        (
            Source::Bytes(m.clone().into_encoded_bytes()),
            a.qos,
            a.retain,
        )
    } else if let Some(f) = &a.file {
        (Source::Bytes(conn::read_file(f, lang)?), a.qos, a.retain)
    } else {
        (Source::Stdin, a.qos, a.retain)
    };

    let mut o = conn::build(g, &a.tls, cmd, lang)?;
    o.subscriptions.clear();
    o.reconnect = false;
    let (handle, _info, mut rx) = mqttpanel_client::connect(o)
        .await
        .map_err(|f| conn::connect_failure(&f, lang))?;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut pending = HashSet::new();
    let mut c = Counters { sent: 0, done: 0 };

    let outcome: Result<(), Failure> = async {
        match source {
            Source::Bytes(payload) => loop {
                send(&handle, &topic, payload.clone(), qos, retain, &mut pending, &mut c);
                if !drain(&mut rx, &mut pending, &mut c, &mut ctrl_c, lang).await? {
                    return Ok(());
                }
                let Some(every) = repeat else { return Ok(()) };
                tokio::select! {
                    _ = ctrl_c.as_mut() => return Ok(()),
                    _ = tokio::time::sleep(every) => {}
                }
            },
            Source::Stdin => {
                let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
                loop {
                    tokio::select! {
                        _ = ctrl_c.as_mut() => return Ok(()),
                        ev = rx.recv() => on_event(ev, &mut pending, &mut c, lang)?,
                        line = lines.next_line() => match line {
                            Ok(Some(l)) => {
                                let l = l.strip_suffix('\r').unwrap_or(&l);
                                if !l.is_empty() {
                                    send(&handle, &topic, l.as_bytes().to_vec(), qos, retain, &mut pending, &mut c);
                                }
                            }
                            // End of input (or unreadable input): finish what was sent.
                            Ok(None) | Err(_) => break,
                        },
                    }
                }
                drain(&mut rx, &mut pending, &mut c, &mut ctrl_c, lang).await.map(|_| ())
            }
        }
    }
    .await;
    handle.disconnect().await;
    outcome?;
    if g.format() == Format::Json {
        let status = Status {
            ok: true,
            topic: &topic,
            qos,
            retain,
            sent: c.sent,
            acknowledged: if qos == 0 { 0 } else { c.done },
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&status).unwrap_or_default()
        );
    }
    Ok(())
}

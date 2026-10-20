//! The MCP stdio shim `mqttpanel mcp` (docs/TESTPLAN.md M-09): forwards bytes both ways to the app's socket; starts
//! the app when the socket is missing and waits for it; says so on stderr when the app does not come.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// An echo server on the socket: answers each line in upper case.
fn echo(listener: UnixListener) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Ok((s, _)) = listener.accept() {
            let mut w = s.try_clone().unwrap();
            for line in BufReader::new(s).lines() {
                let Ok(l) = line else { break };
                if writeln!(w, "{}", l.to_uppercase()).is_err() {
                    break;
                }
            }
        }
    })
}

fn shim(home: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_mqttpanel"));
    c.arg("mcp").env("MQTTPANEL_HOME", home).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

fn round_trip(child: &mut std::process::Child) {
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}").unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert_eq!(line, "{\"JSONRPC\":\"2.0\",\"ID\":1,\"METHOD\":\"PING\"}\n");
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(st) = child.try_wait().unwrap() {
            assert!(st.success(), "exit {st:?}");
            break;
        }
        assert!(Instant::now() < deadline, "the shim did not exit after stdin closed");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn m09_shim_forwards_to_the_running_app() {
    let home = tempfile::tempdir().unwrap();
    let srv = echo(UnixListener::bind(home.path().join("mcp.sock")).unwrap());
    let mut child = shim(home.path()).spawn().unwrap();
    round_trip(&mut child);
    srv.join().unwrap();
}

#[test]
fn m09_shim_starts_the_app_and_waits_for_its_socket() {
    let home = tempfile::tempdir().unwrap();
    let marker = home.path().join("started");
    // The "app": a script that leaves a marker; the test then plays the app and opens the socket a moment later.
    let app = home.path().join("fake-app.sh");
    std::fs::write(&app, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&app, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let mut child = shim(home.path()).env("MQTTPANEL_APP", &app).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "the shim did not start the app");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let srv = echo(UnixListener::bind(home.path().join("mcp.sock")).unwrap());
    round_trip(&mut child);
    srv.join().unwrap();
}

#[test]
fn m09_shim_reports_an_app_that_does_not_come() {
    let home = tempfile::tempdir().unwrap();
    let out = shim(home.path()).env("MQTTPANEL_APP", "/usr/bin/true").env("MQTTPANEL_SHIM_WAIT_S", "0.5").stdin(Stdio::null()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "stdout belongs to the host");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("starting it") && err.contains("did not answer"), "{err}");
}

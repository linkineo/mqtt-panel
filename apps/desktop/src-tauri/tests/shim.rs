//! The app's own executable with the single argument `mcp` is the stdio shim (docs/MCP.md section 2): a host can start
//! the app bundle's executable without a separately installed command line.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

#[test]
fn m09_app_executable_with_mcp_forwards_to_the_socket() {
    let home = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(home.path().join("mcp.sock")).unwrap();
    let srv = std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        let mut w = s.try_clone().unwrap();
        let mut line = String::new();
        BufReader::new(s).read_line(&mut line).unwrap();
        w.write_all(format!("echo {line}").as_bytes()).unwrap();
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_mqtt-panel"))
        .arg("mcp")
        .env("MQTTPANEL_HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(child.stdin.as_mut().unwrap(), "hello").unwrap();
    let mut out = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut out).unwrap();
    assert_eq!(out, "echo hello\n");
    srv.join().unwrap();
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
}

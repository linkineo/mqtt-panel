//! Test harness for the `mqttpanel` binary: process helpers, the Mosquitto fixture (F-03) and the embedded
//! broker started through `mqttpanel broker --config`.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const BIN: &str = env!("CARGO_BIN_EXE_mqttpanel");

/// A `mqttpanel` command isolated from the real app data and keychain, in English.
pub fn mp(home: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env("MQTTPANEL_HOME", home)
        .env("MQTTPANEL_SECRETS", "memory")
        .env("LC_ALL", "en_US.UTF-8")
        .env_remove("MQTTPANEL_PASSWORD")
        .env_remove("MQTTPANEL_SECRETS_SEED")
        .env_remove("MQTTPANEL_READY_HINT");
    c
}

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("free port")
}

pub fn wait_port(port: u16, timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    false
}

/// Result of a finished command.
#[derive(Debug)]
pub struct Out {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub elapsed: Duration,
}

impl Out {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    /// stderr without the trailing newline.
    pub fn err(&self) -> &str {
        self.stderr.trim_end_matches('\n')
    }
}

fn reader<R: Read + Send + 'static>(r: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    })
}

fn wait_child(child: &mut Child, timeout: Duration, what: &str) -> Option<i32> {
    let end = Instant::now() + timeout;
    loop {
        if let Ok(Some(st)) = child.try_wait() {
            return st.code();
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what} did not finish within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Runs a command to completion, feeding `stdin` when given.
pub fn run(mut cmd: Command, stdin: Option<&[u8]>, timeout: Duration) -> Out {
    let started = Instant::now();
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = cmd.spawn().expect("spawn mqttpanel");
    if let Some(data) = stdin {
        if let Some(mut si) = child.stdin.take() {
            let _ = si.write_all(data);
        }
    }
    let so = reader(child.stdout.take());
    let se = reader(child.stderr.take());
    let code = wait_child(&mut child, timeout, &format!("{cmd:?}"));
    Out {
        code,
        stdout: so.join().unwrap_or_default(),
        stderr: String::from_utf8_lossy(&se.join().unwrap_or_default()).into_owned(),
        elapsed: started.elapsed(),
    }
}

/// Shorthand: run with arguments, 20 s timeout.
pub fn run_args(home: &Path, args: &[&str]) -> Out {
    let mut c = mp(home);
    c.args(args);
    run(c, None, Duration::from_secs(20))
}

/// A process running in the background (a subscriber, a repeating publisher, a broker).
pub struct Bg {
    pub child: Child,
    started: Instant,
    stdout: Option<JoinHandle<Vec<u8>>>,
    stdout_lines: Option<mpsc::Receiver<String>>,
    stderr_lines: mpsc::Receiver<String>,
    stderr_all: Arc<Mutex<String>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl Bg {
    /// Spawns; with `line_stdout` stdout is delivered line by line (see [`Bg::wait_stdout_line`]).
    pub fn spawn(mut cmd: Command, line_stdout: bool) -> Bg {
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let mut child = cmd.spawn().expect("spawn mqttpanel");
        let (tx, rx) = mpsc::channel();
        let all = Arc::new(Mutex::new(String::new()));
        let all2 = all.clone();
        let err = child.stderr.take();
        let stderr_thread = std::thread::spawn(move || {
            if let Some(err) = err {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    if let Ok(mut a) = all2.lock() {
                        a.push_str(&line);
                        a.push('\n');
                    }
                    let _ = tx.send(line);
                }
            }
        });
        let (stdout, stdout_lines) = if line_stdout {
            let (tx, rx) = mpsc::channel();
            let out = child.stdout.take();
            let h = std::thread::spawn(move || {
                let mut all = Vec::new();
                if let Some(out) = out {
                    for line in BufReader::new(out).lines().map_while(Result::ok) {
                        all.extend_from_slice(line.as_bytes());
                        all.push(b'\n');
                        let _ = tx.send(line);
                    }
                }
                all
            });
            (Some(h), Some(rx))
        } else {
            (Some(reader(child.stdout.take())), None)
        };
        Bg {
            child,
            started: Instant::now(),
            stdout,
            stdout_lines,
            stderr_lines: rx,
            stderr_all: all,
            stderr_thread: Some(stderr_thread),
        }
    }

    /// A `sub` with the ready hint; returns once every subscription is acknowledged.
    pub fn sub(home: &Path, args: &[&str]) -> Bg {
        let mut c = mp(home);
        c.env("MQTTPANEL_READY_HINT", "1").arg("sub").args(args);
        let bg = Bg::spawn(c, false);
        bg.wait_stderr_line("ready", Duration::from_secs(10));
        bg
    }

    pub fn wait_stderr_line(&self, needle: &str, timeout: Duration) {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            match self.stderr_lines.recv_timeout(Duration::from_millis(50)) {
                Ok(l) if l.contains(needle) => return,
                _ => {}
            }
        }
        panic!(
            "no stderr line containing {needle:?}; stderr so far: {}",
            self.stderr_all
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default()
        );
    }

    pub fn wait_stdout_line(&self, needle: &str, timeout: Duration) -> String {
        let rx = self.stdout_lines.as_ref().expect("line stdout");
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(l) if l.contains(needle) => return l,
                _ => {}
            }
        }
        panic!("no stdout line containing {needle:?}");
    }

    pub fn interrupt(&self) {
        let st = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("kill -INT");
        assert!(st.success());
    }

    pub fn finish(mut self, timeout: Duration) -> Out {
        let code = wait_child(&mut self.child, timeout, "background mqttpanel");
        let stdout = self
            .stdout
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        if let Some(h) = self.stderr_thread.take() {
            let _ = h.join();
        }
        let stderr = self
            .stderr_all
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        Out {
            code,
            stdout,
            stderr,
            elapsed: self.started.elapsed(),
        }
    }
}

impl Drop for Bg {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

// ------------------------------------------------------------------ Mosquitto (F-03)

pub fn mosquitto_path() -> Option<PathBuf> {
    let candidates = [
        "/opt/homebrew/opt/mosquitto/sbin/mosquitto",
        "/opt/homebrew/sbin/mosquitto",
        "/usr/local/sbin/mosquitto",
        "/usr/sbin/mosquitto",
    ];
    if let Some(p) = candidates.iter().map(PathBuf::from).find(|p| p.exists()) {
        return Some(p);
    }
    let out = Command::new("which").arg("mosquitto").output().ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

fn mosquitto_passwd_path() -> Option<PathBuf> {
    [
        "/opt/homebrew/bin/mosquitto_passwd",
        "/usr/local/bin/mosquitto_passwd",
        "/usr/bin/mosquitto_passwd",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

pub const USER: &str = "commissioning";
pub const PASSWORD: &str = "s3cret-pass";

/// A Mosquitto instance on random ports, killed on drop.
pub struct Mosquitto {
    child: Child,
    pub dir: tempfile::TempDir,
    /// Plain TCP, anonymous.
    pub plain: u16,
    /// Plain TCP with a password file (user [`USER`], password [`PASSWORD`]); None without mosquitto_passwd.
    pub passwd: Option<u16>,
    /// WebSocket, anonymous.
    pub ws: u16,
    /// TLS with a server certificate from the test root CA (files in `dir`: root-ca.pem, client-ok.pem, ...).
    pub tls_ca: Option<u16>,
    /// TLS with a self-signed server certificate.
    pub tls_self: Option<u16>,
    /// Mutual TLS: client certificate from the test root CA required.
    pub mtls: Option<u16>,
    pub kit: Option<mqttpanel_tls::kit::TestKit>,
}

impl Drop for Mosquitto {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts Mosquitto, or prints "SKIP: mosquitto not found" and returns None.
pub fn mosquitto(with_tls: bool) -> Option<Mosquitto> {
    let Some(bin) = mosquitto_path() else {
        println!("SKIP: mosquitto not found");
        return None;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let d = dir.path();
    let plain = free_port();
    let ws = free_port();
    let mut conf = String::from("per_listener_settings true\nlog_dest stderr\n");
    conf += &format!("listener {plain} 127.0.0.1\nallow_anonymous true\n");
    conf += &format!("listener {ws} 127.0.0.1\nprotocol websockets\nallow_anonymous true\n");
    let mut passwd = None;
    if let Some(pw) = mosquitto_passwd_path() {
        let file = d.join("passwd");
        let st = Command::new(pw)
            .arg("-c")
            .arg("-b")
            .arg(&file)
            .arg(USER)
            .arg(PASSWORD)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if st.map(|s| s.success()).unwrap_or(false) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            let p = free_port();
            conf += &format!(
                "listener {p} 127.0.0.1\nallow_anonymous false\npassword_file {}\n",
                file.display()
            );
            passwd = Some(p);
        } else {
            println!("SKIP: mosquitto_passwd failed; password listener not available");
        }
    } else {
        println!("SKIP: mosquitto_passwd not found; password listener not available");
    }
    let (mut tls_ca, mut tls_self, mut mtls, mut kit) = (None, None, None, None);
    if with_tls {
        let k = mqttpanel_tls::kit::TestKit::generate().expect("test kit");
        k.write_to(d).expect("write kit");
        let (a, b, c) = (free_port(), free_port(), free_port());
        let f = |n: &str| d.join(n).display().to_string();
        conf += &format!(
            "listener {a}\nallow_anonymous true\ncafile {}\ncertfile {}\nkeyfile {}\n",
            f("root-ca.pem"),
            f("server.pem"),
            f("server.key")
        );
        conf += &format!(
            "listener {b}\nallow_anonymous true\ncafile {}\ncertfile {}\nkeyfile {}\n",
            f("self-signed.pem"),
            f("self-signed.pem"),
            f("self-signed.key")
        );
        conf += &format!(
            "listener {c}\nallow_anonymous true\ncafile {}\ncertfile {}\nkeyfile {}\nrequire_certificate true\n",
            f("root-ca.pem"),
            f("server.pem"),
            f("server.key")
        );
        tls_ca = Some(a);
        tls_self = Some(b);
        mtls = Some(c);
        kit = Some(k);
    }
    let conf_path = d.join("mosquitto.conf");
    std::fs::write(&conf_path, conf).expect("write conf");
    let log = std::fs::File::create(d.join("mosquitto.log")).expect("log");
    let child = Command::new(bin)
        .arg("-c")
        .arg(&conf_path)
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start mosquitto");
    let m = Mosquitto {
        child,
        dir,
        plain,
        passwd,
        ws,
        tls_ca,
        tls_self,
        mtls,
        kit,
    };
    let mut ports = vec![m.plain, m.ws];
    ports.extend(m.passwd);
    ports.extend(m.tls_ca);
    for p in ports {
        if !wait_port(p, Duration::from_secs(5)) {
            let log =
                std::fs::read_to_string(m.dir.path().join("mosquitto.log")).unwrap_or_default();
            panic!("mosquitto did not open port {p}: {log}");
        }
    }
    Some(m)
}

// ------------------------------------------------------------------ embedded broker via the CLI

/// `mqttpanel broker --config` with one anonymous TCP listener on localhost; stopped with SIGINT by the caller.
pub struct Embedded {
    pub bg: Bg,
    pub port: u16,
    pub dir: tempfile::TempDir,
}

pub fn embedded_broker() -> Embedded {
    let dir = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let cfg = dir.path().join("broker.toml");
    std::fs::write(&cfg, format!("bind = \"localhost\"\n\n[auth]\nanonymous = true\n\n[[listener]]\nkind = \"tcp\"\nport = {port}\n")).expect("write broker.toml");
    let mut c = mp(dir.path());
    c.arg("broker").arg("--config").arg(&cfg);
    let bg = Bg::spawn(c, true);
    bg.wait_stdout_line("Broker running", Duration::from_secs(10));
    assert!(
        wait_port(port, Duration::from_secs(5)),
        "embedded broker port {port}"
    );
    Embedded { bg, port, dir }
}

/// An embedded broker that requires a login (for exit-code tests): user [`USER`], password [`PASSWORD`].
pub fn embedded_broker_with_login() -> Embedded {
    let dir = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let cfg = dir.path().join("broker.toml");
    std::fs::write(
        &cfg,
        format!("bind = \"localhost\"\n\n[auth]\nanonymous = false\n[[auth.users]]\nusername = \"{USER}\"\npassword = \"{PASSWORD}\"\n\n[[listener]]\nkind = \"tcp\"\nport = {port}\n"),
    )
    .expect("write broker.toml");
    let mut c = mp(dir.path());
    c.arg("broker").arg("--config").arg(&cfg);
    let bg = Bg::spawn(c, true);
    bg.wait_stdout_line("Broker running", Duration::from_secs(10));
    assert!(
        wait_port(port, Duration::from_secs(5)),
        "embedded broker port {port}"
    );
    Embedded { bg, port, dir }
}

/// A unique topic prefix per test run.
pub fn prefix(name: &str) -> String {
    format!("mqttpanel-test/{}/{name}", uuid::Uuid::new_v4().simple())
}

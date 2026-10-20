//! The MCP stdio shim (docs/MCP.md section 2): a host (Claude Desktop, Claude Code, the ChatGPT desktop app) starts
//! `mqttpanel mcp` (or the app's own executable with the single argument `mcp`); the shim connects to the local socket
//! of the running app, starting the app when the socket is missing, and forwards bytes both ways until either side
//! closes. It never reads or changes the MCP messages. Everything it prints for humans goes to stderr.
//!
//! The endpoint is owned by the app: a Unix domain socket `mcp.sock` in the app data folder (mode 0600) on macOS and
//! Linux, a named pipe `\\.\pipe\mqttpanel-mcp-<user>` on Windows (same user only).
//!
//! Environment (tests and portable use): `MQTTPANEL_HOME` (data folder), `MQTTPANEL_APP` (the program to start when
//! the app is not running), `MQTTPANEL_SHIM_WAIT_S` (how long to wait for the socket, default 20).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The endpoint for a data folder: the socket path on Unix, the pipe name on Windows.
pub fn endpoint_in(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        let user: String = user.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        // A data folder other than the default one (MQTTPANEL_HOME) gets its own pipe, so tests never meet the app.
        let suffix = if std::env::var("MQTTPANEL_HOME").map(|h| !h.is_empty()).unwrap_or(false) {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in dir.to_string_lossy().bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
            format!("-{:08x}", h as u32)
        } else {
            String::new()
        };
        PathBuf::from(format!(r"\\.\pipe\mqttpanel-mcp-{user}{suffix}"))
    }
    #[cfg(not(windows))]
    {
        // A Unix socket path is limited to about 104 bytes (macOS). A data folder deep enough to pass it (the sandboxed
        // Mac App Store build lives under ~/Library/Containers/…) gets a socket in the per-user temporary folder,
        // named after the data folder, which the app and the shim compute alike.
        let p = dir.join("mcp.sock");
        if p.as_os_str().len() < 100 {
            return p;
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in dir.to_string_lossy().bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        std::env::temp_dir().join(format!("mqttpanel-mcp-{:016x}.sock", h))
    }
}

/// The endpoint of the app data folder (`MQTTPANEL_HOME` overrides the folder).
pub fn endpoint() -> PathBuf {
    endpoint_in(&mqttpanel_profiles::default_dir())
}

/// How the shim starts the app when its socket is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// The shim is the app's own executable (`<app> mcp`): start this executable, or its bundle on macOS.
    SelfExe,
    /// The shim is the command line `mqttpanel mcp`: find the installed app.
    FindApp,
}

#[cfg(unix)]
type Stream = tokio::net::UnixStream;
#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

async fn try_connect(ep: &Path) -> Option<Stream> {
    #[cfg(unix)]
    {
        tokio::net::UnixStream::connect(ep).await.ok()
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new().open(ep).ok()
    }
}

/// The app bundle that contains this executable (macOS), if any.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let mut p = exe.parent()?; // MacOS
    if p.file_name()? != "MacOS" {
        return None;
    }
    p = p.parent()?; // Contents
    let bundle = p.parent()?;
    bundle.extension().is_some_and(|e| e == "app").then(|| bundle.to_path_buf())
}

fn sibling_app(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let names: &[&str] = if cfg!(windows) { &["MQTT Panel.exe", "mqtt-panel.exe"] } else { &["MQTT Panel", "mqtt-panel"] };
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

fn detached(cmd: &mut std::process::Command) -> std::io::Result<()> {
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().map(|_| ())
}

/// Starts the app without waiting for it. On macOS the app opens in the background (`open -g`), so the host keeps the
/// focus.
fn start_app(launch: Launch) -> std::io::Result<()> {
    if let Ok(app) = std::env::var("MQTTPANEL_APP") {
        if !app.is_empty() {
            return detached(&mut std::process::Command::new(app));
        }
    }
    let exe = std::env::current_exe()?;
    let open_bundle = |b: &Path| detached(std::process::Command::new("open").arg("-g").arg(b));
    match launch {
        Launch::SelfExe => match bundle_of(&exe) {
            Some(b) if cfg!(target_os = "macos") => open_bundle(&b),
            _ => detached(&mut std::process::Command::new(&exe)),
        },
        Launch::FindApp => {
            if let Some(b) = bundle_of(&exe).filter(|_| cfg!(target_os = "macos")) {
                return open_bundle(&b);
            }
            if let Some(app) = sibling_app(&exe) {
                return detached(&mut std::process::Command::new(app));
            }
            if cfg!(target_os = "macos") {
                return detached(std::process::Command::new("open").args(["-g", "-a", "MQTT Panel"]));
            }
            if cfg!(windows) {
                if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                    let p = PathBuf::from(local).join("MQTT Panel").join("MQTT Panel.exe");
                    if p.is_file() {
                        return detached(&mut std::process::Command::new(p));
                    }
                }
                return detached(&mut std::process::Command::new("MQTT Panel.exe"));
            }
            detached(&mut std::process::Command::new("mqtt-panel"))
        }
    }
}

fn wait_limit() -> Duration {
    let s = std::env::var("MQTTPANEL_SHIM_WAIT_S").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(20.0);
    Duration::from_secs_f64(s.clamp(0.1, 600.0))
}

/// Connects to the app, starting it when its socket is missing, waiting up to 20 s.
async fn connect_or_start(ep: &Path, launch: Launch) -> Result<Stream, String> {
    if let Some(s) = try_connect(ep).await {
        return Ok(s);
    }
    eprintln!("MQTT Panel is not running; starting it.");
    if let Err(e) = start_app(launch) {
        return Err(format!("MQTT Panel could not be started ({e}). Start it, then try again."));
    }
    let deadline = Instant::now() + wait_limit();
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(s) = try_connect(ep).await {
            return Ok(s);
        }
    }
    Err("MQTT Panel did not answer in time. Start it, then try again.".into())
}

/// Forwards stdin to the stream and the stream to stdout until either side closes.
async fn forward(stream: Stream) {
    let (mut rd, mut wr) = tokio::io::split(stream);
    let up = async {
        let mut stdin = tokio::io::stdin();
        let _ = tokio::io::copy(&mut stdin, &mut wr).await;
        let _ = tokio::io::AsyncWriteExt::shutdown(&mut wr).await;
    };
    let down = async {
        let mut stdout = tokio::io::stdout();
        let _ = tokio::io::copy(&mut rd, &mut stdout).await;
        let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
}

/// Runs the shim; returns the process exit code (0 when either side closed, 1 when the app could not be reached).
pub fn run(launch: Launch) -> i32 {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("MQTT Panel: {e}");
            return 1;
        }
    };
    rt.block_on(async {
        match connect_or_start(&endpoint(), launch).await {
            Ok(s) => {
                forward(s).await;
                0
            }
            Err(m) => {
                eprintln!("{m}");
                1
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_is_found_only_inside_an_app() {
        assert_eq!(bundle_of(Path::new("/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel")), Some(PathBuf::from("/Applications/MQTT Panel.app")));
        assert_eq!(bundle_of(Path::new("/opt/x/bin/mqttpanel")), None);
    }

    #[cfg(unix)]
    #[test]
    fn endpoint_is_the_socket_in_the_data_folder() {
        assert_eq!(endpoint_in(Path::new("/tmp/home")), PathBuf::from("/tmp/home/mcp.sock"));
        let deep = Path::new("/Users/someone/Library/Containers/com.remora-labs.mqttpanel/Data/Library/Application Support/com.remora-labs.mqttpanel");
        let e = endpoint_in(deep);
        assert!(e.as_os_str().len() < 104 + std::env::temp_dir().as_os_str().len() && e.starts_with(std::env::temp_dir()), "{e:?}");
        assert_eq!(e, endpoint_in(deep));
    }
}

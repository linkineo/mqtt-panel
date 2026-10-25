//! "Add to Claude" and "Add to ChatGPT" (docs/MCP.md section 2): the entry `mqttpanel` that starts this executable with
//! the argument `mcp`, written into the hosts' configuration files. Everything else in those files is kept.
//!
//! - Claude Desktop: `claude_desktop_config.json` (`mcpServers.mqttpanel`) in `~/Library/Application Support/Claude`
//!   (macOS), `%APPDATA%\Claude` (Windows), `~/.config/Claude` (Linux); written when Claude Desktop is installed (its
//!   app or that folder exists), the file created when missing (Claude Desktop creates it only when its developer
//!   settings are opened).
//! - Claude Code: `~/.claude.json` (`mcpServers.mqttpanel`, user scope); written only when the file exists.
//! - In the App Sandbox (Mac App Store variant) the home is the container and other apps' files cannot be written:
//!   the buttons write nothing and say so; the user copies the configuration instead.
//! - The ChatGPT desktop app (and the Codex CLI and IDE extension, which share the file): `~/.codex/config.toml`
//!   (`[mcp_servers.mqttpanel]`, `$CODEX_HOME` when set); created when missing.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const ENTRY: &str = "mqttpanel";

/// Where the host files live: the user's home, the Codex folder and the folders where apps are installed.
#[derive(Debug, Clone)]
pub struct Homes {
    pub home: PathBuf,
    pub codex: PathBuf,
    /// Folders searched for the Claude Desktop app (macOS `Claude.app`); the config folder counts too.
    pub apps: Vec<PathBuf>,
    /// This copy runs in the macOS App Sandbox (Mac App Store variant): other apps' files cannot be written.
    pub sandboxed: bool,
}

/// The real home folder. Inside the App Sandbox `HOME` (and so `dirs::home_dir`) is the app's container
/// (`~/Library/Containers/<id>/Data`); the user's home is the part before `/Library/Containers/`.
pub fn real_home(home: &Path) -> PathBuf {
    let s = home.to_string_lossy();
    match s.find("/Library/Containers/") {
        Some(i) => PathBuf::from(&s[..i]),
        None => home.to_path_buf(),
    }
}

impl Homes {
    /// The real home (`MQTTPANEL_HOSTS_HOME` replaces it, for tests of the running app).
    pub fn detect() -> Homes {
        let sandboxed = std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some_and(|v| !v.is_empty());
        if let Ok(h) = std::env::var("MQTTPANEL_HOSTS_HOME") {
            if !h.is_empty() {
                return Homes { sandboxed, ..Homes::at(Path::new(&h)) };
            }
        }
        let home = real_home(&dirs::home_dir().unwrap_or_else(std::env::temp_dir));
        let codex = std::env::var_os("CODEX_HOME").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| home.join(".codex"));
        let mut apps = vec![home.join("Applications")];
        if cfg!(target_os = "macos") {
            apps.insert(0, PathBuf::from("/Applications"));
        }
        if cfg!(windows) {
            // the Claude Desktop installer puts the app in %LOCALAPPDATA%\AnthropicClaude
            apps.push(home.join("AppData").join("Local").join("AnthropicClaude"));
        }
        Homes { home, codex, apps, sandboxed }
    }

    pub fn at(home: &Path) -> Homes {
        Homes { home: home.to_path_buf(), codex: home.join(".codex"), apps: vec![home.join("Applications")], sandboxed: false }
    }

    pub fn claude_desktop(&self) -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            self.home.join("Library").join("Application Support")
        } else if cfg!(windows) {
            self.home.join("AppData").join("Roaming")
        } else {
            self.home.join(".config")
        };
        base.join("Claude").join("claude_desktop_config.json")
    }

    /// Claude Desktop is on this computer: its app (macOS) or its settings folder exists. Claude Desktop creates
    /// `claude_desktop_config.json` only when the user opens its developer settings, so the file alone is not a test.
    pub fn claude_desktop_installed(&self) -> bool {
        let folder = self.claude_desktop().parent().is_some_and(|d| d.is_dir());
        folder || self.apps.iter().any(|d| d.join("Claude.app").is_dir() || (cfg!(windows) && d.ends_with("AnthropicClaude") && d.is_dir()))
    }

    pub fn claude_code(&self) -> PathBuf {
        self.home.join(".claude.json")
    }

    pub fn codex_config(&self) -> PathBuf {
        self.codex.join("config.toml")
    }

    /// A path for display: the home folder as "~".
    pub fn display(&self, p: &Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
            Err(_) => p.display().to_string(),
        }
    }
}

/// One host configuration file, as the Settings page shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostFile {
    /// claude or chatgpt
    pub family: String,
    /// claudeDesktop, claudeCode or chatgpt
    pub host: String,
    pub path: String,
    pub exists: bool,
    /// The host is on this computer ("Add to …" can write its file, creating it for Claude Desktop and ChatGPT).
    pub installed: bool,
    /// The file has the entry `mqttpanel` and it starts this executable.
    pub configured: bool,
}

/// What a button wrote.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    /// Files written (for display).
    pub written: Vec<String>,
    /// The hosts whose files were written (claudeDesktop, claudeCode, chatgpt), in the same order.
    pub written_hosts: Vec<String>,
    /// Files of hosts that are not on this computer (nothing written).
    pub missing: Vec<String>,
    pub missing_hosts: Vec<String>,
    pub failed: Vec<WriteFailure>,
    /// Nothing was tried: this copy runs in the App Sandbox, which forbids writing other apps' files.
    pub sandboxed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WriteFailure {
    pub host: String,
    pub path: String,
    pub error: mqttpanel_client::UserMessage,
}

/// Reads a host's JSON file: a UTF-8 byte order mark (Windows editors) is accepted.
fn parse_json(text: &str) -> Option<Value> {
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn read_json(p: &Path) -> Option<Value> {
    std::fs::read_to_string(p).ok().and_then(|s| parse_json(&s))
}

fn json_configured(p: &Path, exe: &str) -> bool {
    read_json(p).is_some_and(|v| {
        let e = &v["mcpServers"][ENTRY];
        e["command"] == json!(exe) && e["args"] == json!(["mcp"])
    })
}

fn toml_configured(p: &Path, exe: &str) -> bool {
    let Ok(s) = std::fs::read_to_string(p) else { return false };
    let Ok(doc) = s.parse::<toml_edit::DocumentMut>() else { return false };
    let e = doc.get("mcp_servers").and_then(|m| m.get(ENTRY));
    let cmd = e.and_then(|e| e.get("command")).and_then(|c| c.as_str());
    let args: Option<Vec<String>> = e.and_then(|e| e.get("args")).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
    cmd == Some(exe) && args.as_deref() == Some(&["mcp".to_string()][..])
}

/// The three files and what they say.
pub fn host_files(h: &Homes, exe: &str) -> Vec<HostFile> {
    let f = |family: &str, host: &str, p: PathBuf, installed: bool, configured: bool| {
        let exists = p.is_file();
        HostFile { family: family.into(), host: host.into(), path: h.display(&p), exists, installed: installed || exists, configured }
    };
    vec![
        f("claude", "claudeDesktop", h.claude_desktop(), h.claude_desktop_installed(), json_configured(&h.claude_desktop(), exe)),
        f("claude", "claudeCode", h.claude_code(), false, json_configured(&h.claude_code(), exe)),
        // the Codex file is created when missing
        f("chatgpt", "chatgpt", h.codex_config(), true, toml_configured(&h.codex_config(), exe)),
    ]
}

/// Writes through a temporary file next to the target, keeping the target's permissions.
fn write_keeping(p: &Path, data: &str) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("mqttpanel-tmp");
    std::fs::write(&tmp, data)?;
    if let Ok(meta) = std::fs::metadata(p) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, p).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn io_fail(h: &Homes, host: &str, p: &Path, e: std::io::Error) -> WriteFailure {
    WriteFailure { host: host.into(), path: h.display(p), error: mqttpanel_client::msg!("file.writeFailed", file = h.display(p), detail = e.kind().to_string()) }
}

fn invalid(h: &Homes, host: &str, p: &Path) -> WriteFailure {
    WriteFailure { host: host.into(), path: h.display(p), error: crate::mcp::text::um("hostFileInvalid", &[("file", json!(h.display(p)))]) }
}

/// Adds the entry to a host's JSON file. A missing file is created only when `create` (the host is installed and
/// starts without the file); otherwise the host counts as not on this computer.
fn add_json(h: &Homes, host: &str, p: &Path, entry: Value, create: bool, r: &mut WriteReport) {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => String::new(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            r.missing.push(h.display(p));
            r.missing_hosts.push(host.into());
            return;
        }
        Err(e) => return r.failed.push(io_fail(h, host, p, e)),
    };
    let mut v: Value = if text.trim_start_matches('\u{feff}').trim().is_empty() {
        json!({})
    } else {
        match parse_json(&text) {
            Some(v) => v,
            None => return r.failed.push(invalid(h, host, p)),
        }
    };
    let Some(obj) = v.as_object_mut() else {
        return r.failed.push(invalid(h, host, p));
    };
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers[ENTRY] = entry;
    let out = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    match write_keeping(p, &out) {
        Ok(()) => {
            r.written.push(h.display(p));
            r.written_hosts.push(host.into());
        }
        Err(e) => r.failed.push(io_fail(h, host, p, e)),
    }
}

/// "Add to Claude": Claude Desktop (when installed; its file is created when missing) and Claude Code (when
/// `~/.claude.json` exists). In the App Sandbox nothing is tried.
pub fn add_to_claude(h: &Homes, exe: &str) -> WriteReport {
    let mut r = WriteReport { sandboxed: h.sandboxed, ..Default::default() };
    if h.sandboxed {
        return r;
    }
    add_json(h, "claudeDesktop", &h.claude_desktop(), json!({ "command": exe, "args": ["mcp"] }), h.claude_desktop_installed(), &mut r);
    add_json(h, "claudeCode", &h.claude_code(), json!({ "type": "stdio", "command": exe, "args": ["mcp"], "env": {} }), false, &mut r);
    r
}

/// "Add to ChatGPT": `[mcp_servers.mqttpanel]` in the Codex configuration, created when missing.
pub fn add_to_chatgpt(h: &Homes, exe: &str) -> WriteReport {
    use toml_edit::{value, Array, DocumentMut, Item, Table};
    let mut r = WriteReport { sandboxed: h.sandboxed, ..Default::default() };
    if h.sandboxed {
        return r;
    }
    let p = h.codex_config();
    let text = if p.is_file() {
        match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) => {
                r.failed.push(io_fail(h, "chatgpt", &p, e));
                return r;
            }
        }
    } else {
        String::new()
    };
    let mut doc = match text.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(_) => {
            r.failed.push(invalid(h, "chatgpt", &p));
            return r;
        }
    };
    if !doc.get("mcp_servers").is_some_and(|i| i.is_table()) {
        let mut t = Table::new();
        t.set_implicit(true);
        doc["mcp_servers"] = Item::Table(t);
    }
    let mut entry = Table::new();
    entry["command"] = value(exe);
    let mut args = Array::new();
    args.push("mcp");
    entry["args"] = value(args);
    doc["mcp_servers"][ENTRY] = Item::Table(entry);
    match write_keeping(&p, &doc.to_string()) {
        Ok(()) => {
            r.written.push(h.display(&p));
            r.written_hosts.push("chatgpt".into());
        }
        Err(e) => r.failed.push(io_fail(h, "chatgpt", &p, e)),
    }
    r
}

/// "Other hosts": the usual JSON configuration of a stdio server.
pub fn snippet(exe: &str) -> String {
    serde_json::to_string_pretty(&json!({ "mcpServers": { ENTRY: { "command": exe, "args": ["mcp"] } } })).unwrap_or_default()
}

/// The executable hosts start: this app's own executable (it runs the shim with the argument `mcp`).
pub fn executable() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "mqttpanel".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel";

    #[test]
    fn claude_writes_only_existing_files_and_keeps_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::write(h.claude_code(), r#"{"numStartups": 3, "mcpServers": {"other": {"command": "x"}}, "zeta": true}"#).unwrap();
        let r = add_to_claude(&h, EXE);
        assert_eq!(r.written, vec![h.display(&h.claude_code())]);
        assert_eq!(r.missing, vec![h.display(&h.claude_desktop())]);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(h.claude_code()).unwrap()).unwrap();
        assert_eq!(v["numStartups"], 3);
        assert_eq!(v["zeta"], true);
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["mcpServers"]["mqttpanel"]["command"], EXE);
        assert_eq!(v["mcpServers"]["mqttpanel"]["args"], json!(["mcp"]));
        // key order kept: the new entry goes last, "numStartups" stays first
        let text = std::fs::read_to_string(h.claude_code()).unwrap();
        assert!(text.find("numStartups").unwrap() < text.find("zeta").unwrap());
        let files = host_files(&h, EXE);
        assert!(files.iter().any(|f| f.host == "claudeCode" && f.exists && f.configured));
        assert!(files.iter().any(|f| f.host == "claudeDesktop" && !f.exists && !f.configured));
        // a second run changes nothing else
        let again = add_to_claude(&h, EXE);
        assert_eq!(again.written.len(), 1);
    }

    #[test]
    fn claude_desktop_file_and_invalid_json() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::create_dir_all(h.claude_desktop().parent().unwrap()).unwrap();
        std::fs::write(h.claude_desktop(), "{}").unwrap();
        std::fs::write(h.claude_code(), "{ not json").unwrap();
        let r = add_to_claude(&h, EXE);
        assert_eq!(r.written, vec![h.display(&h.claude_desktop())]);
        assert_eq!(r.failed.len(), 1);
        assert_eq!(std::fs::read_to_string(h.claude_code()).unwrap(), "{ not json");
        let v = read_json(&h.claude_desktop()).unwrap();
        assert_eq!(v, json!({ "mcpServers": { "mqttpanel": { "command": EXE, "args": ["mcp"] } } }));
    }

    #[test]
    fn chatgpt_creates_or_edits_the_codex_file() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        let r = add_to_chatgpt(&h, EXE);
        assert_eq!(r.written.len(), 1);
        let text = std::fs::read_to_string(h.codex_config()).unwrap();
        assert!(text.contains("[mcp_servers.mqttpanel]"), "{text}");
        assert!(toml_configured(&h.codex_config(), EXE));
        // an existing file keeps its other settings and comments
        std::fs::write(h.codex_config(), "# mine\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n\n[mcp_servers.mqttpanel]\ncommand = \"old\"\n").unwrap();
        add_to_chatgpt(&h, EXE);
        let text = std::fs::read_to_string(h.codex_config()).unwrap();
        assert!(text.starts_with("# mine\nmodel = \"o4\""), "{text}");
        assert!(text.contains("[mcp_servers.other]\ncommand = \"x\""), "{text}");
        assert!(!text.contains("\"old\""), "{text}");
        assert!(toml_configured(&h.codex_config(), EXE));
        assert!(host_files(&h, EXE).iter().any(|f| f.host == "chatgpt" && f.configured));
    }

    /// A Claude Desktop file as users have it: other servers, preferences, two-space indent.
    const DESKTOP_REAL: &str = r#"{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp/demo"]
    },
    "weather": {
      "command": "uvx",
      "args": ["weather-mcp"],
      "env": { "API_KEY": "placeholder" }
    }
  },
  "globalShortcut": "Alt+Space",
  "preferences": { "menuBarEnabled": true }
}
"#;

    #[test]
    fn claude_with_realistic_existing_files() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::create_dir_all(h.claude_desktop().parent().unwrap()).unwrap();
        std::fs::write(h.claude_desktop(), DESKTOP_REAL).unwrap();
        std::fs::write(h.claude_code(), r#"{"numStartups": 120, "projects": {"/tmp/demo": {"allowedTools": []}}, "mcpServers": {}, "userID": "x"}"#).unwrap();
        let files = host_files(&h, EXE);
        assert!(files.iter().filter(|f| f.family == "claude").all(|f| f.exists && f.installed && !f.configured), "{files:?}");
        let r = add_to_claude(&h, EXE);
        assert_eq!(r.written_hosts, vec!["claudeDesktop", "claudeCode"]);
        assert!(r.missing.is_empty() && r.failed.is_empty() && !r.sandboxed, "{r:?}");
        let v = read_json(&h.claude_desktop()).unwrap();
        assert_eq!(v["mcpServers"]["filesystem"]["args"][2], "/tmp/demo");
        assert_eq!(v["mcpServers"]["weather"]["env"]["API_KEY"], "placeholder");
        assert_eq!(v["globalShortcut"], "Alt+Space");
        assert_eq!(v["preferences"]["menuBarEnabled"], true);
        assert_eq!(v["mcpServers"]["mqttpanel"], json!({ "command": EXE, "args": ["mcp"] }));
        let c = read_json(&h.claude_code()).unwrap();
        assert_eq!(c["numStartups"], 120);
        assert_eq!(c["mcpServers"]["mqttpanel"]["type"], "stdio");
        assert!(host_files(&h, EXE).iter().filter(|f| f.family == "claude").all(|f| f.configured));
    }

    #[test]
    fn claude_desktop_installed_without_its_file_gets_one() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::create_dir_all(d.path().join("Applications").join("Claude.app")).unwrap();
        let files = host_files(&h, EXE);
        let desk = files.iter().find(|f| f.host == "claudeDesktop").unwrap();
        assert!(desk.installed && !desk.exists);
        assert!(!files.iter().find(|f| f.host == "claudeCode").unwrap().installed);
        let r = add_to_claude(&h, EXE);
        assert_eq!(r.written_hosts, vec!["claudeDesktop"]);
        assert_eq!(r.missing_hosts, vec!["claudeCode"]);
        assert_eq!(read_json(&h.claude_desktop()).unwrap(), json!({ "mcpServers": { "mqttpanel": { "command": EXE, "args": ["mcp"] } } }));
    }

    #[test]
    fn nothing_installed_reports_both_missing() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        let r = add_to_claude(&h, EXE);
        assert!(r.written.is_empty() && r.failed.is_empty());
        assert_eq!(r.missing_hosts, vec!["claudeDesktop", "claudeCode"]);
        assert!(!h.claude_desktop().exists());
    }

    #[test]
    fn byte_order_mark_is_accepted() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::write(h.claude_code(), "\u{feff}{\"a\": 1}").unwrap();
        let r = add_to_claude(&h, EXE);
        assert_eq!(r.written_hosts, vec!["claudeCode"]);
        assert_eq!(read_json(&h.claude_code()).unwrap()["a"], 1);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_a_failure_not_missing() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let h = Homes::at(d.path());
        std::fs::write(h.claude_code(), "{}").unwrap();
        std::fs::set_permissions(h.claude_code(), std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(h.claude_code()).is_ok() {
            return; // running as root
        }
        let r = add_to_claude(&h, EXE);
        std::fs::set_permissions(h.claude_code(), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(r.failed.len(), 1, "{r:?}");
        assert_eq!(r.failed[0].host, "claudeCode");
        assert!(!r.missing_hosts.contains(&"claudeCode".to_string()));
    }

    #[test]
    fn sandboxed_writes_nothing_and_says_so() {
        let d = tempfile::tempdir().unwrap();
        let h = Homes { sandboxed: true, ..Homes::at(d.path()) };
        std::fs::write(h.claude_code(), "{}").unwrap();
        let r = add_to_claude(&h, EXE);
        assert!(r.sandboxed && r.written.is_empty() && r.missing.is_empty() && r.failed.is_empty());
        assert_eq!(std::fs::read_to_string(h.claude_code()).unwrap(), "{}");
        let r = add_to_chatgpt(&h, EXE);
        assert!(r.sandboxed && r.written.is_empty());
        assert!(!h.codex_config().exists());
    }

    #[test]
    fn real_home_leaves_the_container() {
        assert_eq!(real_home(Path::new("/Users/someone/Library/Containers/com.remora-labs.mqttpanel/Data")), PathBuf::from("/Users/someone"));
        assert_eq!(real_home(Path::new("/home/someone")), PathBuf::from("/home/someone"));
    }

    #[test]
    fn snippet_names_the_executable() {
        let s: Value = serde_json::from_str(&snippet(EXE)).unwrap();
        assert_eq!(s["mcpServers"]["mqttpanel"]["args"], json!(["mcp"]));
    }
}

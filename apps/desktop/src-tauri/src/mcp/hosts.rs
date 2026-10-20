//! "Add to Claude" and "Add to ChatGPT" (docs/MCP.md section 2): the entry `mqttpanel` that starts this executable with
//! the argument `mcp`, written into the hosts' configuration files. Everything else in those files is kept.
//!
//! - Claude Desktop: `claude_desktop_config.json` (`mcpServers.mqttpanel`) in `~/Library/Application Support/Claude`
//!   (macOS), `%APPDATA%\Claude` (Windows), `~/.config/Claude` (Linux); written only when the file exists.
//! - Claude Code: `~/.claude.json` (`mcpServers.mqttpanel`, user scope); written only when the file exists.
//! - The ChatGPT desktop app (and the Codex CLI and IDE extension, which share the file): `~/.codex/config.toml`
//!   (`[mcp_servers.mqttpanel]`, `$CODEX_HOME` when set); created when missing.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const ENTRY: &str = "mqttpanel";

/// Where the host files live: the user's home and the Codex folder.
#[derive(Debug, Clone)]
pub struct Homes {
    pub home: PathBuf,
    pub codex: PathBuf,
}

impl Homes {
    /// The real home (`MQTTPANEL_HOSTS_HOME` replaces it, for tests of the running app).
    pub fn detect() -> Homes {
        if let Ok(h) = std::env::var("MQTTPANEL_HOSTS_HOME") {
            if !h.is_empty() {
                return Homes::at(Path::new(&h));
            }
        }
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let codex = std::env::var_os("CODEX_HOME").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| home.join(".codex"));
        Homes { home, codex }
    }

    pub fn at(home: &Path) -> Homes {
        Homes { home: home.to_path_buf(), codex: home.join(".codex") }
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
    /// The file has the entry `mqttpanel` and it starts this executable.
    pub configured: bool,
}

/// What a button wrote.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub written: Vec<String>,
    /// Files that do not exist on this computer (the host is not installed or never started).
    pub missing: Vec<String>,
    pub failed: Vec<WriteFailure>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WriteFailure {
    pub path: String,
    pub error: mqttpanel_client::UserMessage,
}

fn read_json(p: &Path) -> Option<Value> {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok())
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
    let f = |family: &str, host: &str, p: PathBuf, configured: bool| HostFile { family: family.into(), host: host.into(), path: h.display(&p), exists: p.is_file(), configured };
    vec![
        f("claude", "claudeDesktop", h.claude_desktop(), json_configured(&h.claude_desktop(), exe)),
        f("claude", "claudeCode", h.claude_code(), json_configured(&h.claude_code(), exe)),
        f("chatgpt", "chatgpt", h.codex_config(), toml_configured(&h.codex_config(), exe)),
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
    std::fs::rename(&tmp, p)
}

fn io_fail(h: &Homes, p: &Path, e: std::io::Error) -> WriteFailure {
    WriteFailure { path: h.display(p), error: mqttpanel_client::msg!("file.writeFailed", file = h.display(p), detail = e.kind().to_string()) }
}

fn add_json(h: &Homes, p: &Path, entry: Value, r: &mut WriteReport) {
    if !p.is_file() {
        r.missing.push(h.display(p));
        return;
    }
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) => return r.failed.push(io_fail(h, p, e)),
    };
    let mut v: Value = if text.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return r.failed.push(WriteFailure { path: h.display(p), error: crate::mcp::text::um("hostFileInvalid", &[("file", json!(h.display(p)))]) }),
        }
    };
    let Some(obj) = v.as_object_mut() else {
        return r.failed.push(WriteFailure { path: h.display(p), error: crate::mcp::text::um("hostFileInvalid", &[("file", json!(h.display(p)))]) });
    };
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers[ENTRY] = entry;
    let out = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    match write_keeping(p, &out) {
        Ok(()) => r.written.push(h.display(p)),
        Err(e) => r.failed.push(io_fail(h, p, e)),
    }
}

/// "Add to Claude": Claude Desktop and Claude Code, the files that exist.
pub fn add_to_claude(h: &Homes, exe: &str) -> WriteReport {
    let mut r = WriteReport::default();
    add_json(h, &h.claude_desktop(), json!({ "command": exe, "args": ["mcp"] }), &mut r);
    add_json(h, &h.claude_code(), json!({ "type": "stdio", "command": exe, "args": ["mcp"], "env": {} }), &mut r);
    r
}

/// "Add to ChatGPT": `[mcp_servers.mqttpanel]` in the Codex configuration, created when missing.
pub fn add_to_chatgpt(h: &Homes, exe: &str) -> WriteReport {
    use toml_edit::{value, Array, DocumentMut, Item, Table};
    let mut r = WriteReport::default();
    let p = h.codex_config();
    let text = if p.is_file() {
        match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) => {
                r.failed.push(io_fail(h, &p, e));
                return r;
            }
        }
    } else {
        String::new()
    };
    let mut doc = match text.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(_) => {
            r.failed.push(WriteFailure { path: h.display(&p), error: crate::mcp::text::um("hostFileInvalid", &[("file", json!(h.display(&p)))]) });
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
        Ok(()) => r.written.push(h.display(&p)),
        Err(e) => r.failed.push(io_fail(h, &p, e)),
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

    #[test]
    fn snippet_names_the_executable() {
        let s: Value = serde_json::from_str(&snippet(EXE)).unwrap();
        assert_eq!(s["mcpServers"]["mqttpanel"]["args"], json!(["mcp"]));
    }
}

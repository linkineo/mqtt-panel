//! The MCP server: a local socket owned by the running app (Unix domain socket `mcp.sock` in the data folder, mode
//! 0600; a named pipe on Windows), newline-delimited JSON-RPC through `rmcp`, one host session at a time.

use super::prompts::{self, connections_json, log_text};
use super::text::{self, t};
use super::tools::{self, gives_secret, masked, Call, Outcome, ToolDef, Wait};
use super::{acknowledged, wait_for_ack, AppLink, AssistantEvent, LogEntry};
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};

pub const URI_CONNECTIONS: &str = "mqttpanel://connections";
pub const URI_LOG: &str = "mqttpanel://assistant-log";
pub const URI_BROKER: &str = "mqttpanel://broker";
pub const URI_CHARTS: &str = "mqttpanel://charts";
/// `mqttpanel://topics/{connection}`: the first 200 topics of a connection (id or name).
pub const URI_TOPICS: &str = "mqttpanel://topics/";

/// The rmcp handler of one host session.
pub struct McpServer {
    link: Arc<dyn AppLink>,
    tools: Arc<Vec<ToolDef>>,
}

impl McpServer {
    pub fn new(link: Arc<dyn AppLink>) -> McpServer {
        McpServer { link, tools: Arc::new(tools::registry()) }
    }

    fn lang(&self) -> mqttpanel_client::catalog::Lang {
        self.link.state().assistant.lang()
    }

    fn log(&self, def: &ToolDef, args: &serde_json::Map<String, Value>, outcome: &str, detail: &str, direct: bool) {
        self.log_noted(def, args, outcome, detail, direct, false);
    }

    fn log_noted(&self, def: &ToolDef, args: &serde_json::Map<String, Value>, outcome: &str, detail: &str, direct: bool, confirmed: bool) {
        let hub = &self.link.state().assistant;
        let mut notes = vec![];
        if direct {
            notes.push(t(self.lang(), "secretNote", &[]));
        }
        if confirmed {
            notes.push(t(self.lang(), "confirmedNote", &[]));
        }
        hub.push_log(LogEntry {
            at: crate::types::now_ms(),
            host: hub.host(),
            tool: def.name.into(),
            arguments: masked(def, args),
            outcome: outcome.into(),
            detail: detail.into(),
            note: (!notes.is_empty()).then(|| notes.join(" · ")),
        });
        self.link.emit(AssistantEvent::LogChanged);
    }

    /// Runs one tool call with the acknowledgement, the limits, the log, the toast and the direct-secret warning.
    pub async fn run_tool(&self, name: &str, args: serde_json::Map<String, Value>, waiting: Arc<dyn Fn() + Send + Sync>) -> CallToolResult {
        let lang = self.lang();
        let Some(def) = self.tools.iter().find(|d| d.name == name) else {
            return CallToolResult::error(vec![ContentBlock::text(t(lang, "unknownTool", &[("name", json!(name))]))]);
        };
        let direct = gives_secret(def, &args);
        if def.writes && !wait_for_ack(self.link.as_ref()).await {
            let m = t(lang, "notAllowed", &[]);
            self.log(def, &args, "refused", &m, direct);
            return CallToolResult::error(vec![ContentBlock::text(m)]);
        }
        let limits = self.link.state().assistant.limits();
        let asks = def.args_ask_in_app(&args) && !direct;
        let limit = match def.wait {
            Wait::Call => limits.call,
            Wait::Connect => limits.connect,
            Wait::MaybeUser if asks => limits.user + limits.call,
            Wait::MaybeUser => limits.call,
            Wait::User => limits.user + limits.call,
        };
        let limit = if tools::CONFIRMING.contains(&def.name) { limit + limits.confirm } else { limit };
        let confirmed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let call = Call { link: self.link.clone(), args: args.clone(), waiting, confirmed: confirmed.clone() };
        let r = tokio::time::timeout(limit, (def.handler)(call)).await;
        let logged = def.writes || direct;
        match r {
            Ok(Ok(out)) => {
                let Outcome { text: mut body, data, toast, changed } = out;
                for what in changed {
                    self.link.emit(AssistantEvent::Changed { what: what.into() });
                }
                if def.writes {
                    let toast = toast.unwrap_or_else(|| body.clone());
                    self.link.emit(AssistantEvent::Toast { text: t(lang, "toast", &[("text", json!(toast))]) });
                }
                if logged {
                    self.log_noted(def, &args, "ok", &body, direct, confirmed.load(std::sync::atomic::Ordering::Relaxed));
                }
                if direct && self.link.state().assistant.take_warning() {
                    body = format!("{body}\n\n{}", t(lang, "directSecret", &[]));
                }
                let json = serde_json::to_string_pretty(&data).unwrap_or_default();
                CallToolResult::success(vec![ContentBlock::text(body), ContentBlock::text(format!("```json\n{json}\n```"))])
            }
            Ok(Err(fail)) => {
                let m = text::m(lang, &fail.msg);
                if logged {
                    self.log(def, &args, fail.outcome, &m, direct);
                }
                CallToolResult::error(vec![ContentBlock::text(m)])
            }
            Err(_) => {
                let m = t(lang, "timeout", &[("seconds", json!(limit.as_secs()))]);
                if logged {
                    self.log(def, &args, "timeout", &m, direct);
                }
                CallToolResult::error(vec![ContentBlock::text(m)])
            }
        }
    }
}

fn json_contents(v: &Value, uri: &str) -> ResourceContents {
    ResourceContents::text(serde_json::to_string_pretty(v).unwrap_or_default(), uri).with_mime_type("application/json")
}

impl ToolDef {
    fn args_ask_in_app(&self, args: &serde_json::Map<String, Value>) -> bool {
        args.get("ask_in_app").and_then(Value::as_bool).unwrap_or(true)
    }
}

fn instructions(lang: mqttpanel_client::catalog::Lang) -> String {
    t(lang, "instructions", &[])
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let caps = ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build();
        let mut info = Implementation::new("mqtt-panel", env!("CARGO_PKG_VERSION"));
        info.title = Some("MQTT Panel".into());
        info.website_url = Some("https://mqtt-panel.com".into());
        ServerConfig::new(caps).with_server_info(info).with_instructions(instructions(self.lang()))
    }

    async fn initialize(&self, request: InitializeRequestParams, context: RequestContext<RoleServer>) -> Result<InitializeResult, McpError> {
        context.peer.set_peer_info(request.clone());
        let ci = &request.client_info;
        let host = ci.title.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| ci.name.clone());
        let st = self.link.state();
        st.assistant.set_host(&host);
        self.link.emit(AssistantEvent::Session { connected: true, host: Some(host) });
        if !acknowledged(st) {
            self.link.emit(AssistantEvent::Acknowledge);
            self.link.raise_window();
        }
        self.negotiate_initialize(&request)
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let tools = self
            .tools
            .iter()
            .map(|d| {
                let schema = match (d.schema)() {
                    Value::Object(o) => o,
                    _ => Default::default(),
                };
                let ann = ToolAnnotations::new().read_only(!d.writes).destructive(d.destructive);
                Tool::new(d.name, d.description, Arc::new(schema)).with_annotations(ann)
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let args = request.arguments.clone().unwrap_or_default();
        let token = context.meta.get_progress_token();
        let peer = context.peer.clone();
        let msg = t(self.lang(), "waitingUser", &[]);
        let waiting: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(tok) = token.clone() {
                let peer = peer.clone();
                let m = msg.clone();
                tokio::spawn(async move {
                    let _ = peer.notify_progress(ProgressNotificationParam::new(tok, 0.0).with_message(m)).await;
                });
            }
        });
        Ok(self.run_tool(&request.name, args, waiting).await.into())
    }

    async fn list_resources(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListResourcesResult, McpError> {
        let lang = self.lang();
        let mut items = vec![
            Resource::new(URI_CONNECTIONS, "connections").with_title(t(lang, "res.connections", &[])).with_mime_type("application/json"),
            Resource::new(URI_BROKER, "broker").with_title(t(lang, "res.broker", &[])).with_mime_type("application/json"),
            Resource::new(URI_CHARTS, "charts").with_title(t(lang, "res.charts", &[])).with_mime_type("application/json"),
            Resource::new(URI_LOG, "assistant-log").with_title(t(lang, "res.log", &[])).with_mime_type("text/plain"),
        ];
        let st = self.link.state();
        let mut open: Vec<(String, String)> = crate::state::lock(&st.conns).values().map(|c| (c.id.clone(), crate::state::lock(&c.name).clone())).collect();
        open.sort();
        for (id, name) in open {
            items.push(Resource::new(format!("{URI_TOPICS}{id}"), format!("topics-{id}")).with_title(t(lang, "res.topicsOf", &[("name", json!(name))])).with_mime_type("application/json"));
        }
        Ok(ListResourcesResult::with_all_items(items))
    }

    async fn list_resource_templates(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListResourceTemplatesResult, McpError> {
        let lang = self.lang();
        let items = vec![ResourceTemplate::new(format!("{URI_TOPICS}{{connection}}"), "topics")
            .with_title(t(lang, "res.topics", &[]))
            .with_description(t(lang, "res.topicsDescription", &[]))
            .with_mime_type("application/json")];
        Ok(ListResourceTemplatesResult::with_all_items(items))
    }

    async fn read_resource(&self, request: ReadResourceRequestParams, _context: RequestContext<RoleServer>) -> Result<ReadResourceResponse, McpError> {
        let lang = self.lang();
        let contents = match request.uri.as_str() {
            URI_CONNECTIONS => {
                let v = connections_json(self.link.as_ref());
                ResourceContents::text(serde_json::to_string_pretty(&v).unwrap_or_default(), URI_CONNECTIONS).with_mime_type("application/json")
            }
            URI_LOG => ResourceContents::text(log_text(lang, &self.link.state().assistant.log()), URI_LOG).with_mime_type("text/plain"),
            URI_BROKER => {
                let v = tools::broker::status_json(self.link.state(), lang).map_err(|m| McpError::internal_error(text::m(lang, &m), None))?;
                json_contents(&v, URI_BROKER)
            }
            URI_CHARTS => json_contents(&tools::charts::charts_json(self.link.state()), URI_CHARTS),
            other if other.starts_with(URI_TOPICS) => {
                let needle = &other[URI_TOPICS.len()..];
                let st = self.link.state();
                let not_found = || McpError::resource_not_found(t(lang, "noResource", &[("uri", json!(other))]), None);
                let p = st.store.find(needle).ok_or_else(not_found)?;
                let conn = crate::state::lock(&st.conns).get(&p.profile.id).cloned().ok_or_else(not_found)?;
                let mut v = tools::data::topics_json(&conn, "", tools::data::TOPICS_MAX);
                v["connection"] = json!(p.profile.id);
                json_contents(&v, other)
            }
            other => return Err(McpError::resource_not_found(t(lang, "noResource", &[("uri", json!(other))]), None)),
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }

    async fn list_prompts(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListPromptsResult, McpError> {
        let lang = self.lang();
        let items = prompts::prompts()
            .into_iter()
            .map(|p| {
                let args = p.arguments.iter().map(|(n, k)| PromptArgument::new(*n).with_description(t(lang, &format!("prompt.{k}"), &[])).with_required(false)).collect();
                Prompt::new(p.name, Some(t(lang, &format!("prompt.{}.description", p.key), &[])), Some(args)).with_title(t(lang, &format!("prompt.{}.title", p.key), &[]))
            })
            .collect();
        Ok(ListPromptsResult::with_all_items(items))
    }

    async fn get_prompt(&self, request: GetPromptRequestParams, _context: RequestContext<RoleServer>) -> Result<GetPromptResponse, McpError> {
        let args = request.arguments.clone().unwrap_or_default();
        let Some((description, body)) = prompts::prompt_text(self.link.as_ref(), &request.name, &args) else {
            return Err(McpError::invalid_params(t(self.lang(), "noPrompt", &[("name", json!(request.name))]), None));
        };
        let mut r = GetPromptResult::new(vec![PromptMessage::new_text(Role::User, body)]);
        r.description = Some(description);
        Ok(r.into())
    }
}

// ---------------------------------------------------------------- sessions

/// Answers the first request of a second host with "another assistant is connected", then closes.
async fn refuse<S: AsyncRead + AsyncWrite + Unpin>(link: &dyn AppLink, stream: S) {
    let (rd, mut wr) = tokio::io::split(stream);
    let mut lines = tokio::io::BufReader::new(rd).lines();
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line()).await;
    let id = match first {
        Ok(Ok(Some(l))) => serde_json::from_str::<Value>(&l).ok().and_then(|v| v.get("id").cloned()).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let m = t(link.state().assistant.lang(), "anotherHost", &[]);
    let resp = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": m } });
    let _ = wr.write_all(format!("{resp}\n").as_bytes()).await;
    let _ = wr.flush().await;
    let _ = wr.shutdown().await;
}

/// Serves one connection of the socket: a session when none is open, else the refusal.
pub async fn serve_stream<S>(link: Arc<dyn AppLink>, stream: S)
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let hub = &link.state().assistant;
    if !hub.open_session() {
        refuse(link.as_ref(), stream).await;
        return;
    }
    match rmcp::serve_server(McpServer::new(link.clone()), stream).await {
        Ok(running) => {
            let _ = running.waiting().await;
        }
        Err(e) => {
            if std::env::var("MQTTPANEL_LOG").as_deref() == Ok("1") {
                eprintln!("[mqttpanel] mcp session: {e}");
            }
        }
    }
    link.state().assistant.close_session();
    link.emit(AssistantEvent::Session { connected: false, host: None });
}

/// What Settings, Assistant shows when the socket (named pipe on Windows) cannot be opened: taken by another program
/// (or another copy of the app), or another error with its detail.
pub fn socket_error_message(e: &std::io::Error) -> mqttpanel_client::UserMessage {
    // Windows: ERROR_ACCESS_DENIED (5) or ERROR_PIPE_BUSY (231) when the pipe name already exists.
    let taken = matches!(e.kind(), std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied) || matches!(e.raw_os_error(), Some(5 | 231) if cfg!(windows));
    if taken {
        text::um("socketTaken", &[])
    } else {
        text::um("socketFailed", &[("detail", json!(e.to_string()))])
    }
}

/// The socket of a running server; dropping it stops listening and removes the socket file.
pub struct Listening {
    pub endpoint: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.task.abort();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.endpoint);
    }
}

/// Starts listening on the endpoint of the data folder `dir`. Fails when another running app owns the socket.
#[cfg(unix)]
pub async fn listen(link: Arc<dyn AppLink>, dir: &Path) -> std::io::Result<Listening> {
    use std::os::unix::fs::PermissionsExt;
    let endpoint = mqttpanel_shim::endpoint_in(dir);
    if endpoint.exists() {
        if tokio::net::UnixStream::connect(&endpoint).await.is_ok() {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another MQTT Panel owns the assistant socket"));
        }
        let _ = std::fs::remove_file(&endpoint);
    }
    std::fs::create_dir_all(dir)?;
    let listener = tokio::net::UnixListener::bind(&endpoint)?;
    std::fs::set_permissions(&endpoint, std::fs::Permissions::from_mode(0o600))?;
    let task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(serve_stream(link.clone(), stream));
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            }
        }
    });
    Ok(Listening { endpoint, task })
}

/// Starts listening on the named pipe of this user (Windows). Every instance is created with a security descriptor
/// that grants access to the current user's SID only; the first one must be new (a name already taken, by another
/// copy of the app or by a program of another user, fails here and is shown on Settings, Assistant).
#[cfg(windows)]
pub async fn listen(link: Arc<dyn AppLink>, dir: &Path) -> std::io::Result<Listening> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let endpoint = mqttpanel_shim::endpoint_in(dir);
    let name = endpoint.to_string_lossy().to_string();
    let mut attrs = mqttpanel_shim::winsec::OwnerOnly::new()?;
    // SAFETY: `attrs` points to a valid SECURITY_ATTRIBUTES that lives as long as the accept task.
    let mut server = unsafe { ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create_with_security_attributes_raw(&name, attrs.as_ptr())? };
    let task = tokio::spawn(async move {
        loop {
            if server.connect().await.is_err() {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                continue;
            }
            let next = match unsafe { ServerOptions::new().reject_remote_clients(true).create_with_security_attributes_raw(&name, attrs.as_ptr()) } {
                Ok(s) => s,
                Err(_) => break,
            };
            let connected = std::mem::replace(&mut server, next);
            tokio::spawn(serve_stream(link.clone(), connected));
        }
    });
    Ok(Listening { endpoint, task })
}

//! Adapter-level types, serialised exactly like packages/engine-client/src/types.ts.

use mqttpanel_client::{TrustInfo, UserMessage};
use mqttpanel_tls::{CertDetails, CertSummary};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub native: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionError {
    pub filter: String,
    pub error: UserMessage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub id: String,
    pub name: String,
    pub state: String, // disconnected | connecting | connected | reconnecting | failed
    pub negotiated_version: Option<String>,
    pub requested_version: String,
    pub topics: usize,
    pub msg_rate: f64,
    pub skip_verify: bool,
    pub error: Option<UserMessage>,
    pub reconnect_attempt: u32,
    pub max_packet_size: Option<u32>,
    pub is_local_broker: bool,
    pub subscription_errors: Vec<SubscriptionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub at: i64,
    pub latency_ms: Option<u64>,
    pub negotiated_version: Option<String>,
    pub max_packet_size: Option<u32>,
    pub server_cert: Option<CertDetails>,
    pub tls_version: Option<String>,
    pub verified: Option<bool>,
    pub error: Option<UserMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustPrompt {
    pub profile_id: String,
    #[serde(flatten)]
    pub info: TrustInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectOutcome {
    Connected { status: ConnectionStatus },
    NeedsTrust { trust: TrustPrompt },
    Failed { error: UserMessage },
}

/// Result of one step of the "Connect to this app's broker" flow (TS `AppBrokerStepResult`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBrokerStepResult {
    pub step: String,
    /// created, kept, issued, skipped, trusted, started, running, connected or failed.
    pub result: String,
    /// The password of the user "mqtt-panel" (step `user`), shown behind the eye icon.
    pub password: Option<String>,
    /// The connection outcome (step `connect`).
    pub outcome: Option<ConnectOutcome>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TestOutcome {
    Tested { result: TestResult },
    NeedsTrust { trust: TrustPrompt },
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FileInspection {
    Ok { ok: bool, path: String, #[serde(rename = "fileName")] file_name: String, summary: CertSummary },
    Err { ok: bool, path: String, #[serde(rename = "fileName")] file_name: String, error: UserMessage },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    Cancelled,
    Failed { error: UserMessage },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: Vec<String>,
    pub skipped: Vec<ImportSkip>,
    pub notes: Vec<UserMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkip {
    pub name: String,
    pub reason: UserMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublishProps {
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub payload_format: Option<u8>,
    #[serde(default)]
    pub message_expiry: Option<u32>,
    #[serde(default)]
    pub user_properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSpec {
    pub response_topic: String,
    #[serde(default)]
    pub correlation_data: String,
    #[serde(default = "ten")]
    pub wait_s: f64,
}

fn ten() -> f64 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub topic: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default = "text")]
    pub format: String, // json | text | hex | base64
    #[serde(default)]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
    #[serde(default)]
    pub props: Option<PublishProps>,
    #[serde(default)]
    pub request: Option<RequestSpec>,
}

fn text() -> String {
    "text".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentRequest {
    pub response_topic: String,
    pub correlation_data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub at: i64,
    pub rtt_ms: i64,
    pub payload_text: String,
    pub correlation_data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentEntry {
    pub id: String,
    pub at: i64,
    pub topic: String,
    pub size: usize,
    pub qos: u8,
    pub retain: bool,
    pub status: String, // pending | sent | acked | failed | deletedRetained | repeat
    pub reason: Option<UserMessage>,
    pub repeat_count: Option<u32>,
    pub payload_text: String,
    pub format: String,
    pub request: Option<SentRequest>,
    pub reply: Option<Reply>,
    pub reply_timed_out: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatStatus {
    pub connection_id: String,
    pub running: bool,
    pub interval_s: f64,
    pub sent: u32,
    pub count: Option<u32>,
    pub next_at: Option<i64>,
    pub topic: String,
    pub stopped_reason: Option<UserMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentUpdate {
    pub connection_id: String,
    pub entry: SentEntry,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EngineEvent {
    #[serde(rename_all = "camelCase")]
    Tick {
        connections: Vec<ConnectionStatus>,
        changed: std::collections::HashMap<String, u64>,
        repeats: Vec<RepeatStatus>,
        sent_updates: Vec<SentUpdate>,
        broker: Option<serde_json::Value>,
        broker_log_seq: u64,
    },
    Menu { id: String },
    /// The assistant (docs/MCP.md): toasts, navigation, refreshes, the acknowledgement dialog, prompts for the user.
    Assistant { event: crate::mcp::AssistantEvent },
    /// A web page wants to connect to the broker without a login (SPEC 6): the window asks Allow or Deny.
    #[serde(rename_all = "camelCase")]
    BrokerOrigin { id: String, listener_id: String, kind: mqttpanel_broker::ListenerKind, port: u16, origin: String },
    /// The web page prompt was answered or timed out: the window closes it.
    BrokerOriginDone { id: String },
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

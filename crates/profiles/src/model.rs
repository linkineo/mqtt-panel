//! Profile, settings and template types. Field names serialise to the TypeScript contract
//! (packages/engine-client/src/types.ts).

use mqttpanel_tls::CertSummary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub filter: String,
    pub qos: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastWill {
    pub topic: String,
    pub payload: String,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileField {
    pub file_name: String,
    pub summary: Option<CertSummary>,
    pub in_keychain: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "TlsSettingsFile")]
pub struct TlsSettings {
    /// Accept any broker certificate for this connection (change set 6; replaces `verifyServer`).
    pub skip_verify: bool,
    pub ca_cert: Option<FileField>,
    pub client_cert: Option<FileField>,
    pub client_key: Option<FileField>,
    #[serde(default)]
    pub trusted_fingerprints: Vec<String>,
    #[serde(default)]
    pub sni: String,
    #[serde(default)]
    pub alpn: String,
    /// Change set 11: the client certificate comes from "Generate key and request a certificate" (the key was generated
    /// by the app and never left the secret store) rather than "Import certificate and key". The connection uses the
    /// same certificate blob and client-key secret either way.
    #[serde(default)]
    pub client_key_generated: bool,
}

/// On-disk shape of [`TlsSettings`], also accepting the field `verifyServer` of profiles saved before
/// change set 6: `verifyServer: false` loads as `skipVerify: true`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TlsSettingsFile {
    #[serde(default)]
    skip_verify: Option<bool>,
    #[serde(default)]
    verify_server: Option<bool>,
    #[serde(default)]
    ca_cert: Option<FileField>,
    #[serde(default)]
    client_cert: Option<FileField>,
    #[serde(default)]
    client_key: Option<FileField>,
    #[serde(default)]
    trusted_fingerprints: Vec<String>,
    #[serde(default)]
    sni: String,
    #[serde(default)]
    alpn: String,
    #[serde(default)]
    client_key_generated: bool,
}

impl From<TlsSettingsFile> for TlsSettings {
    fn from(f: TlsSettingsFile) -> Self {
        TlsSettings {
            skip_verify: f.skip_verify.unwrap_or(f.verify_server == Some(false)),
            ca_cert: f.ca_cert,
            client_cert: f.client_cert,
            client_key: f.client_key,
            trusted_fingerprints: f.trusted_fingerprints,
            sni: f.sni,
            alpn: f.alpn,
            client_key_generated: f.client_key_generated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoSchema {
    pub file_name: String,
    pub topic_filter: String,
    pub message_type: String,
    #[serde(default)]
    pub message_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reconnect {
    pub enabled: bool,
    pub min_delay_s: u32,
    pub max_delay_s: u32,
}

impl Default for Reconnect {
    fn default() -> Self {
        Reconnect { enabled: true, min_delay_s: 1, max_delay_s: 30 }
    }
}

/// A saved view (filters, layout, pinned set); kept opaque for the Rust side.
pub type SavedView = serde_json::Value;

/// One field on a topic's own chart, under the payload in the Explorer (change set 8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartField {
    /// JSON path, "$" for the whole payload.
    pub path: String,
    /// sage | clay | slate | brick | ink2
    #[serde(default = "sage")]
    pub colour: String,
    /// line | step | points
    #[serde(default = "line")]
    pub style: String,
    /// left | right
    #[serde(default = "left")]
    pub axis: String,
    #[serde(default)]
    pub unit: String,
}

fn sage() -> String {
    "sage".into()
}
fn line() -> String {
    "line".into()
}
fn left() -> String {
    "left".into()
}

/// What the profile remembers for one topic (change set 8): the value picked with "Use as sparkline" and the fields
/// of the topic's chart; change set 9 adds whether the sparkline is hidden.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparkline_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_fields: Option<Vec<ChartField>>,
    /// Change set 9: the user hid the sparkline of this topic ("Hide sparkline"); a new pick clears it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sparkline_off: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A connection profile as the UI sees it (no secrets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default = "target_other")]
    pub target: String, // "other" | "app"
    #[serde(default)]
    pub broker_listener_id: Option<String>,
    pub host: String,
    pub port: u16,
    pub scheme: String, // mqtt | mqtts | ws | wss
    #[serde(default = "ws_path")]
    pub ws_path: String,
    #[serde(default = "auto")]
    pub mqtt_version: String, // 3.1.1 | 5 | auto
    pub client_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub tls: TlsSettings,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default = "keep_alive")]
    pub keep_alive: u16,
    #[serde(default = "yes")]
    pub clean_start: bool,
    #[serde(default)]
    pub session_expiry: u32,
    #[serde(default)]
    pub last_will: Option<LastWill>,
    #[serde(default)]
    pub reconnect: Reconnect,
    #[serde(default)]
    pub proto_schemas: Vec<ProtoSchema>,
    #[serde(default)]
    pub views: Vec<SavedView>,
    /// Per topic: the picked sparkline value and the topic chart (change set 8).
    #[serde(default)]
    pub topics: BTreeMap<String, TopicPrefs>,
    #[serde(default)]
    pub last_used: Option<i64>,
    #[serde(default)]
    pub last_test: Option<serde_json::Value>,
    #[serde(default)]
    pub is_local_broker: bool,
}

fn target_other() -> String {
    "other".into()
}
fn ws_path() -> String {
    "/mqtt".into()
}
fn auto() -> String {
    "auto".into()
}
fn keep_alive() -> u16 {
    60
}

impl Profile {
    /// The values picked with "Use as sparkline", per topic.
    pub fn sparkline_paths(&self) -> std::collections::HashMap<String, String> {
        self.topics.iter().filter_map(|(t, p)| p.sparkline_path.clone().map(|s| (t.clone(), s))).collect()
    }

    /// A new profile with defaults: subscribed to everything ("#", QoS 0), random client id, plain MQTT on localhost.
    pub fn new_default() -> Profile {
        Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "New connection".into(),
            target: "other".into(),
            broker_listener_id: None,
            host: "localhost".into(),
            port: 1883,
            scheme: "mqtt".into(),
            ws_path: "/mqtt".into(),
            mqtt_version: "auto".into(),
            client_id: mqttpanel_client::random_client_id(),
            username: String::new(),
            has_password: false,
            tls: TlsSettings::default(),
            subscriptions: vec![Subscription { filter: "#".into(), qos: 0 }],
            keep_alive: 60,
            clean_start: true,
            session_expiry: 0,
            last_will: None,
            reconnect: Reconnect::default(),
            proto_schemas: vec![],
            views: vec![],
            topics: BTreeMap::new(),
            last_used: None,
            last_test: None,
            is_local_broker: false,
        }
    }
}

/// Public, non-secret file contents kept with the profile (certificates and .proto files).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileBlobs {
    /// CA certificate(s), PEM.
    #[serde(default)]
    pub ca_pem: Option<String>,
    /// Client certificate chain, PEM (absent when the identity is a PKCS#12 bundle in the keychain).
    #[serde(default)]
    pub client_cert_pem: Option<String>,
    /// .proto files: (file name, content).
    #[serde(default)]
    pub proto_files: Vec<(String, String)>,
}

/// What is persisted per profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredProfile {
    #[serde(flatten)]
    pub profile: Profile,
    #[serde(default)]
    pub blobs: ProfileBlobs,
}

/// Secret keys for a profile in the [`crate::SecretStore`].
pub mod secret_keys {
    pub fn password(id: &str) -> String {
        format!("profile/{id}/password")
    }
    /// Client private key, PEM.
    pub fn client_key(id: &str) -> String {
        format!("profile/{id}/client-key")
    }
    /// PKCS#12 bundle, base64.
    pub fn pkcs12(id: &str) -> String {
        format!("profile/{id}/pkcs12")
    }
    /// Passphrase of the key or the PKCS#12 bundle.
    pub fn passphrase(id: &str) -> String {
        format!("profile/{id}/passphrase")
    }
    pub fn all(id: &str) -> Vec<String> {
        vec![password(id), client_key(id), pkcs12(id), passphrase(id)]
    }
}

/// App settings (see the TS type Settings). An old settings file with `skipVerify` still loads; the field
/// is ignored (skipping verification is per connection since change set 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub accent: String,
    pub density: String,
    pub mono_size: u32,
    pub values_in_tree: bool,
    pub motion: bool,
    pub language: String,
    pub buffer_per_topic: u32,
    pub points_per_series: u32,
    pub memory_budget_mb: u32,
    pub payload_render_limit_kb: u32,
    pub update_check: bool,
    pub update_channel: String,
    /// The broker log is expanded in the right pane of the Broker screen (collapsed by default, change set 2).
    pub broker_log_open: bool,
    /// The day (YYYY-MM-DD) the user accepted the assistant acknowledgement (docs/MCP.md section 3); until then the
    /// assistant's writing tools are refused.
    pub assistant_acknowledged: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            accent: "sage".into(),
            density: "comfortable".into(),
            mono_size: 12,
            values_in_tree: true,
            motion: true,
            language: "system".into(),
            buffer_per_topic: 200,
            points_per_series: 5000,
            memory_budget_mb: 256,
            payload_render_limit_kb: 256,
            update_check: true,
            update_channel: "stable".into(),
            broker_log_open: false,
            assistant_acknowledged: None,
        }
    }
}

impl Settings {
    /// Clamps values to their documented ranges.
    pub fn clamped(mut self) -> Self {
        self.buffer_per_topic = self.buffer_per_topic.clamp(10, 10_000);
        self.points_per_series = self.points_per_series.clamp(100, 1_000_000);
        self.memory_budget_mb = self.memory_budget_mb.clamp(16, 16_384);
        self.payload_render_limit_kb = self.payload_render_limit_kb.clamp(16, 10_240);
        self.mono_size = self.mono_size.clamp(10, 16);
        self
    }
}

/// A publish template (request kept as the UI sends it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub folder: String,
    pub request: serde_json::Value,
    #[serde(default)]
    pub repeat_s: Option<f64>,
}

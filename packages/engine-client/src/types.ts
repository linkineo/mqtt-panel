// Contract between the UI and the engine adapter.
// Every type here is produced by the Rust side with serde (camelCase) and must stay in sync with
// apps/desktop/src-tauri (commands) and crates/engine (view slices). Times are milliseconds since
// the Unix epoch unless the name says otherwise. Sizes are bytes.

export type Qos = 0 | 1 | 2;
export type Scheme = 'mqtt' | 'mqtts' | 'ws' | 'wss';
export type MqttVersionPref = '3.1.1' | '5' | 'auto';
export type NegotiatedVersion = '3.1.1' | '5';

/** A user-facing message: a catalog key plus parameters. `text` is the English rendering, used as a fallback. */
export interface UserMessage {
  key: string;
  params: Record<string, string | number>;
  text: string;
}

// ---------------------------------------------------------------- settings

export interface Settings {
  theme: 'system' | 'light' | 'dark';
  accent: 'sage' | 'slate' | 'clay';
  density: 'comfortable' | 'compact';
  monoSize: number; // px, 10..16
  valuesInTree: boolean;
  motion: boolean; // true = follow the system "reduce motion" setting; false = never animate
  language: 'system' | 'en' | 'fr' | 'de';
  bufferPerTopic: number; // 10..10000, default 200
  pointsPerSeries: number; // default 5000
  memoryBudgetMb: number; // default 256
  payloadRenderLimitKb: number; // default 256
  skipVerify: boolean; // lab only; never persisted, resets at launch
  updateCheck: boolean;
  updateChannel: 'stable' | 'beta';
}

export interface AppInfo {
  version: string;
  platform: 'macos' | 'windows' | 'linux' | 'web';
  /** True when running inside the desktop shell (native dialogs, menus, keychain). */
  native: boolean;
}

// ---------------------------------------------------------------- certificates and files

export interface CertSummary {
  kind: 'certificate' | 'key' | 'pkcs12' | 'bundle';
  subject: string | null; // "CN=broker.plant.example, O=…"
  cn: string | null;
  issuer: string | null;
  notBefore: string | null; // ISO date "2026-09-01"
  notAfter: string | null;
  keyType: string | null; // "ECDSA P-256", "RSA 2048"
  count: number; // certificates in a bundle
  selfSigned: boolean;
  isCa: boolean;
  encrypted: boolean; // encrypted key or PKCS#12 that needs a passphrase
}

export interface CertDetails {
  subject: string;
  issuer: string;
  san: string[];
  notBefore: string; // ISO date
  notAfter: string;
  fingerprint: string; // SHA-256, colon-separated upper-case hex
  keyType: string;
  selfSigned: boolean;
  isCa: boolean;
}

/** A file field in a form. Paths are never typed: they come from the native dialog or a drop. */
export interface FileField {
  fileName: string;
  summary: CertSummary | null;
  /** True when the content (a key or a PKCS#12 bundle) was moved to the OS keychain. */
  inKeychain: boolean;
}

/** Result of inspecting a picked or dropped file. */
export type FileInspection =
  | { ok: true; path: string; fileName: string; summary: CertSummary }
  | { ok: false; path: string; fileName: string; error: UserMessage };

export type FileKind = 'caCert' | 'clientCert' | 'clientKey' | 'proto' | 'import' | 'payload' | 'csr' | 'brokerCert' | 'brokerKey';

// ---------------------------------------------------------------- profiles

export interface Subscription {
  filter: string;
  qos: Qos;
}

export interface LastWill {
  topic: string;
  payload: string;
  qos: Qos;
  retain: boolean;
}

export interface TlsSettings {
  verifyServer: boolean;
  caCert: FileField | null;
  clientCert: FileField | null;
  clientKey: FileField | null;
  trustedFingerprints: string[];
  sni: string;
  alpn: string;
}

export interface ProtoSchema {
  fileName: string;
  topicFilter: string;
  messageType: string; // fully qualified, e.g. "plant.Telemetry"
  messageTypes: string[]; // types found in the file, for the picker
}

export interface SavedView {
  name: string; // "Default" always exists
  layout: 'tree' | 'table';
  search: string;
  filters: Filters;
  expanded: string[];
  pinned: string[];
  columns: TableColumn[];
  sort: TableSort;
}

export interface Profile {
  id: string;
  name: string;
  target: 'other' | 'app'; // "Another broker" | "This app's broker"
  brokerListenerId: string | null; // when target = 'app'
  host: string;
  port: number;
  scheme: Scheme;
  wsPath: string; // default "/mqtt"
  mqttVersion: MqttVersionPref;
  clientId: string;
  username: string;
  hasPassword: boolean; // the password itself lives in the keychain and never reaches the UI
  tls: TlsSettings;
  subscriptions: Subscription[]; // a new profile starts with [{ filter: '#', qos: 0 }]
  keepAlive: number; // seconds
  cleanStart: boolean;
  sessionExpiry: number; // seconds, 0 = none
  lastWill: LastWill | null;
  reconnect: { enabled: boolean; minDelayS: number; maxDelayS: number };
  protoSchemas: ProtoSchema[];
  views: SavedView[];
  lastUsed: number | null;
  lastTest: TestResult | null;
  isLocalBroker: boolean; // the single profile "Local broker" managed by "Connect Explorer here"
}

export interface ImportSummary {
  imported: string[]; // profile names
  skipped: { name: string; reason: UserMessage }[];
  notes: UserMessage[]; // e.g. "Passwords were not imported."
}

// ---------------------------------------------------------------- connections

export type ConnState = 'disconnected' | 'connecting' | 'connected' | 'reconnecting' | 'failed';

export interface ConnectionStatus {
  id: string; // = profile id
  name: string;
  state: ConnState;
  negotiatedVersion: NegotiatedVersion | null;
  requestedVersion: MqttVersionPref;
  topics: number;
  msgRate: number; // messages per second, 10 s window
  skipVerify: boolean;
  error: UserMessage | null;
  reconnectAttempt: number;
  maxPacketSize: number | null; // from CONNACK (MQTT 5)
  isLocalBroker: boolean;
  subscriptionErrors: { filter: string; error: UserMessage }[];
}

export interface TestResult {
  ok: boolean;
  at: number;
  latencyMs: number | null;
  negotiatedVersion: NegotiatedVersion | null;
  maxPacketSize: number | null;
  serverCert: CertDetails | null;
  tlsVersion: string | null;
  verified: boolean | null; // null when not TLS
  error: UserMessage | null;
}

export type TrustReason = 'selfSigned' | 'unknownCa' | 'expired' | 'notYetValid' | 'fingerprintChanged';

export interface TrustPrompt {
  profileId: string;
  reason: TrustReason;
  reasonText: UserMessage;
  leaf: CertDetails;
  chain: CertDetails[]; // as presented, leaf first
  /** True when the chain ends in a private CA certificate: offer "Trust anything this CA signs". */
  caOffer: boolean;
  previousFingerprint: string | null; // for fingerprintChanged
  unsafe: boolean; // expired or not yet valid: trusting is marked unsafe
}

export type ConnectOutcome =
  | { kind: 'connected'; status: ConnectionStatus }
  | { kind: 'needsTrust'; trust: TrustPrompt }
  | { kind: 'failed'; error: UserMessage };

export type TestOutcome =
  | { kind: 'tested'; result: TestResult }
  | { kind: 'needsTrust'; trust: TrustPrompt };

// ---------------------------------------------------------------- explorer

export interface Filters {
  retainedOnly: boolean;
  changedWithinMin: number | null;
  payloadContains: string;
  qos: Qos | null;
  hasValue: boolean;
}

export interface TreeQuery {
  /** Plain text (substring), MQTT wildcard filter (+, #), or regex when it starts with "/". */
  search: string;
  filters: Filters;
}

export interface TreeRow {
  path: string; // full topic path of this node
  name: string; // last segment
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
  topicCount: number; // topics in the subtree (the grey count chip)
  messages: number; // messages since connect, own plus subtree
  isTopic: boolean; // a message was received on this exact path
  value: string | null; // last payload preview, truncated to 80 chars
  retained: boolean;
  lastReceivedAt: number | null; // own or subtree, drives the highlight
  active: boolean; // changed in the last 10 s (own or subtree)
  rate: number; // msg/s, subtree, 10 s window
  spark: number[]; // last 10 cast values of the default series, oldest first; empty when none
  sparkTimes: number[]; // receive times for spark values
  formatHint: string | null; // "Sparkplug B" on spBv1.0 roots, etc.
  pinned: boolean;
}

export interface TreeSlice {
  total: number; // number of visible rows
  start: number;
  rows: TreeRow[];
  seq: number; // engine sequence when the slice was taken
}

export type TableColumn = 'pin' | 'topic' | 'value' | 'spark' | 'updated' | 'rate' | 'messages' | 'qos' | 'retained' | 'size';

export interface TableSort {
  column: TableColumn;
  dir: 'asc' | 'desc';
}

export interface TableQuery extends TreeQuery {
  sort: TableSort;
}

export interface TableRow {
  path: string;
  value: string | null;
  spark: number[];
  sparkTimes: number[];
  lastReceivedAt: number | null;
  rate: number;
  messages: number;
  qos: Qos | null;
  retained: boolean;
  size: number;
  pinned: boolean;
}

export interface TableSlice {
  total: number; // rows matching
  topicsInView: number; // all leaf topics before filters
  start: number;
  rows: TableRow[];
  seq: number;
}

export type PayloadFormat = 'auto' | 'json' | 'text' | 'hex' | 'base64' | 'image' | 'cbor' | 'msgpack' | 'sparkplug' | 'protobuf';

export interface Mqtt5Props {
  contentType: string | null;
  payloadFormat: number | null;
  responseTopic: string | null;
  correlationData: string | null; // UTF-8 when valid, else hex
  messageExpiry: number | null; // seconds
  userProperties: [string, string][];
  subscriptionIds: number[];
  topicAlias: number | null;
}

export interface DecodedPayload {
  format: Exclude<PayloadFormat, 'auto'>; // what was used
  detected: Exclude<PayloadFormat, 'auto'>; // what auto-detection chose
  json: unknown | null; // structured value for json, cbor, msgpack, sparkplug, protobuf
  pretty: string | null; // pretty-printed JSON text when json is set
  text: string | null; // for text
  hex: string | null; // hex dump (offset, bytes, ascii), for binary and hex
  base64: string | null;
  imageDataUrl: string | null; // for image/* content types or magic bytes
  error: UserMessage | null; // decode error; raw bytes still shown in hex
  paged: boolean; // payload above the render limit: raw view only unless "format anyway"
}

export interface MessageView {
  seq: number; // unique per connection
  topic: string;
  receivedAt: number;
  qos: Qos;
  retain: boolean;
  dup: boolean;
  size: number; // original size on the wire
  truncated: boolean; // kept truncated (above 1 MB)
  decoded: DecodedPayload;
  properties: Mqtt5Props | null; // null on 3.1.1
}

export interface HistoryEntry {
  seq: number;
  receivedAt: number;
  size: number;
  qos: Qos;
  retain: boolean;
  changedKeys: string[]; // compared with the previous entry, dotted paths
  preview: string; // short value preview
}

export interface FieldInfo {
  path: string; // JSON pointer style "$.supply.temp"; "$" for the whole payload
  label: string; // "supply.temp"; "value" for the whole payload
  lastValue: string;
  cast: 'number' | 'parsedNumber' | 'boolean' | 'text' | 'null' | 'object' | 'array';
  plottable: boolean;
}

export interface TopicDetail {
  topic: string;
  exists: boolean;
  last: MessageView | null;
  historyCount: number;
  bufferSize: number; // ring buffer capacity for this topic
  messages: number; // since connect
  pinned: boolean;
  fields: FieldInfo[];
  defaultField: string | null; // path of the series used for sparklines
  seq: number;
}

export type DiffKind = 'same' | 'changed' | 'added' | 'removed';

export interface DiffLine {
  indent: number;
  key: string | null; // object key, or null for array items and closing brackets
  path: string | null; // "$.supply.temp" when the line holds a value
  oldValue: string | null; // JSON text of the old value (changed, removed)
  newValue: string | null; // JSON text of the new value (changed, added, same)
  open: string | null; // "{" or "[" when the line opens a container
  close: string | null; // "}" or "]" (with trailing comma handled by the UI)
  kind: DiffKind;
  plottable: boolean;
}

export interface DiffResult {
  mode: 'json' | 'text';
  lines: DiffLine[]; // json mode
  textLines: { kind: DiffKind; text: string }[]; // text mode
  changes: number;
  a: number; // baseline seq
  b: number; // compared seq
}

export interface SeriesData {
  topic: string;
  path: string;
  points: [number, number][]; // [receivedAt, value]
  gaps: number[]; // indexes i where a gap starts before points[i]
  skipped: number;
  firstSkipReason: string | null;
  castRule: 'number' | 'parsedNumber' | 'boolean' | 'mixed' | 'none';
  step: boolean; // default drawing style (true for booleans)
  precisionLoss: boolean;
  count: number; // points recorded in total (before the cap dropped any)
}

export interface MemoryEstimate {
  bytes: number;
  budgetBytes: number;
  points: number;
  messages: number;
}

// ---------------------------------------------------------------- export

export interface ExportOptions {
  scope: { kind: 'topic'; topic: string } | { kind: 'subtree'; prefix: string } | { kind: 'topics'; topics: string[] };
  fromMs: number | null;
  toMs: number | null;
  format: 'csv' | 'json' | 'ndjson';
  includeProperties: boolean;
  rawBase64: boolean;
}

export interface ExportEstimate {
  messages: number;
  topics: number;
  bytes: number;
}

export interface ChartSeriesRef {
  name: string;
  topic: string;
  path: string;
}

/** Outcome of an action that opens a native save dialog. */
export type SaveOutcome = { kind: 'saved'; fileName: string } | { kind: 'cancelled' } | { kind: 'failed'; error: UserMessage };

// ---------------------------------------------------------------- publish

export type PayloadEncoding = 'json' | 'text' | 'hex' | 'base64';

export interface PublishProps {
  contentType: string;
  payloadFormat: number | null;
  messageExpiry: number | null;
  userProperties: [string, string][];
}

export interface PublishRequest {
  topic: string;
  payload: string;
  format: PayloadEncoding;
  qos: Qos;
  retain: boolean;
  props: PublishProps | null; // MQTT 5 only; ignored on 3.1.1
  request: { responseTopic: string; correlationData: string; waitS: number } | null; // MQTT 5 only; correlationData "" = auto
}

export type SentStatus = 'pending' | 'sent' | 'acked' | 'failed' | 'deletedRetained' | 'repeat';

export interface SentEntry {
  id: string;
  at: number;
  topic: string;
  size: number;
  qos: Qos;
  retain: boolean;
  status: SentStatus;
  reason: UserMessage | null; // for failed
  repeatCount: number | null; // for repeat rows
  payloadText: string;
  format: PayloadEncoding;
  request: { responseTopic: string; correlationData: string } | null;
  reply: { at: number; rttMs: number; payloadText: string; correlationData: string } | null;
  replyTimedOut: boolean;
}

export interface RepeatStatus {
  connectionId: string;
  running: boolean;
  intervalS: number;
  sent: number;
  count: number | null;
  nextAt: number | null;
  topic: string;
  stoppedReason: UserMessage | null;
}

export interface Template {
  id: string;
  name: string;
  folder: string;
  request: PublishRequest;
  repeatS: number | null;
}

// ---------------------------------------------------------------- broker

export type ListenerKind = 'tcp' | 'tls' | 'ws' | 'wss';
export type Preset = 'plain' | 'encrypted' | 'passwords' | 'mtls';

export interface Listener {
  id: string;
  kind: ListenerKind;
  bind: 'network' | 'localhost';
  port: number;
  enabled: boolean;
  requireClientCert: boolean;
  preset: Preset;
  login: { username: string; hasPassword: boolean } | null;
}

export interface BrokerConfig {
  listeners: Listener[];
  anonymous: boolean;
  users: { username: string }[];
  verifyAgainst: 'local' | 'imported';
  maxClients: number;
}

export interface BrokerStatus {
  state: 'stopped' | 'starting' | 'running' | 'failed';
  startedAt: number | null;
  clients: number;
  maxClients: number;
  msgInRate: number;
  msgOutRate: number;
  retained: number;
  restartNeeded: boolean;
  error: UserMessage | null;
  listeners: { id: string; up: boolean; error: UserMessage | null }[];
}

export interface IssuedCert {
  name: string;
  serial: string;
  issuedAt: string; // ISO date
  expiresAt: string;
  keyType: string;
  lastSeen: 'connected' | 'never' | number; // number = last seen time
  revoked: boolean;
  system: boolean; // "mqttpanel-desktop", used by Connect Explorer here
}

export interface BrokerCerts {
  ca: CertDetails;
  server: CertDetails;
  imported: boolean;
  importedCa: CertDetails | null; // for "Verify against: Imported CA"
}

export interface BrokerClient {
  clientId: string;
  address: string; // "localhost" for loopback
  protocol: NegotiatedVersion;
  transport: 'TCP' | 'TLS' | 'WS' | 'WSS';
  connectedAt: number;
  subscriptions: number;
  msgIn: number;
  msgOut: number;
  certCn: string | null;
  username: string | null;
}

export interface LogLine {
  seq: number;
  at: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  text: string;
}

export interface AddListenerRequest {
  preset: Preset;
  kind: ListenerKind;
  port: number;
  bind: 'network' | 'localhost';
  login: { username: string; password: string } | null;
}

export interface IssueCertRequest {
  name: string;
  validYears: 1 | 2 | 5;
  keyType: 'ecdsa' | 'rsa';
  format: 'pem' | 'pkcs12';
  passphrase: string; // may be empty for PEM
  saveCa: boolean;
}

// ---------------------------------------------------------------- events

export interface TickEvent {
  type: 'tick';
  connections: ConnectionStatus[];
  /** Per connection: the engine sequence. When it changes, re-request visible slices and the selected topic. */
  changed: Record<string, number>;
  repeats: RepeatStatus[];
  sentUpdates: { connectionId: string; entry: SentEntry }[];
  broker: BrokerStatus | null;
  brokerLogSeq: number;
}

export interface MenuEvent {
  type: 'menu';
  /** Id of a native menu item: 'new-connection', 'open-settings', 'go-connect', 'go-explore', 'go-publish',
   * 'go-charts', 'go-broker', 'command-palette', 'find', 'export', 'clear-history', 'about'. */
  id: string;
}

export type EngineEvent = TickEvent | MenuEvent;

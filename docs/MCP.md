# MCP interface: mini spec for validation

Status: draft for the owner's validation (20 Oct 2026). Nothing here is implemented. Decisions taken with the owner
on 20 Oct 2026 are marked (owner); the rest are proposals to confirm or change.

## 1. Goal

A chatbot (Claude Desktop, Claude Code, ChatGPT, any MCP host) helps the user of MQTT Panel with three jobs:

1. **Connect to an external broker**: create or fix a connection profile, deal with TLS (CA, client certificate, key,
   CSR, pinned certificate), test, connect, explain errors.
2. **Set up the embedded broker**: listeners, users, client certificates, the local CA, exports for devices.
3. **Work with data**: find topics, read values and history, build charts, extract data to files.

The assistant acts through the running app. The user sees every change in the window and in an Assistant log.

## 2. Transport and hosting (owner: stdio shim; seamless Claude and ChatGPT support)

- `mqttpanel mcp` is an MCP server over stdio in the existing CLI binary. It forwards every call to the running app
  over a local socket (Unix socket in the app data folder, named pipe on Windows), same user only. If the app is not
  running, the shim starts it and waits for the socket (owner: start the app).
- The app also serves MCP over streamable HTTP on `127.0.0.1` with a random bearer token, for hosts that only speak
  HTTP. It is off until the user turns it on in Settings; the token is shown there and can be rotated.
- "Seamless" setup, Settings page **Assistant**:
  - **Claude Desktop** and **Claude Code**: one button each, "Add to Claude Desktop" / "Add to Claude Code", which
    writes the `mqttpanel mcp` entry into the host's configuration file after a confirmation that names the file.
  - **ChatGPT**: ChatGPT connects to MCP servers as connectors over HTTP. The spike verifies whether the ChatGPT
    desktop app accepts a `127.0.0.1` connector; if yes, the page offers "Copy connector address" (URL and token) and
    the short list of steps; if ChatGPT only accepts public addresses, MQTT Panel says so on the page and stops there
    (no tunnel, no relay; the app makes no outbound connection for this).
  - **Any other host**: the generic configuration snippet to copy.
- The MCP server is **enabled by default** (owner), reachable only once a host launches the shim or the user turns
  the HTTP endpoint on.

## 3. Security model

- **One-time acknowledgement (owner):** the first time a host connects, and from the Settings page, the app shows one
  sentence to accept once: "Over this interface an assistant can change everything in MQTT Panel, including deleting
  connections, retained messages and broker settings. You stay in control of what you ask it." Until accepted, tools
  that write return "not yet allowed: accept the acknowledgement in MQTT Panel, Settings, Assistant".
- **No in-app confirmations** per tool call (owner); the host's own permission prompts remain the user's gate.
- **Secrets (owner: both ways, with warnings and a recommendation):** every tool that needs a password, a key file,
  a PKCS#12 passphrase or a CSR response has two forms:
  - **recommended**: the tool asks the app to collect the secret itself (`ask_in_app: true`): MQTT Panel opens its
    prompt or file picker, the tool returns "waiting for the user in MQTT Panel" and then the result; the secret never
    passes through the chat;
  - **direct**: the secret or path is given as a tool argument. The tool description says plainly that the value
    travels through the chat transcript and the model provider and recommends the in-app way; the result carries the
    same warning once per session; the Assistant log records "password given through the assistant".
- **Secrets never flow out**: no tool returns a password, a private key or a passphrase; exports of keys go to files
  the user picks in the app.
- **Audit (owner: toast plus Assistant log):** every writing tool call shows a toast ("Assistant: connected Plant
  broker") and appends a row to Settings, Assistant (time, host name, tool, arguments without secrets, outcome), kept
  for the session, exportable as text.
- **Limits**: one host session at a time per transport; tool calls time out after 30 s (connect: 20 s); history reads
  are bounded (section 5); the HTTP endpoint accepts only loopback clients with the token.

## 4. Tools (names and arguments are proposals)

All tools return plain-words text plus a small JSON block; errors reuse the app's catalog messages.

**Connections**
- `list_connections()` → profiles with state (connected, idle, failed), host, protocol, security chip.
- `create_connection(name?, host, port?, protocol, mqtt_version?, username?, client_id?)` → profile id; the name
  follows the host when omitted, like the UI.
- `update_connection(id, …same fields…, keep_alive?, clean_start?, session_expiry?, subscriptions?)`.
- `set_connection_password(id, ask_in_app | password)`; `set_ca_certificate(id, ask_in_app | file)`;
  `set_client_certificate(id, ask_in_app | cert_file, key_file?, passphrase?)`;
  `generate_client_key(id, algorithm?)` → fingerprint; `get_client_csr(id, common_name?)` → PEM text (a CSR is not
  secret); `import_signed_certificate(id, ask_in_app | file)`.
- `test_connection(id)` → result text, certificate details when verification fails, the trust choices available.
- `trust_certificate(id, mode: pin | ca | skip_once | skip_always)`: same choices as the certificate prompt.
- `connect(id)`, `disconnect(id)`, `delete_connection(id)`.

**Embedded broker**
- `broker_status()` → running, listeners with state and errors, counts.
- `broker_start()`, `broker_stop()`.
- `broker_add_listener(kind, port, preset)`, `broker_update_listener(id, enabled?, port?)`,
  `broker_remove_listener(id)`, `broker_set_reachability(this_computer | whole_network)`.
- `broker_set_anonymous(on)`, `broker_add_user(username, ask_in_app | password)`, `broker_remove_user(username)`.
- `broker_client_certs()`; `broker_issue_client_cert(name)` → files written to a folder the user picks in the app
  (PEM or PKCS#12), returns the paths; `broker_revoke_client_cert(name)`; `broker_set_client_cert_rule(optional |
  required, verify_against)`.
- `broker_export_ca(ask_in_app | folder)`, `broker_export_server_cert(…)`, `broker_regenerate_server_cert(extra_names?)`,
  `broker_rotate_ca()`.
- `broker_log(level?, limit?)`.

**Explorer and data (bounded, owner)**
- `list_topics(connection, filter?, limit=200)` → topics with last value, rate, message count, subtopics.
- `get_topic(connection, topic)` → last payload (decoded), properties, counts.
- `get_history(connection, topic, limit=200, before?)` → at most 500 rows per call with time, payload, QoS, size.
- `search_values(connection, text, limit=100)`.
- `publish(connection, topic, payload, qos?, retain?, format?)`; `delete_retained(connection, topic, subtree?)`.
- `export_data(connection, scope: topic | subtree, format: csv | json | ndjson, from?, to?, ask_in_app | file)` →
  the file the user chose in the app and a summary (rows, size).

**Charts**
- `list_charts()`, `create_chart(name)`, `rename_chart(id, name)`, `delete_chart(id)`.
- `add_series(chart, connection, topic, field_path, name?, colour?, style?, axis?, unit?, cast?)`,
  `update_series(…)`, `remove_series(…)`.
- `set_topic_chart(connection, topic, fields[])` and `set_sparkline(connection, topic, path | off)`.
- `export_chart(id, csv | png, ask_in_app | file)`.

**Navigation (owner: yes)**
- `show_connection(id)`, `show_topic(connection, topic)`, `show_chart(id)`, `show_broker()`, `show_settings(page?)`:
  bring the window forward and open the place; never change data.

**Resources** (read-only, cheap): `mqttpanel://connections`, `mqttpanel://broker`, `mqttpanel://charts`,
`mqttpanel://topics/{connection}` (first 200), `mqttpanel://assistant-log`.

**Prompts** (guided flows the host can offer): "Connect me to a broker", "Set up the built-in broker for my
devices" (asks what devices, picks the preset, issues certificates, exports CA.pem), "Chart this value",
"Why can't I connect?" (reads the last error and the certificate prompt, proposes the fix).

## 5. Behaviour rules

- Tools never guess secrets, never skip verification without the explicit `skip_*` mode, never make network calls
  other than to the brokers configured.
- Writing tools report what changed in one sentence and point to where it is in the app.
- Everything a tool can do, the UI can do; nothing is MCP-only (the tools call the same engine functions as the UI).
- Localisation: tool texts follow the app language (EN, FR, DE) so the chat matches the window.

## 6. Settings page "Assistant"

Rows: the acknowledgement (accepted date or the button); hosts: "Add to Claude Desktop", "Add to Claude Code",
"ChatGPT" (address and token when the HTTP endpoint is on, or the explanation), "Other hosts" (snippet, copy); HTTP
endpoint switch with the token and "Rotate"; the Assistant log (table, Clear, Export…). Every row has an ⓘ.

## 7. Tests

- Protocol: an MCP client test harness (Rust, `rmcp` or hand-rolled JSON-RPC) runs every tool against the mock and the
  real engine on temporary homes; the secrets rules (no secret in any result; warning on direct secrets; refusal
  before the acknowledgement) have their own tests; bounds on history.
- Hosts: Claude Code configured by the Settings button, one scripted session that connects to the built-in broker,
  charts a value and exports a CSV; ChatGPT checked by hand in the spike.
- The forbidden-words guard covers the new strings; FR and DE need the native review like everything else.

## 8. Phases

1. Spike (2 days): shim, socket, three tools (`list_connections`, `connect`, `list_topics`), Claude Code and Claude
   Desktop setup buttons, ChatGPT connector check.
2. Connections and certificates tools, secrets rules, acknowledgement, toasts and log.
3. Broker tools, data and charts tools, navigation, resources, prompts, exports.
4. Settings page polish, docs (RUNNING, FEATURES, SPEC 6 security), website section "Works with your assistant".

## 9. Readings to confirm

- The HTTP endpoint exists only for hosts that cannot spawn a process (ChatGPT); it is off until turned on, even
  though MCP as such is "enabled by default".
- "Enabled by default" plus the one-time acknowledgement means: a host can connect at any time, but writing tools
  work only after the sentence was accepted once.
- Direct secrets are allowed but every such tool result carries the warning once per session, and the log says so.
- Navigation tools bring the window forward; they never steal focus while the user is typing in another app (the
  window is raised without activation when MQTT Panel is not frontmost; a toast says the assistant moved the view).
- History reads are capped at 500 rows per call; bigger extracts go through `export_data` to a file picked in the app.
- One host session per transport at a time; a second host gets "another assistant is connected".

# MQTT Panel: technical specification

Version 1.0 target. Draft 2, 12 Oct 2026. Companion documents: `FEATURES.md` (scope and requirements), `DECISIONS.md` (decision log).

## Vision

MQTT Panel is the tool an engineer opens to understand what is happening on a broker: which topics exist, what the last payload was, how a value moved over the last hour, and whether a device answered. It must feel instant with a hundred thousand topics, treat security as a first-class feature (certificates, mutual TLS, OS keychain), and let the same person stand up a local broker in two clicks when they have none. It is a clean rewrite; no code is taken from any existing tool.

Target users: automation and IoT engineers commissioning devices (building automation, industrial, home automation); backend developers integrating with a broker; support teams who need a quick look at a customer's broker from their laptop.

### Deliverables

| Deliverable | Form | Capabilities |
|---|---|---|
| Desktop app | Microsoft Store (Windows, MSIX) and Mac App Store (macOS); Linux from source only (25 Oct 2026) | Everything: TCP, TLS, mTLS, WebSocket, keychain, embedded broker, export, CLI companion |
| CLI | Single binary `mqttpanel`, bundled with the desktop app; built from source when wanted alone | `sub`, `pub`, `broker`, `profiles`, using the same saved connection profiles |
| Web app | Deferred | Not in v1.0. The engine stays I/O-free so a browser build remains possible later (section 9). |

### Non-goals for v1.0
- Web version (deferred). Mobile apps.
- Plugin or scripting system (payload decoders are built in).
- Broker administration of third-party brokers (users, ACLs on EMQX, Mosquitto, HiveMQ).
- Multi-user collaboration, cloud sync, accounts.
- On-disk message history. Everything stays in memory; the user exports what they want to keep.

## 1. UI toolkit choice (desktop only)

With the web version deferred, the question is "what gives the best desktop experience for a dense data tool, on three operating systems, from one codebase, next to a Rust engine and broker, built by one implementer". Candidates were scored 1 (weak) to 5 (strong). Scores are judgement calls from documentation and public track record, not measurements; the spike in section 12 tests them.

| Criterion | Tauri 2 + React | Avalonia (.NET) | Qt 6 Widgets (C++) | Flutter desktop | iced (Rust) |
|---|---|---|---|---|---|
| Consistent rendering on Windows, macOS, Linux | 4 (three WebViews, CSS-controlled; Linux WebKitGTK is the weak one) | 5 (own renderer) | 4 | 5 | 5 |
| Native feel: menus, shortcuts, dialogs, window chrome, scrolling, text selection, context menus | 3 (native menus, dialogs, windows; content is web and needs discipline, see 1.2) | 3 (Fluent look everywhere) | 5 | 3 | 2 |
| Dense data performance (100k virtualised rows, 30 updates/s) | 4 | 5 (TreeDataGrid) | 5 (model/view) | 4 | 4 |
| Ready components: virtualised tree and table, code editor, time-series chart | 5 (TanStack Virtual, CodeMirror 6, uPlot) | 4 | 4 (charting modules are GPL) | 4 | 2 |
| Accessibility, IME, screen readers | 4 | 4 | 5 | 4 | 2 |
| Fit with the Rust engine and broker | 5 (same process) | 2 (C ABI, or MQTTnet instead) | 2 (cxx-qt or C ABI) | 3 (flutter_rust_bridge) | 5 |
| Build and contributor cost, packaging | 5 | 4 | 2 | 3 | 3 |
| Binary size and startup | 5 (about 10 MB) | 3 (50 to 70 MB) | 3 | 3 | 5 |
| Licence fit with an Apache 2.0 based application | 5 | 5 | 3 (LGPL core fine; Qt Charts and QCustomPlot are GPL) | 5 | 5 |
| **Total** | **40** | **35** | **33** | **34** | **33** |

### 1.1 Decision (adopted 12 Oct 2026)

**Tauri 2 with a React UI, hardened for native feel.** It has every component this product needs ready and proven, the tightest fit with the Rust engine and broker, the smallest binary and the lowest contributor cost. Its weakness, web content inside a native window, is addressed by the checklist in 1.2 and tested in the spike. Avalonia was the runner-up (own-drawn renderer, .NET, engine via C ABI or MQTTnet). Qt is the most native-looking but costliest, and its chart modules are GPL. Flutter adds a second language for little gain. Rust-native toolkits (iced, egui, GPUI, Slint) are not ready for a v1 with a code editor, charts, accessibility and IME; iced is worth re-evaluating for v2.

The spike gathers evidence for the native-feel checklist on all three OSes; anything that fails becomes a task, not a reason to switch.

### 1.2 Native-feel checklist for the Tauri app
- Native menu bar, context menus and keyboard shortcuts through Tauri's menu API, with platform conventions (⌘ on macOS, Ctrl elsewhere; Preferences under the app menu on macOS, under Edit or File elsewhere).
- Native window chrome, title bar and traffic lights; native file open and save dialogs; native notifications only when the user asks for them. On macOS the window uses the overlay title bar: the traffic lights are placed vertically centred in the app's 44 px title row (`trafficLightPosition`), the app name starts to the right of them on the same centre line, and nothing of ours sits under the lights (owner, 13 Oct 2026: the name was not aligned with the buttons). The title bar holds the app name and the app mark only; state lives on the rail as small badges (play on Broker while the broker runs, a count bubble on Connect while connections are open) and in the connection bar (change set 4, owner: the pill was awkward). Under the title bar the rail runs the full height on the left; the connection bar is the first row of the main column to its right and is always present (change set 8, owner, 15 Oct 2026). Text inputs never auto-capitalise, auto-correct or spell-check.
- System font stack and platform text rendering; tabular figures for numbers; no web fonts.
- No web-isms: no text cursor or selection on chrome, no link hover underlines, no rubber-band artefacts, no drag images; text selection and copy work everywhere data is shown.
- Scrollbars styled per platform (overlay on macOS, classic on Windows and Linux); smooth scrolling that matches the OS.
- Theme follows the OS instantly (light, dark, reduced motion, high contrast); optional use of the OS accent colour.
- Keyboard-first: every action reachable without the mouse; visible focus rings; a command palette.
- Runtime prerequisites documented: WebView2 Evergreen bootstrapper included in the Windows installer; minimum WebKitGTK version on Linux stated and checked at startup with a clear message.
- A visual regression suite on all three OSes to catch WebView differences before users do.

## 2. Architecture

One rule: the UI never touches sockets, parsing or storage. It talks to an **engine** through a typed interface. The engine is a pure Rust crate, no I/O and no threads, driven by events and queried for view slices. It exposes a C ABI as well as a Rust API, so it can be consumed from Tauri commands today and from .NET, C++ or WebAssembly if the toolkit decision or the web question ever changes.

```
UI layer (React in Tauri 2): views, state, i18n, charts; talks only to EngineClient
        |  EngineClient: commands in, batched events out (<= 30 Hz), view slices on request
Desktop adapter (Tauri 2 IPC: commands + channels). Engine runs in the Rust process.
        Keychain, file dialogs, broker control live here (no auto-update: the stores update the app).
Other adapters (not built in v1.0): C ABI for Avalonia or Qt; WASM in a Worker for a later web build.
        |
crates/engine (Rust, pure): topic store (radix tree + flat index), per-topic ring buffers,
        series with casting rules, decoders (JSON, text, hex, base64, Sparkplug B, CBOR,
        MessagePack, Protobuf with runtime .proto schemas), diff, search, pinned sets, views,
        export (CSV, JSON, NDJSON). No I/O, no threads.
crates/client   MQTT 3.1.1 and 5 over TCP, TLS (rustls), WebSocket. Candidate: rumqttc.
crates/broker   Embedded broker in library mode. Candidate: rmqtt. Fallback: rumqttd.
                Alternative: bundle Mosquitto as a managed sidecar process (EPL/EDL).
crates/cli      mqttpanel sub | pub | broker | profiles. clap. Shares profiles with the app.
crates/profiles Profile storage, OS keychain (keyring), import and export without secrets.
```

## 3. Repository layout

```
mqtt-panel/
  apps/
    desktop/          Tauri 2 app (Rust shell, menus, bundling, store packaging)
  packages/
    ui/               React components, routes, state, i18n catalogs (en, fr, de)
    engine-client/    EngineClient interface + the Tauri adapter (+ a mock for UI tests)
  crates/
    engine/           core state machine (pure; Rust API + C ABI)
    client/           native MQTT transport (rumqttc or equivalent)
    broker/           embedded broker wrapper, cert generation, presets, config
    cli/              mqttpanel binary
    profiles/         connection profile storage, keychain, import/export
  docs/               SPEC.md, FEATURES.md, DECISIONS.md, mockups/
  scripts/            release, signing, visual tests, check-forbidden-words
  .github/workflows/  CI matrix (win, mac, linux), release with tauri-action
```

## 4. Data model (engine)

| Entity | Fields (essentials) | Notes |
|---|---|---|
| ConnectionProfile | id, name, host, port, scheme (mqtt, mqtts, ws, wss), path (WS), mqttVersion (3.1.1, 5, auto), clientId, username, credentialRef, tls {caCert, clientCert, clientKey, skipVerify (per connection, change set 6; an old verifyServer: false loads as skipVerify: true), trustedFingerprints, sni, alpn}, subscriptions [{filter, qos}] (a new profile starts with one entry: "#", QoS 0), topics {<topic>: {sparklinePath, sparklineOff, chartFields [{path, colour, style, axis, unit}]}} (change set 8: the picked sparkline value and the topic chart, saved per topic; change set 9: sparklineOff when the user hid it), keepAlive, cleanStart, sessionExpiry, lastWill, reconnect policy, target (custom or embedded broker listener) | Passwords and keys never live in the profile file. The form shows scheme, host and port together in one address field mirrored by the port box (change set 5); host is stored bare, without scheme or port; `credentialRef` points into the OS keychain. Export of profiles strips secrets unless the user sets a passphrase. |
| TopicNode | path segments, childCount, messagesSinceConnect (own plus subtree), lastMessage ref, lastReceivedAt (drives the highlight), retained flag, activity (rate per 10 s window), pinned | Radix tree for the tree view and a flat sorted index for the table view and search. |
| Message | timestamp (receive), topic ref, payload bytes, qos, retain, dup, packetId, mqtt5 {contentType, responseTopic, correlationData, userProperties, messageExpiry, payloadFormat} | Ring buffer per topic, default 200 messages, configurable 10 to 10 000. Payloads above 1 MB are kept truncated with the full size recorded. |
| Series | topic ref, valuePath (JSON pointer, optional), cast rule, points [(t, f64)], skipped count, max points | A series is one topic plus one field, never the whole payload; the engine records a series for every numeric and boolean field automatically, charts show only the fields the user picks (owner, 12 Oct 2026). Any JSON attribute or whole payload can be charted. Cast to a plottable number: integers and floats as they are, numeric strings parsed, booleans and boolean-like text (true/false, T/F, on/off, yes/no, 1/0, case-insensitive) as 0/1 drawn as a step series. Values that cannot be cast are skipped and counted, never break the series. Default 5 000 points, oldest dropped. |
| View | name, subscription filters, search, pinned topics, layout (tree/table), column set | Saved per profile. |
| PublishTemplate | name, topic, payload, format, qos, retain, mqtt5 props | Per profile, created only on "Save as template". Also "last 500 sent" log entries, re-sendable. |
| BrokerConfig | bind (network/localhost, one for the whole broker; change set 4, an old per-listener value is ignored, network wins when any listener had it), listeners [{kind: tcp/tls/ws/wss, port 1024..65535, enabled, preset}], requireClientCert (broker-wide, encrypted listeners only; change set 2), verifyAgainst, certs {ca, server, issuedClients[]}, auth {anonymous, users[]}, maxClients, retained store | Stored next to the profiles. The broker's own CA, server and client certificates are generated locally. |

## 5. Performance plan

| Target | Approach |
|---|---|
| 100 000 topics in the tree, scrolling at 60 fps | Virtualised tree and table (only visible rows rendered). The UI requests "rows between index a and b for view v"; the engine answers with a flat slice. No full tree object crosses the IPC boundary. |
| 5 000 messages per second sustained | Engine ingests at wire speed on its own thread. UI updates are coalesced into one diff per animation frame, capped at 30 per second. Per-topic ring buffers keep memory bounded. |
| Payloads up to 10 MB | Pretty-printing and diff are lazy and only for the selected topic. Above 256 KB the payload view switches to a paged raw view with "format anyway" on demand. |
| Startup under 1 s | No bundled runtime; the shell launches with a prebuilt bundle. Profiles are read lazily. |
| Memory | Ring buffer and series caps are user settings with a live memory estimate in Settings. "Clear history" per connection and per topic subtree. |

## 6. Security

- **TLS stack:** rustls with the platform root store (`rustls-platform-verifier` or `rustls-native-certs`). Supports a custom CA, client certificate and key (PEM; PKCS#12 converted on import), SNI override and ALPN. **Self-signed and private-CA brokers are supported:** when the server certificate cannot be verified, the app shows it (subject, issuer, SAN list, SHA-256 fingerprint, expiry) and offers one action for a self-signed certificate, "Trust this certificate", stored by fingerprint with a one-line scope underneath ("This certificate only, for this profile. If the broker presents a different one, you are asked again."); if the fingerprint later changes, the connection stops and the prompt returns. Only when the chain ends in a private CA certificate does a second button appear, "Trust anything this CA signs", which fills the profile's CA field (owner, 12 Oct 2026: two similar options were confusing). Skipping verification is a per-connection choice (owner, 14 Oct 2026, change set 6): the certificate prompt offers "only this time" and "always for this connection" next to pinning, and the profile's Advanced fold has the switch "Skip unverified broker certificate" with the same prompt. There is no app-wide setting any more (an old settings file with the flag loads and ignores it); the CLI keeps `--insecure` per invocation. A connection with the check skipped or a pinned fingerprint shows the ⚠ icon with a tooltip on its tab and card.
- **File pickers:** certificate and key fields open the native file dialog and accept drag-and-drop; paths are never typed. Every security field has an info bubble in plain words (texts in `FEATURES.md`).
- **Secrets:** passwords, private keys and PKCS#12 passphrases never sit in profile files; profile files contain references only. Storage (changed 13 Oct 2026 after the owner hit a keychain prompt on every broker restart and at start-up on the unsigned build): the OS keychain (`keyring`: Keychain, Credential Manager, Secret Service) holds exactly one item for MQTT Panel, a random 256-bit master key. Every other secret lives in `secrets.enc` in the app data folder, encrypted and authenticated with that key (XChaCha20-Poly1305 or AES-256-GCM from an allowed-licence crate), with the version and a per-record nonce. The master key is read lazily, the first time a secret is needed in a session, never at start-up, and cached in memory; a broker start or restart uses the cached material and never touches the keychain. The master key is the only keychain item the app ever reads or writes (changed 13 Oct 2026, change set 3): there is no migration of the per-secret entries of earlier test builds, they are simply ignored. When the keychain refuses the key (the user clicks Deny, or the keychain is locked), the app does not ask again in that session; secrets are unavailable until the next start and every place that needs one says so. Effect: at most one keychain prompt per session on an unsigned build, none on a signed build after "Always allow". A broker whose server key is missing from the file issues a new server certificate on start; a missing CA key regenerates the local CA (logged, issued client certificates are no longer trusted). The CLI shares the same file and key; it is a separate program for the keychain, so it has its own one-time prompt.
- **Where the master key lives, by build (owner, 18 Oct 2026, change set 10: the keychain dialogs on the unsigned builds were friction; regular users of a signed release must never see one):** the rule follows the build, there is no settings toggle.
  - **Signed release builds** keep the key in the OS secret store, which is silent on every platform: on macOS the **Data Protection keychain** (`kSecUseDataProtectionKeychain` with the mqtt-panel.com keychain access group; access is decided by the code-signing entitlement, so no dialog can appear, not even once, and updates keep access), on Windows **DPAPI** in user scope (`CryptProtectData`, silent), on Linux **Secret Service** (silent while the desktop keyring is unlocked, which it is from login) with the owner-only key file below as the fallback where no keyring exists. Until the macOS build is signed, a build signed with a local identity uses the login keychain as before.
  - **Unsigned builds** (dev, CI, an unsigned beta; on macOS detected at the first secret use by the absence of a Developer ID team identifier in the code signature, i.e. ad-hoc or none) keep the key in an owner-only file, `master.key` (mode 0600, folder 0700) in the app data folder, so no dialog ever appears in a test cycle. The ⓘ next to the password and key fields says where the key lives.
  - **Moving the key** (first start after this change, or the first signed start later): the key is read once from the old place, written to the new one and the old item deleted; nobody is asked twice. On an unsigned build with an existing login-keychain item this is the last dialog that build family ever shows; a refused keychain keeps the `keychain.refused` behaviour and the move is tried again at the next start. The CLI follows the same rule on the same data folder. `MQTTPANEL_SECRETS=memory` is unchanged; `MQTTPANEL_KEYSTORE=file|keychain` forces the store for tests.
  - **Security trade-off, stated plainly:** the key file is readable by any program running as the user, the same level as an ssh key without a passphrase, a kubeconfig or the AWS CLI credentials; the OS stores restrict access to the app. That is why the release keeps the OS store and the file is for unsigned builds only.
- **Shell hardening (Tauri):** strict Content Security Policy, capabilities limited to the commands the UI needs, no shell or arbitrary file access from the WebView, no updater (the stores update the app).
- **Embedded broker:** anonymous access off by default, TLS listener generated with a local CA on first start, client certificates optionally required. The broker binds to all interfaces by default (one broker-wide choice since change set 4, "This computer only / Whole network") so devices on the network can connect without extra steps; the Broker screen shows a clear warning and an explainer, and every listener can be switched to localhost in one click. The plain TCP listener carries an extra warning when reachable from the network.
- **Assistant interface (MCP, `docs/MCP.md`):** the running app owns a same-user local socket (`mcp.sock`, mode 0600, in the data folder; a named pipe on Windows), no network port; hosts reach it through the stdio shim `mqttpanel mcp` (or the app executable with `mcp`). Writing tools work only after the one-time acknowledgement was accepted; secrets are asked in the app by default (`ask_in_app`), a secret given as an argument is warned about and logged, and no tool result ever returns a password, a private key or a passphrase.
- **Privacy:** no telemetry, no crash reporting, no network calls except to the brokers the user configured. There is no update check; the stores update the app (25 Oct 2026).
- **Supply chain:** `cargo deny` and `npm audit` in CI, licence allow-list (MIT, Apache 2.0, BSD, ISC, MPL 2.0), lockfiles committed, release builds reproducible from CI only.

Needs human review: export compliance for shipping TLS code in signed builds (owner, before the first release).

## 7. Embedded broker

The broker runs inside the desktop app process (and as `mqttpanel broker` in the CLI) and is managed from a Broker screen. The Explorer can connect to it with one click, and the active configuration is offered as a target in the Connections form.

| Area | v1.0 |
|---|---|
| Protocols | MQTT 3.1.1 and 5, QoS 0 to 2, retained messages, last will, shared subscriptions if the library supports them. |
| Listeners | Defaults TCP 1883, TLS 8883, WebSocket 8083, secure WebSocket 8084. Each independently enabled, with a Network / Localhost switch and a configurable port in the range 1024 to 65535; ports below 1024 are refused with an explanation, and a port already in use is reported before start. The WS listeners serve browser-based dashboards and tools on the network. |
| Setup presets | Offered in the Add listener flow, not as a standalone block. Choosing a setup for a new listener configures that listener and the shared certificates and authentication it needs: **Plain, lab only** (TCP, anonymous, marked unencrypted); **Encrypted** (local CA and server certificate, no client authentication); **Encrypted with passwords** (server certificate plus username and password per client); **Mutual TLS** (server certificate and one or several named client certificates issued from the local CA, exportable as PEM or PKCS#12 for devices). Each preset has an info bubble explaining what it protects against and when it is enough. Defaults (confirmed): ECDSA P-256 keys, CA valid 5 years, server and client certificates 2 years. |
| Client certificates | The prominent section: issue per device, import CSR, list with name, issued, expiry, last seen, export PEM or PKCS#12, revoke. |
| Broker certificates | Local CA and server certificate (rcgen) with chosen SANs, validity and key type; import PEM or PKCS#12; subject, issuer, SAN list, fingerprint and expiry; export the CA; rotate. Collapsed behind a disclosure row with a one-line summary. |
| Authentication | Anonymous toggle, which when on disables the users list and client-certificate checks together (greyed, kept; confirmation prompt; turning it off restores the previous setup); one shared username and password list for every listener (argon2-hashed at rest, plain password kept in the keychain so the UI can reveal and copy it); require client certificate (mutual TLS) per TLS listener. No per-listener login: one broker, several ports (owner, 13 Oct 2026). |
| Observability | Connected clients (id, address, protocol, subscriptions, uptime), message counters with incoming and outgoing shown as two separately coloured tiles (owner, 12 Oct 2026), retained message count, live log with level filter. Disconnect a client. |
| As a client target | Once a broker configuration is active it appears as a selectable target in the Connections form. Choosing it fills host, port, scheme, the CA and, for mutual TLS, the chosen client certificate and key. Reached from Connections, "New connection", "MQTT Panel broker" (existing listener or add one; change set 5 renamed it): the app creates the user "mqtt-panel", the client certificate "mqttpanel-desktop" if required, trusts its CA, starts the broker if it is stopped and connects, updating the single profile "Local broker" (change set 1, 13 Oct 2026; no restart since change set 7). |
| Live apply | Every change applies while the broker runs (change set 7, owner, 14 Oct 2026): listeners start, stop or rebind one by one, TLS material is swapped for new handshakes, the client-certificate rule and the users are read live. Only the sessions a change invalidates are dropped (see `FEATURES.md`, Broker); a listener that cannot start fails alone. The runtime therefore owns one task per listener, swappable TLS material behind an `ArcSwap`/`RwLock`, and a session registry that can drop sessions by listener, by login kind or by certificate issuer. The status has no `restartNeeded` flag; a per-listener `error` replaces it. |
| Later (v1.1) | Topic ACLs per user, bridge to another broker, persistence of retained messages across restarts. |

The chosen library must be verified against this table before implementation starts (section 10).

## 8. Command line

Four subcommands, nothing else: `sub`, `pub`, `broker`, `profiles`. Every command and subcommand answers `--help`. Defaults are chosen so that the simplest case needs no flags beyond the topic.

### Start here
```sh
# watch everything on a broker on this machine (host defaults to localhost, port to 1883)
mqttpanel sub -t "#"

# watch one topic on another machine
mqttpanel sub -h 10.20.4.17 -t building/f2/r204/temperature

# send a message
mqttpanel pub -h 10.20.4.17 -t test/hello -m "hello"

# run a local broker with the settings from the desktop app (or the defaults if none)
mqttpanel broker
```

### Help
```text
$ mqttpanel --help
MQTT Panel command line

Usage: mqttpanel <command> [options]

Commands:
  sub        Subscribe to topics and print messages
  pub        Publish one message (or one per line from stdin)
  broker     Run the embedded broker
  profiles   List, export and import saved connection profiles
  mcp        Connect an assistant (MCP host) to the running app

Options:
  --profile <name>   Use a saved connection profile (from the desktop app)
  -h, --host <host>  Broker host (default: localhost)
  -p, --port <port>  Broker port (default: 1883, or 8883 with --tls)
  --tls              Connect with TLS
  -u, --user <name>  Username (password is asked for, or read from the keychain)
  --format <fmt>     Output: text (default), json, ndjson, raw
  -V, --version      Print the version
  --help             Show this help; also works after any command

$ mqttpanel sub --help
Subscribe to topics and print messages

Usage: mqttpanel sub [options] -t <filter> [-t <filter> ...]

Options:
  -t, --topic <filter>   Topic filter, repeatable ("#" for everything)
  -q, --qos <0|1|2>      Subscription QoS (default: 0)
  --retained-only        Print retained messages only, then exit
  --count <n>            Exit after n messages
  --timeout <s>          Exit after s seconds without a message
  --cafile, --cert, --key, --insecure   TLS options (see mqttpanel sub --help tls)

$ mqttpanel pub --help
Publish one message, or one message per line from stdin

Usage: mqttpanel pub [options] -t <topic> (-m <payload> | --stdin | --file <path>)

Options:
  -q, --qos <0|1|2>      QoS (default: 0)
  -r, --retain           Set the retain flag
  --delete-retained      Clear the retained message on the topic
  --repeat <s>           Send again every s seconds until Ctrl-C
```

### More
```sh
# use a profile saved in the desktop app, print NDJSON (one message per line)
mqttpanel sub --profile plant-broker -t "building/+/+/temperature" --format ndjson

# ad hoc TLS with a client certificate
mqttpanel sub -h broker.example.net -p 8883 --cafile ca.pem --cert client.pem --key client.key -t "#"

# self-signed broker: trust its fingerprint for this call, or use --insecure in a lab
mqttpanel sub -h 10.20.4.17 --tls --trust-fingerprint 3F:9A:...:C1 -t "#"

# publish from an argument, a file or stdin; QoS 1; retained
mqttpanel pub --profile plant-broker -t building/f2/r204/setpoint -m '{"value":21.5}' -q 1 -r
mqttpanel pub --profile plant-broker -t building/f2/r204/setpoint --file payload.json
cat payload.json | mqttpanel pub --profile plant-broker -t building/f2/r204/setpoint --stdin

# delete a retained message
mqttpanel pub --profile plant-broker -t building/f2/r204/setpoint --delete-retained

# broker with an explicit config file, or override the bind address
mqttpanel broker --config ./broker.toml
mqttpanel broker --bind 127.0.0.1

# profiles
mqttpanel profiles list
mqttpanel profiles export --without-secrets > profiles.json
```

Output formats: `text` (default, human: time, topic, payload), `json`, `ndjson`, `raw` (payload bytes only, for piping). Exit codes: 0 ok, 1 connection or TLS error, 2 bad arguments, 3 refused by the broker. Errors are one plain sentence with the reason and the fix, the same wording as in the app. The CLI reads the same profile store and keychain as the app, so a profile created in the UI works immediately on the command line.

## 9. Deferred: web version

Dropped from v1.0 on 12 Oct 2026 so that all effort goes into the desktop experience. Kept possible at no cost by two rules: the engine crate has no I/O, so it can be compiled to WebAssembly later; and the UI depends on the shell only through the EngineClient adapter. If it comes back, a browser build would be limited to MQTT over WebSocket, without client certificates, OS keychain or the embedded broker.

## 10. Libraries and their status

Facts verified on 12 Oct 2026 from the projects' own pages are marked Verified. Everything else is a candidate to verify at implementation start, by reading the current docs and running a spike.

| Need | Candidate | Licence | Status |
|---|---|---|---|
| Desktop shell | Tauri 2 | MIT / Apache 2.0 | Verified: 2.0 is stable; check the current 2.x minor at kick-off. |
| Native MQTT client | rumqttc | Apache 2.0 | Verified: MQTT 3.1.1 and 5. Verify: TLS and WebSocket feature flags, maintenance cadence. |
| Embedded broker | rmqtt (library mode) | MIT / Apache 2.0 | Verified: MQTT 3.1, 3.1.1, 5, TLS, WS, WSS, embeddable. Verify: API stability, client-cert auth hooks, retained store. |
| Embedded broker, fallback | rumqttd | Apache 2.0 | Verified: embeddable, MQTT 3.1.1 and 5, TLS. Verify: WebSocket listener and release activity (last crates.io release seen was Dec 2023). |
| Embedded broker, alternative | Mosquitto as a managed sidecar | EPL 2.0 / EDL 1.0 | Verify: licence compatibility for bundling and per-platform binaries. |
| TLS | rustls + rustls-platform-verifier | Apache 2.0 / MIT / ISC | Verify |
| Certificate generation | rcgen | MIT / Apache 2.0 | Verify |
| Keychain | keyring | MIT / Apache 2.0 | Verify: Linux Secret Service backend; fallback encrypted file with passphrase prompt. |
| Sparkplug B | Protobuf schema via prost | Apache 2.0 | Verify |
| CBOR | ciborium | Apache 2.0 | Verify |
| Protobuf with user .proto files at runtime | prost-reflect + protox (parse .proto without protoc) | MIT / Apache 2.0 | Verify: dynamic message decoding and import resolution. |
| CLI parsing | clap | MIT / Apache 2.0 | Standard |
| UI | React 18+, TypeScript, Vite | MIT | Standard |
| Virtualised lists | TanStack Virtual | MIT | Verify |
| Charts | uPlot (fast canvas time series) | MIT | Verify: theming hooks. |
| Code and payload editor | CodeMirror 6 | MIT | Standard |
| i18n | Lingui or i18next with ICU messages | MIT | Pick one |
| State | Zustand | MIT | Standard |
| Windows signing | Microsoft Store (signs the MSIX) | n/a | Decided 25 Oct 2026: no code-signing certificate; a self-signed certificate only for local MSIX tests (`docs/WINDOWS_BUILD.md`). |

## 11. Quality and release

- **Engine tests:** Rust unit tests plus a corpus of recorded packet streams (JSON fixtures) replayed into the engine, asserting tree state, series, casting and diffs.
- **Integration:** CI starts the embedded broker and runs the native client and the CLI against it, including TLS and mTLS with generated certificates, plus Mosquitto and EMQX in Docker to catch broker-specific behaviour.
- **UI:** component tests (Vitest) and flow tests (Playwright against the UI with a mocked engine; WebDriver through tauri-driver for smoke tests on Windows and Linux).
- **Visual regression:** screenshots of every screen in light and dark, EN and DE (longest strings), on the three desktop platforms in CI; any diff blocks a release. The validated mockups in `docs/mockups/` are the reference.
- **Performance gate:** a benchmark fixture with 100k topics and a 5k msg/s replay; the UI must keep frame time under 16 ms on the CI runner's baseline.
- **Native-feel review:** the section 1.2 checklist walked through on each OS before every release.
- **Secrets gate (release, change set 10):** the signed macOS build stores the master key in the Data Protection keychain and shows no keychain dialog at install, first start, first secret use, after an update or from the CLI, checked on a fresh user account; Windows uses DPAPI and Linux Secret Service with the file fallback, checked the same way. A build that shows a keychain dialog to an end user does not ship (TESTPLAN V-12).
- **Release (owner, 25 Oct 2026):** two stores and the source, nothing else.
  - **Mac App Store:** the sandboxed variant (`tauri.mas.conf.json`, `Entitlements.mas.plist`), built, signed with the mqtt-panel.com Apple distribution identity and uploaded to App Store Connect by `scripts/release-mac-testflight.sh`; TestFlight first, then review.
  - **Microsoft Store:** an MSIX package built on a Windows machine from the Tauri release executable and `apps/desktop/windows/AppxManifest.xml` by `scripts/build-msix.ps1`, uploaded unsigned to Partner Center; the Store signs it, so no code-signing certificate is bought. Steps in `docs/WINDOWS_BUILD.md`.
  - **GitHub:** the repository is the source for people who build MQTT Panel themselves. No downloadable installers (no DMG, MSI, NSIS, AppImage, deb or rpm, no Homebrew cask or winget manifest), no auto-updater, no Linux packages; Linux users build from source. A local `.dmg` or `.app` from `cargo tauri build` stays a developer build for the owner's own testing.
  - **Signing:** beta builds are not signed; signing is the final step before a store upload. Signing keys stay in the OS keychain or the CI secret store, never in the repository.
- **Licence:** Apache License 2.0 with the Commons Clause License Condition v1.0 (owner, 25 Oct 2026), for every version and commit: free to use, copy, modify and build, including in companies; not for resale. Source-available, not OSI open source. Manifests point to `LICENSE` (`license-file` in Cargo, `SEE LICENSE IN` in npm). Dependencies keep their own licences (allow-list above).
- **Website:** https://mqtt-panel.com, one Cloudflare Worker, EN/FR/DE, no cookies or third-party resources (owner, 18 Oct 2026; built in `website/`).

## 12. Implementation phases

1. **Spike (1 to 2 weeks):** engine crate with topic store, ring buffers and casting; rumqttc client; rmqtt broker with TLS and mTLS; the 100k / 5k benchmark. In parallel, a Tauri 2 shell with the Explorer tree on all three OSes, walked through the native-feel checklist. Outcome: evidence per checklist item per OS; failures become phase 2 tasks.
2. **Core desktop:** profiles + keychain, connection form with TLS, Explorer tree and table, inspection with JSON, diff, properties, and the decoders (Sparkplug B, CBOR, MessagePack, Protobuf with user .proto files).
3. **Publish and history:** composer, templates, sent log, delete retained, series and chart screen, export.
4. **Broker:** embedded broker screen, Add listener flow with presets, client certificates, authentication, clients, log. CLI.
5. **Polish and release:** FR and DE catalogs with native review, accessibility pass, native-feel review, Mac App Store and Microsoft Store packages (no installers, no auto-update, 25 Oct 2026), website.

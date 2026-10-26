# Decisions in force

The standing decisions behind MQTT Panel. The detailed behaviour is in `docs/SPEC.md`, `docs/FEATURES.md`,
`docs/MCP.md` and the mockups; this file says what was chosen and, where it is not obvious, why.

## Product

| Topic | Decision |
|---|---|
| Name | MQTT Panel. The app, the website and the store listings reference https://mqtt-panel.com only; no company or other brand name, no personal names. The predecessor tool is never named in public material. |
| Scope | Desktop MQTT client (Explorer, Publish, Charts, export) with an embedded broker managed in the same window, a simple CLI (`sub`, `pub`, `broker`, `profiles`, `mcp`) and an MCP interface for AI assistants. |
| History | In memory only, never written to disk; selected data is exported (CSV, JSON, NDJSON; chart CSV and PNG). Any JSON field can be charted, cast to a number (numbers as-is, numeric strings parsed, booleans as 0/1 steps; uncastable values skipped and counted). |
| Publish | A standalone Publish screen with a sent log, QoS, retain and delete-retained; one-off by default, a template only on "Save as template"; quick publish in the Explorer detail pane. |
| Broker | Embedded broker with setup presets in "Add listener" (plain lab, encrypted, encrypted with passwords, encrypted with client certificates), each explained in an ⓘ bubble. Shared users only (no per-listener login). Reachability is one broker-wide choice, "Whole network" by default with a warning. "Connect to this app's broker" creates and maintains one profile, "Local broker", which follows every broker change. |
| Live apply | Every broker change applies while the broker runs; there is no Restart button or pending state. Only the sessions a change invalidates are dropped. |
| | Why: a restart dropped every connection for a change that concerned one listener or one user; live apply keeps the others open. Where continuity and security conflict, security wins (removing a user or changing a password drops that user's sessions; turning Anonymous off drops sessions without a login). |
| Platforms | Desktop: macOS and Windows as store builds; Linux builds from source. The web version is deferred, not dropped. |
| Distribution | Mac App Store and Microsoft Store, updated by the stores. macOS also as a signed and notarized universal DMG on the GitHub release (26 Oct 2026), which updates itself (see Updater). No Windows installer outside the store; GitHub is otherwise for building from source. |
| Updater | 26 Oct 2026: the direct macOS download (and it alone) checks for updates and installs them when the user asks (`docs/UPDATER.md`): cargo feature `self-update`, Tauri's updater plugin driven from Rust, at most one automatic check a day (switch in Settings, Updates, on by default) plus "Check now". The app asks `https://mqtt-panel.com/update/<platform>/<arch>/<version>`; the Worker redirects to `latest.json` on the latest GitHub release and keeps nothing. Archives are signed with the project's minisign key and verified by the plugin. Store and developer builds compile no updater code. |
| | Why: a direct download that never updates leaves users on old versions with known bugs; the stores refuse self-updating apps, so only the DMG gets it. The address stays under the project's domain, the files on the GitHub release. |
| Signing | The Microsoft Store signs the MSIX (no Windows code-signing certificate). macOS is signed for the Mac App Store (Apple Distribution, sandboxed) and for the direct download (Developer ID, hardened runtime, notarized). Developer builds are unsigned. |
| Licence | Apache License 2.0 with the Commons Clause License Condition v1.0 (`LICENSE`, `NOTICE`). Free use, including at work and for one's own commercial activities; no selling MQTT Panel or a product or service whose value derives substantially from it. |
| | Why: the owner wants free professional use without third parties reselling the app; that makes MQTT Panel source-available, not OSI open source, and it is never called "open source". |
| Repository | https://github.com/linkineo/mqtt-panel (public). |

## Look and wording

| Topic | Decision |
|---|---|
| Mockups | `docs/mockups/mqtt-panel-mockups.html` is the validated UI reference (1280 × 700). The UI must match it; deviations go through a spec change first. |
| Look | Developer-tool density, identical on all platforms, light and dark, app palette with Sage (default), Slate and Clay accents. Native menus, shortcuts, dialogs and window chrome on each OS. Rail labels are verbs: Connect, Explore, Publish, Charts, Broker, AI, Settings. |
| Labels | Plain words, never jargon ("New broker…", "Built-in broker", "Server name (SNI)", "Protocol tag (ALPN)"; no "mTLS", no "embedded"). Form rows never wrap: helper text short, the explanation in the ⓘ bubble. |
| Files | Users never type file paths: every file field has a Browse button (native dialog) and accepts drag-and-drop. |
| ⓘ bubbles | Every security option has one, in plain words, in EN, FR and DE. |
| App mark | A decorative pixel mark in the title bar (`AppMark`, `aria-hidden`, no text naming it). |

## Stack

| Topic | Decision |
|---|---|
| Core | Rust engine (`crates/engine`: topic store, history, decoders, diff, search, export), pure, with a C ABI, independent of the UI toolkit. |
| UI | Tauri 2 with React and TypeScript, hardened for a native feel (`docs/SPEC.md` 1.2). Chosen after a desktop comparison with Avalonia, Qt, Flutter and iced. npm workspaces. |
| MQTT client | Own session layer (`crates/client`) on rumqttc's packet codec. |
| | Why: rumqttc's event loop hides the socket, which the SNI override, precise network errors, the "plain on a TLS port" probe, the trust prompt's certificate chain and 3.1.1/5 auto-negotiation need. |
| Broker | Own compact broker (`crates/broker`) on tokio, tokio-rustls and tokio-tungstenite, reusing the same codec; MQTT 3.1.1 and 5, QoS 0 to 2. |
| | Why: rmqtt, rumqttd and a Mosquitto sidecar lacked in-process control (keys from the secret store, live revocation, per-connection certificate name, clients list) or brought licences outside the allow-list. |
| TLS | rustls with the `ring` provider; RSA 2048 keys generated with the `rsa` crate. `aws-lc-rs` is avoided because `aws-lc-sys` carries the OpenSSL licence. |
| Errors | One catalog for app and CLI (`packages/ui/src/i18n/<lang>/errors.json`), embedded by the Rust side so both print the same sentence. |
| Dependencies | MIT, Apache 2.0, BSD, ISC, MPL 2.0, plus permissive data licences listed in `deny.toml`; no GPL, LGPL-static or AGPL. `cargo deny` and `npm audit` in CI; lockfiles committed. |

## Security and secrets

| Topic | Decision |
|---|---|
| Network | No telemetry, no crash reporting, no accounts, no network calls except to configured brokers. |
| Secrets | Passwords, private keys and passphrases live in `secrets.enc` (XChaCha20-Poly1305) in the data folder, never in profile files or the repository. Its master key is the only item kept outside it. |
| Key store | The master key's place follows the build: signed builds use the silent OS store (macOS Data Protection keychain, Windows DPAPI, Linux Secret Service with a file fallback); unsigned and ad-hoc builds use `master.key`, readable by the user only. The key is read at the first secret use, never at start-up; a refusal is remembered for the session. `MQTTPANEL_KEYSTORE=file|keychain` forces the choice. |
| | Why: the macOS login keychain identifies an unsigned program by its build hash, so every rebuild asked for the password again; a signed release must never show a keychain dialog (release gate, `docs/SPEC.md` 11, `docs/TESTPLAN.md` V-12). |
| Trust | Self-signed and private-CA brokers are trusted per profile by certificate fingerprint or CA; skipping verification is per connection, off by default, and marked with ⚠. |
| Broker certificates | ECDSA P-256 by default; local CA valid 5 years, server and client certificates 2 years. The server certificate names include localhost, the host name, its `.local` name and the LAN addresses, plus "Extra names". |
| Broker limits | Built-in limits instead of settings: 128 topic levels, 64 waiting connections per listener and 8 per address, 64 KiB CONNECT before login, 1 s delay on a refused login, 4 password checks at once, in-flight resend after 30 s and disconnect after 60 s (values in `docs/SPEC.md` 6). |
| | Why: one client, or anyone who can reach a listener, must not be able to stop or exhaust the app that hosts the broker; the values stay far above what devices need. |
| Web pages | Browser connections to the WS listeners are a standard use, even anonymous. With Anonymous on, a page connects only from an origin on the listener's list; an unknown origin is asked about in the app (Allow adds it). Pages that log in and device clients are not affected. |
| | Why: without the check any web page the user opens could read and publish through an anonymous broker; asking once per origin keeps dashboards working without a setting to find first. |
| Long topics | Topics are kept cut at 1024 bytes by default (per profile, `maxTopicBytes`); levels share one string per topic. |
| | Why: a broker the user explores can send any topic; memory per topic must stay linear and bounded. |

## Assistant (MCP)

| Topic | Decision |
|---|---|
| Transport | The app is the MCP server (`rmcp`) on a same-user local socket (`mcp.sock`, named pipe on Windows). Hosts start the stdio shim `mqttpanel mcp` or the app executable with `mcp`; the shim starts the app when needed. One host at a time. Marked beta. |
| | Why no HTTP endpoint: Claude Desktop, Claude Code and the ChatGPT desktop app all launch stdio servers, and nothing in the app should listen on the network for this. |
| Safety | Enabled by default; writing tools work after a one-time acknowledgement in the app. Secrets are asked in the app by default; a secret given in the chat works but is warned about and logged. Every writing call shows a toast and a row in the Assistant log. |
| | Why the acknowledgement: an assistant can change everything in the app, so the user accepts that once, knowingly; the host's own permission prompts stay the per-call gate. |
| Confirmations | A short list of actions waits for Allow or Deny in the app (Anonymous on, whole network, skip verification for good, moving a profile that holds a credential, adding a web origin while Anonymous is on, writing outside the usual folders, replacing a file); 60 s without an answer is a denial. |
| | Why: text the assistant reads (payloads, topic names, log lines) can steer it, and another local program can reach the socket; these actions expose the broker, send a stored credential elsewhere or overwrite user files, so a person decides each time. |
| Windows pipe | Owner-only security descriptor on the named pipe; the shim checks that the pipe's server runs as its own user. |
| | Why: the pipe namespace is machine-wide, so another user could otherwise create the name first and read or answer the assistant's traffic. |
| Texts | Tool names, descriptions and schemas in English (they are read by the model); results, errors and prompts in the app language. |

## Languages

| Topic | Decision |
|---|---|
| Languages | English is the source; French and German ship in v1.0, written natively and reviewed by a native reader before release. |
| French | « vous », French typography (no-break spaces before `: ; ? !` and inside « »). |
| German | "du" everywhere. |
| Protocol terms | Stay in English where practitioners use them (QoS, retain, topic, broker, client ID). |
| Demo data | Screenshot and demo data is localised per language. |
| Fitting | When a French or German string does not fit, the layout gives way first; the string is shortened only after that. |

## Readings awaiting the owner

Implementation choices where the spec left room. Each is easy to change.

- Licence allow-list additions in `deny.toml`: Unicode-3.0, Unicode-DFS-2016, Zlib, CC0-1.0, 0BSD, BSL-1.0, CDLA-Permissive-2.0 (none copyleft).
- Licence text in `LICENSE`, `NOTICE` and the README summary: legal review before release (human review required).
- Website legal page: the operator details are a placeholder; what to publish is a legal decision for the owner.
- Website privacy page: the sentence on Cloudflare's processing of technical data needs the owner's (and, where required, legal) review.
- Website: the official Mac App Store badge needs Apple's marketing licence accepted by the owner; until then a neutral button is shown. The store links are empty until the listings are live.
- Listener table: the "unencrypted" chip sits in the Kind cell (mockup: next to the reachability choice) so the table fits in French and German.
- Series editor: the unit hint is an ⓘ bubble instead of helper text, so the row stays on one line in every language.
- "Skip unverified broker certificate": the label follows the switch in the control column (mockup 02d: label column), because it is too long for the label column in every language.
- Mockup 02b: the explainer is one generic sentence ("if the listener requires one", "if needed") instead of wording that changes with the selected listener.
- A revealed password longer than 12 characters shows its first and last four characters ("Vq7m…kR2p"); the full value is in the tooltip and on the clipboard.
- Message rates are whole numbers (messages in the last completed second); mockups 04 and 06 show fractions, which would need a longer averaging window (a one-line change).
- The play button on an idle card connects and stays on the Connections screen; Connect elsewhere opens the Explorer.
- Under reduced motion the new-message highlight switches on and off without a fade; there is no activity dot.
- A leaf-only imported server certificate is trusted for "Local broker" by pinning its fingerprint.
- MCP: "nothing listens until a host starts the shim" is read as "nothing on the network"; the same-user socket exists while the app runs.
- Settings page "Assistant" has no mockup yet; it follows the other Settings pages.
- Import from other tools is hidden in the UI; the parsers were checked against synthetic fixtures only.
- The assistant confirmation dialog, the web page prompt and the "Allowed web pages" detail of a WS listener are not in the validated mockups; they reuse the existing modal, chip and ⓘ styles and need the owner's look.
- `update_connection` asks before a host or port change of a profile with a stored credential, not before a protocol change alone (mqtts to mqtt would send the password unencrypted to the same host); one line to add if wanted.
- Limits: the 8 waiting connections per address apply to localhost too; a CONNECT over 64 KiB (for example a large last will) is refused before the login; a full in-flight window closes with reason 0x98; MQTT 3.1.1 clients that subscribe deeper than 128 levels are disconnected (no reason code exists).
- The topic length limit (1024 bytes) is a profile value without a form field.
- The `null` origin (sandboxed pages, local files) is refused without a question, because any page can send it.

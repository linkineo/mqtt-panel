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
| Distribution | Mac App Store and Microsoft Store only, updated by the stores. No downloadable installers, no updater, no update check. GitHub is for building from source. |
| Signing | The Microsoft Store signs the MSIX (no Windows code-signing certificate). macOS is signed for the Mac App Store. Developer builds are unsigned. |
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

## Assistant (MCP)

| Topic | Decision |
|---|---|
| Transport | The app is the MCP server (`rmcp`) on a same-user local socket (`mcp.sock`, named pipe on Windows). Hosts start the stdio shim `mqttpanel mcp` or the app executable with `mcp`; the shim starts the app when needed. One host at a time. Marked beta. |
| | Why no HTTP endpoint: Claude Desktop, Claude Code and the ChatGPT desktop app all launch stdio servers, and nothing in the app should listen on the network for this. |
| Safety | Enabled by default; writing tools work after a one-time acknowledgement in the app. Secrets are asked in the app by default; a secret given in the chat works but is warned about and logged. Every writing call shows a toast and a row in the Assistant log. |
| | Why the acknowledgement and no per-call confirmation: an assistant can change everything in the app, so the user accepts that once, knowingly; the host's own permission prompts stay the per-call gate. |
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

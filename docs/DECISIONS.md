# Decisions

## Decisions in force

| Topic | Decision |
|---|---|
| Name | MQTT Panel, by mqtt-panel.com. No personal names in app, site or repo. The predecessor tool is never named in public material. |
| Platforms | Desktop only for v1.0: Windows, macOS, Linux. Web version deferred (12 Oct 2026). |
| Core | Rust engine (topic store, history, decoders, export) with a C ABI, and a Rust broker, independent of the UI toolkit. |
| UI toolkit | Tauri 2 with a React UI, hardened for native feel (12 Oct 2026). Comparison and checklist in `SPEC.md` section 1. |
| History | In memory only, with export of selected data (CSV, JSON, NDJSON; chart CSV and PNG). Any JSON attribute can be charted, cast to a plottable value. |
| Publish | A standalone Publish screen with a sent log, QoS, retain and delete-retained; one-off by default, templates only on "Save as template"; quick publish in the Explorer detail pane. |
| Broker | Embedded broker managed in the same UI. Setup presets offered when adding a listener (plain lab, encrypted, encrypted with passwords, mutual TLS with issued client certificates), each explained in an info bubble. Client certificate management is the prominent section; broker certificates are collapsed. Listeners default to all interfaces with a warning and a one-click switch to localhost. Active configuration selectable as a target in the Connections form. Desktop and CLI only. |
| CLI | Simple: sub, pub, broker, profiles. No capture or export command in v1. |
| Languages | EN (source), FR, DE from v1.0. |
| Look | Developer-tool density, identical on all platforms, light and dark, mqtt-panel palette with Sage (default), Slate and Clay accents. Native menus, shortcuts, dialogs and window chrome on each OS. Rail labels are verbs: Connect, Explore, Publish, Charts, Broker, Settings. Sent log in the right column of Publish. Tree rows show values and sparklines by default. Plain words in user-facing labels, never jargon; form rows never wrap (helper text short, explanation in the ⓘ bubble). |
| Mockups | `docs/mockups/mqtt-panel-mockups.html` is the validated UI reference (22 screens, 1280×700, validated by the owner on 12 Oct 2026). The UI must match it. |
| Licence | Apache 2.0, confirmed by the owner (12 Oct 2026). Dependencies: MIT, BSD, Apache 2.0, ISC, MPL 2.0; no GPL or AGPL. |
| Repository | github.com/linkineo/mqtt-panel, private until release. Website: mqtt-panel.com. |
| Signing | Beta builds are not signed on Windows or macOS. Signing (Windows) and signing plus notarization (macOS) are the final step before a release. Windows: the cheapest option that stays hassle-free for end users; candidate Azure Trusted Signing, eligibility and price to verify. |
| Model split | Spec model writes specs, architecture and reviews; implementing model builds the code. |

## Decision log

### 12 Oct 2026, kickoff answers
- Rust core with a TypeScript UI; React.
- History not persisted to disk; export of selected data instead.
- Publish as an independent function with a sent-message log, QoS, retain, delete.
- Name: MQTT Panel. mqtt-panel.com branding.
- Added scope: embedded MQTT broker with certificates managed in the same interface; simple CLI (subscribe, publish, run the broker).
- Deliverable flow: review artifact first, then Markdown and HTML in the repo.

### 12 Oct 2026, review comments on the artifact
- The predecessor tool may be named in internal specs only, never publicly.
- Any JSON attribute can be charted; values cast to a plottable number (numbers as-is, numeric strings parsed, booleans and boolean-like text as 0/1 step; uncastable values skipped and counted).
- Broker setup presets with info bubbles; active broker configuration selectable as a client target. Presets later moved into the Add listener flow only.
- Web version dropped for v1.0 to focus on the best desktop UI; deferred, not deleted.
- UI toolkit: Tauri 2 (after a desktop-only comparison with Avalonia, Qt, Flutter and iced).
- Licence: MIT considered, then Apache 2.0 kept.
- Protobuf with user `.proto` files in v1.0.
- Request and response (MQTT 5) in v1.0, shown only when MQTT 5 was negotiated; negotiated protocol version always visible.
- CLI stays simple.
- Broker listeners default to all interfaces with warning and explainer; per-listener switch to localhost. Ports configurable in 1024 to 65535.
- Self-signed and private-CA brokers supported by trusting the certificate fingerprint per profile; lab-only skip-verification off by default.
- CLI section rewritten with simple examples and help output.
- Windows: ship unsigned was considered, then reversed to signed releases (betas unsigned).
- Tree rows flash on new messages (collapsed ancestors too) and show a per-node message count since connection.
- Certificate fields are file pickers with drag-and-drop; info bubbles for CA certificate, client certificate, client key, SNI and ALPN.
- New profiles subscribe to everything ("#") by default; filter chips removable with an X.
- Sparklines show the last 10 cast values, never the message rate.
- Sending never creates a template; quick publish row in the Explorer detail pane.
- Client certificate management is the prominent Broker section; broker certificates collapsed behind a disclosure row.
- No default connection; "Connect Explorer here" reuses one profile "Local broker" and picks the most secure enabled listener (mutual TLS, TLS, TCP), with a per-listener explicit option.

### 12 Oct 2026, mockup review
- Plain labels: the Connections "Target" choice reads "Another broker" / "This app's broker"; no "custom" or "embedded" in user-facing text. Form rows never wrap; helper text short, full explanation in the ⓘ bubble. SNI and ALPN as separate rows, "Server name (SNI)" and "Protocol tag (ALPN)"; keep-alive and session expiry as separate rows.
- Trust dialog: one action "Trust this certificate" for a self-signed certificate, with a one-line scope; "Trust anything this CA signs" appears only when the chain ends in a private CA.
- A series is one topic plus one field, never the whole payload. The engine keeps history for every numeric and boolean field; charts show only picked fields. "Add to chart" opens a picker (checkbox per field, nothing preselected, text fields greyed "not plottable", custom path, target chart). The Chart tab shows charted fields as removable chips.
- Right-click (or the hover chart icon) on any key or value in the Payload and Diff views offers "Add <field> to chart", "Copy path" and "Copy value"; text fields disabled.
- Series cards are editable in place: name, topic, field (typed path or a click on a key or value in the topic's last message shown under the field), colour, style (line, step, points), axis, unit, Remove. Charts can be renamed, duplicated and removed.
- Broker messages in and out are shown as two separately coloured tiles.
- Per-listener login: one optional username and password per listener, keychain-stored, masked, edited in place and in Add listener step 2. `login {username, credentialRef}` added to the listener model.
- Anonymous gates everything: confirmation prompt; users list, per-listener logins and the whole client-certificate section disabled (greyed, kept); mutual-TLS listeners show "not checked"; the pill shows "anonymous"; turning it off restores the previous setup.
- Passphrase ⓘ bubble in Issue client certificate: protects only the exported file, not the broker login nor a listener password; the device asks for it once at install.
- Request-and-response ⓘ bubble in Publish explains the MQTT 5 mechanism (response topic, correlation ID, automatic subscription, reply shown next to the sent entry; not available on 3.1.1).
- Mockups validated: `docs/mockups/mqtt-panel-mockups.html`, 22 screens at 1280×700, is the implementation reference.

### 12 Oct 2026, final answers
- Repository `linkineo/mqtt-panel`, website `mqtt-panel.com`.
- Windows signing: cheapest hassle-free option; betas unsigned on both desktop platforms; signing is the last step before release.
- Import sources in v1.0: the predecessor tool's settings file and MQTTX; MQTT.fx later.
- Design defaults confirmed: verb rail labels, sent log right column, values and sparklines in the tree, Sage accent.
- Broker certificate defaults confirmed: ECDSA P-256, CA valid 5 years, server and client certificates 2 years. Lab-only skip-verification off by default, visible in the connection pill while active.
- Workflow: after the Markdown, a detailed mockup artifact is reviewed; validated mockups are committed as HTML before implementation starts.

## Needs human review (owner timing)
- Licence Apache 2.0: confirmed 12 Oct 2026.
- Export compliance for TLS code in signed builds: before the first release.
- Windows signing certificate or Azure Trusted Signing setup (procurement and IT): at the end, before the first signed release.
- Public copy (README, site, release notes) must not name the predecessor tool: checked by `scripts/check-forbidden-words.sh` before each release.

## Open
- Awaiting owner confirmation (per-listener login): (1) a listener's own login is accepted in addition to the shared users list, not instead of it; (2) on a mutual-TLS listener the login is optional and the client certificate stays required. Implemented as stated until the owner says otherwise.
- Library candidates are verified in the spike (`SPEC.md` section 10).

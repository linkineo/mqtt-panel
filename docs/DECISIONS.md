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
| Mockups | `docs/mockups/mqtt-panel-mockups.html` is the validated UI reference (22 screens validated by the owner on 12 Oct 2026, 1280×700; change set 1 of 13 Oct 2026 added 02b and 12c and edited 02, 12, 12b, 14, 19). The UI must match it. |
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
- (Closed 13 Oct 2026) Per-listener login: dropped altogether by the owner, see change set 1.
- Library candidates are verified in the spike (`SPEC.md` section 10).
- Small layout deviations from the mockups made so French and German fit (listed under "13 Oct 2026, phases 2 to 4 UI" below): the "unencrypted" chip sits next to the listener kind instead of next to the Network/Localhost switch; the broker uptime sits on the Status tile's label line; the series unit hint is an ⓘ bubble. Awaiting owner confirmation; easy to revert.
- Change set 1 (13 Oct 2026), readings of the spec awaiting owner confirmation, details under "13 Oct 2026, change set 1" below:
  - Mockup 02b explainer: the app shows one generic sentence ("issue the client certificate mqttpanel-desktop if the listener requires one … restart the broker if needed") instead of wording that changes with the selected listener; the Anonymous variant is separate. The FR and DE texts suggested in the brief were generic too.
  - Mockup 12b still draws the users as greyed chips; the app keeps the users table (greyed, buttons disabled) under Anonymous, so the layout does not jump.
  - A revealed password longer than 12 characters is shown as its first and last four characters ("Vq7m…kR2p", as drawn in mockup 12); the full value is in the tooltip and on the clipboard. Inputs (passphrases, dialogs) reveal the full text.
  - The 02b dialog lists enabled listeners only; a switched-off listener is not offered (it would need switching on first). With none enabled, the dialog says so and preselects "Add a listener…".
  - Changing the password of "mqtt-panel" on the Broker screen does not update the profile "Local broker" by itself; the next "This app's broker" run brings the profile up to date. Should the Broker screen do it at once?

### 13 Oct 2026, owner feedback after the first run (change set 1)
Owner feedback on the running app, 13 Oct 2026. Specified in `docs/FEATURES.md` (Connections, Broker), `docs/SPEC.md` section 7, mockups 02, 02b, 12, 12b, 12c, 14, 19, and `docs/TESTPLAN.md` C-12, C-13, B-06, B-10, B-12, B-15.
- "New connection" becomes a menu: "Another broker…" (the form) or "This app's broker" with "Use an existing listener" and "Add a listener…". The Target row leaves the form.
- "This app's broker" does everything itself: user "mqtt-panel" with a strong password, client certificate when required, CA trust, broker start or restart, connect; one profile "Local broker".
- A listener change that needs a restart prompts "Restart now" or "Later"; on "Later" the Restart button pulses with "Restart broker and apply changes" until pressed (static highlight under reduced motion).
- "Connect Explorer here" and "Connect Explorer to this listener" are removed.
- One broker, several ports: per-listener logins are removed; the users table in Authentication is the only login store. Authentication moves above Client certificates.
- Passwords and passphrases the app owns can be revealed with an eye icon and copied.
- The test button reads "Test" (FR « Tester », DE „Testen“).
- Test lab scripts (`tools/testlab/`, Python, mosquitto and Paho): mosquitto in five security modes for the app to connect to; Paho clients against the app's broker with a 500 msg/s generator over 50 topics.

## Implementation decisions

Log of decisions taken by the implementing model where the specs leave room. Each entry is dated; deviations from the spec say why.

### 13 Oct 2026, phase 1 kick-off
- **Workspace layout.** As in SPEC section 3, plus one support crate `crates/tls` (`mqttpanel-tls`): certificate, key and PKCS#12 parsing, fingerprints, and certificate generation (local CA, server and client certificates, CSR signing, PKCS#12 export, the F-01 test kit). Client, broker, desktop app and CLI all need it; keeping it separate avoids a dependency from the client on the broker.
- **npm workspaces instead of pnpm.** The repository uses npm workspaces (`packages/engine-client`, `packages/ui`, `apps/desktop`) and a root `package-lock.json`; npm is already on every machine. `HANDOFF.md` commands read `npm` accordingly.
- **Crypto provider: ring only.** rustls, rcgen and the TLS stack use the `ring` provider. `aws-lc-rs` is avoided because `aws-lc-sys` carries the OpenSSL licence, outside the allow-list. ring cannot generate RSA keys, so RSA 2048 keys (issue client certificate "RSA 2048 for old devices", T-17) are generated with the `rsa` crate and signed through rcgen; the `rsa` crate is used for key generation only.
- **Licence allow-list in `deny.toml`.** Besides MIT, Apache 2.0, BSD, ISC and MPL 2.0, the list admits permissive licences that common Rust crates carry: Unicode-3.0 / Unicode-DFS-2016 (unicode tables), Zlib, CC0-1.0, 0BSD, BSL-1.0 and CDLA-Permissive-2.0 (the Mozilla root list in `webpki-roots`). Needs owner confirmation; none of them is copyleft.
- **MQTT client: own session layer on the rumqttc codec.** `crates/client` uses rumqttc's public packet codec (`mqttbytes` v4 and v5, Apache 2.0) but not its event loop. rumqttc's event loop gives no access to the socket, so the SNI override (T-10), precise DNS/refused/timeout messages (E-01..E-03), the "plain on a TLS port" probe (E-05), the WebSocket HTTP status (E-15), the presented certificate chain for the trust prompt (T-04..T-09) and auto-negotiation of 3.1.1 vs 5 against brokers that answer a v5 CONNECT with a v3 CONNACK could not be done reliably. Transport (TCP, TLS via tokio-rustls, WebSocket via tokio-tungstenite) and the session (CONNECT, keep-alive, QoS 1 and 2 flows, subscriptions, reconnect) are ours.
- **One error catalog for app and CLI.** User-facing errors are `{key, params, text}`. Catalogs live in `packages/ui/src/i18n/<lang>/errors.json` (namespace `errors`); the Rust side embeds the same files to render CLI messages, so the app and the CLI print the same sentence. Placeholders use `{name}` (i18next configured with single braces). A parameter whose value starts with `@` is itself a catalog key (for example the MQTT 5 reason in "Disconnected by the broker: {reason}."), translated by the renderer.
- **FR and DE catalogs** are drafts written by the implementing model for the error catalog and the strings visible in mockups 19 and 20; the full catalogs and the native review stay in phase 5.
- **Embedded broker: own in-process broker (fallback beyond SPEC section 10).** Evaluated 13 Oct 2026, about 10 minutes, against the requirements of SPEC 7 (per-listener TLS with keys from the keychain, client certificates with revocation at the next handshake and the CN per connection, per-listener login plus shared users, anonymous gating, clients list with disconnect, counters, live log):
  - rmqtt 0.24: its network layer forces rustls with `aws-lc-rs` on non-Windows targets (licence outside the allow-list); TLS only from file paths (private keys would sit on disk); fixed client verifier without a revocation hook or optional client authentication; authentication through a plugin system with a global context; heavy dependency tree.
  - rumqttd 0.20: rustls 0.22, certificates from file paths, no revocation, no per-listener login, only a static authentication closure, no in-process clients list or disconnect; last release December 2023.
  - Mosquitto sidecar: a binary per platform, EPL/EDL, no in-process control (clients list, CN per connection, live revocation).
  - Chosen: a compact broker in `crates/broker` on tokio, tokio-rustls (ring) and tokio-tungstenite, reusing rumqttc's packet codec (v4 and v5) without its TLS features. MQTT 3.1.1 and 5, QoS 0 to 2, retained, last will with delay, persistent sessions (in memory), session takeover, shared subscriptions, bounded per-client queues.
- **rumqttc codec workarounds.** rumqttc 0.25 writes a malformed v5 DISCONNECT (remaining length 1 with two body bytes) and rejects valid short ones. Client and broker check frame completeness themselves and read and write DISCONNECT by hand.
- **Broker behaviour choices** (open points in the spec): users, anonymous and listener logins apply live; listeners, certificates, CA rotation and "verify against" need a restart (`restartNeeded`). A listener's login is accepted in addition to the shared users (the open assumption, implemented as stated). On a mutual-TLS listener a verified certificate is enough and the login is optional. Anonymous on means no client certificate is requested. The "Plain, lab only" preset creates an unencrypted listener without turning global anonymous on. A 3.1.1 bad login gets CONNACK 0x05 as in TESTPLAN B-05. After a CA rotation, certificates issued by the old CA are marked `superseded` (new additive field of `IssuedCert`) and "mqttpanel-desktop" is re-issued on the next "Connect Explorer here".
- **Broker data folder.** `<app data>/broker/` (`broker.json`, public certificates); the CA key, the server key, listener passwords and issued client keys live in the keychain. `mqttpanel broker` reads the same folder.
- **Engine choices** (open points): nodes start collapsed; search and filters show matching topics plus their ancestors without changing the stored expansion; `#` in a search does not match `$` topics (MQTT rule); the default sparkline series of a topic is the whole payload when it casts, else `$.value`, else the first numeric or boolean field; automatic series for up to 64 fields per topic; CBOR and MessagePack are detected by content type only (forcing is always possible); Sparkplug B metrics are exposed by name (`$.Temperature`), aliases resolved per edge node from NBIRTH/DBIRTH; the memory budget is reported, not enforced yet.
- **Sparkplug B schema** written as hand-made prost structs from the public payload structure; no `.proto` file from another project is copied (licence).
- **Diagnostics.** `MQTTPANEL_LOG=1` prints page loads and IPC command names (no payloads) to stderr; `MQTTPANEL_HOME` and `MQTTPANEL_SECRETS=memory` exist for tests.
- **Contract check.** A Rust test compares every `EngineClient` method in `client.ts` with the registered Tauri command (name and parameter names), so a renamed argument fails CI instead of failing at runtime.
- **Import formats.** The predecessor tool's settings file (connections under `ConnectionManager_connections`) and MQTTX exports are parsed from their observed public structure; the committed fixtures under `tests/fixtures/import/` are synthetic. Passwords from other tools are never imported; the summary says so. Needs a check against real export files from both tools.

### 13 Oct 2026, phases 2 to 4 UI
- **V-02 overflow check, stricter than the plan.** Besides each label, button, chip, segment, helper text and table header (one line, not clipped), the check fails when any pane, card, form, dialog or scroll box scrolls sideways, except code and payload views. It runs on all 27 screen states in EN, FR and DE. The first run found real defects (series editor wider than its pane, listener and clients tables wider than their columns, compare row and history tables in the Diff tab, publish composer in French), all fixed.
- **Fitting French and German.** Where a German or French string did not fit, the layout gives way first (wrapping paragraphs for long explanations, wrap of the publish Options row as in mockup 08, wider fixed table columns, rail padding 6 px); only then the string is shortened (DE "Gesendet · {n} in Sitzung", DE "Entfernen", FR "RSA : appareils anciens."). Native review in phase 5 may reword.
- **Listener table.** The "unencrypted" chip moved from the Reachable column to the Kind cell so the table fits the pane in French and German; the row menu (⋯) is also opened by right-click on the row, like a native list.
- **Broker status tile.** The uptime moved to the label line ("Status · 2 h 14 min") because it was clipped next to "Running".
- **Explorer.** Detail pane 520 px by default as in mockup 06 (mockup 04 draws 440 px); it stays resizable. The table's Value cell reads a small JSON object with "value" (and optional "unit") as "22.4 °C" as in mockup 05; the raw payload is in the tooltip. Pinned chips show above the tree only; the table groups pinned rows at the top instead (FEATURES, mockup 05).
- **Field picker.** Plottable fields are listed first, text fields greyed at the bottom, as in mockup 10b.
- **Series editor.** The unit helper text became an ⓘ bubble so the row stays on one line in every language; "or pick from the last message" is a wrapping line above the sample, as in mockup 10.
- **Native menu bar in the app language.** Labels are `nativeMenu.*` keys in the UI catalogs, embedded by the shell; the UI calls `set_menu_language` at start and on each language change and the shell rebuilds the menu. French and German follow each platform's own menu wording. The usual German label for Redo contains a word blocked by the commit guard, so the menu says "Erneut ausführen".
- **Increased contrast.** `prefers-contrast: more` darkens secondary text and lines; `forced-colors` uses system colours for borders and focus.
- **Mock engine in e2e.** In the browser build only (never in the desktop shell) the mock engine and platform are reachable as `window.__mqttpanelMock`, so Playwright flows can inject messages and read the clipboard, saved files and the menu language. The mock records every file it "saves".
- **Catalog guard in tests.** The catalog test reads the blocked-word patterns from `scripts/check-forbidden-words.sh` and applies them to every EN, FR and DE string, so the list lives in one place. Further catalog tests: French no-break spaces before `: ; ? !` and inside « », and no formal "Sie" in German.

### 13 Oct 2026, change set 1
- **Flow as five adapter steps.** "Connect to this app's broker" calls `app_broker_step(listenerId, step, profileName)` five times in order: `user` (create or keep "mqtt-panel"; `skipped` with Anonymous on), `cert` (issue or keep "mqttpanel-desktop"; `skipped` unless the listener requires a client certificate and Anonymous is off), `trust` (create or update the single profile "Local broker": host localhost, port and scheme of the listener, verify server on, the broker's CA for TLS and WSS, certificate and key in the keychain, user and password reference, MQTT 5), `broker` (start when stopped, restart when a change is pending, else nothing; no prompt), `connect` (closes an open connection of the profile, then connects). The dialog ticks off real progress without an event channel; every step is idempotent, an error is a plain-words message that stops the list. Covered by adapter tests with in-memory secrets (B-12, C-13).
- **Steps shown.** Planned before the run from the listener and the broker status: the certificate line only for mutual TLS with Anonymous off, the broker line only when the broker is stopped or a restart is pending; on a plain listener the trust line reads "Profile updated". After "Add a listener…" (menu or dialog) the steps start at once for the new listener ("then the dialog's steps run", C-13).
- **Profile "Local broker".** Named in the app language when it is created (EN "Local broker", FR « Broker local », DE „Lokaler Broker“) and never renamed afterwards; client ID "mqttpanel-desktop"; MQTT 5 (was auto). Its password can be revealed and copied because the app generated it; `profile_password` answers for that profile only, never for a password the user typed.
- **User passwords.** broker.json keeps the argon2 hash; the plain password is kept in the keychain (`broker/user/<name>/password`) so the users table can reveal and copy it. Users created before change set 1 have no kept password and show "not kept" with a hint to set a new one (pre-release, no migration). The generated password of "mqtt-panel" is 24 letters and digits without look-alikes (about 143 bits). An existing "mqtt-panel" without a kept password gets a new one; a hand-made "mqtt-panel" is marked as the app's user.
- **Listener ranking.** Mutual TLS (TLS before WSS), TLS, secure WebSocket, TCP, WebSocket; the same order in Rust (`BrokerManager::listener_rank`), the mock and the dialog.
- **TOML.** The `login` key of `[[listener]]` is gone and refused like any unknown key ("file.unreadable"); no compatibility shim (pre-release).
- **Restart prompt.** Opened only by actions that need a restart: listener added, removed, switched on or off, port, reachability; regenerate, import or rotate certificates; "Verify against"; importing a client CA while it is in use. It relies on the broker's `restartNeeded`, so it only asks while the broker runs; a stopped broker applies everything at the next start. The prompt lists what is pending ("Pending: TLS listener port 8883 → 8884."); a second port change of the same listener keeps the original port. The list lives in the UI session and is cleared when the broker no longer reports a pending restart. "Motion off" in Settings stops the pulse as reduced motion does (static highlight).
- **Pill.** On the Broker screen "restart pending" replaces the client count, as in mockup 12c; on other screens it is appended while the active connection is the profile "Local broker".
- **Enter in the port field.** It no longer causes an implicit click: the prompt opened and took the focus before the key was released, so "Later" received the Enter.
- **New connection.** The button reads "New connection ▾" as in mockup 02 (it read "+ New"). The menu bar item File > New connection still opens the empty form ("Another broker…"): a menu bar item cannot open a menu at the button.
- **Context submenus.** `ContextMenuItem.items` makes a submenu: a native `Submenu` in the desktop shell, a DOM submenu in the mock (hover, click, arrow right and left, Enter, Escape).
- **Eye and copy.** One component pair (`SecretText` for values the app keeps, `SecretInput` for fields): users table, the "mqtt-panel" password in the dialog, the "Local broker" password, the issue and export PKCS#12 passphrases, the add-user and change-password dialogs. Tooltips "Show password", "Hide password", "Copy" in every case, as the brief asks, also for passphrases. Copy shows a short "Copied to the clipboard." toast.
- **Add listener closing note.** Depends on where the flow was started: Broker screen with the broker running: "After adding, the app asks to restart the broker."; broker stopped: "The listener starts with the broker."; Connections flow: "After adding, MQTT Panel restarts the broker if needed and connects." The preset sentence follows.
- **Strings.** The FR and DE texts suggested in the brief are used as given (FR « Redémarrer » for "Restart now"); the 02b explainer in FR and DE follows the EN text. Nothing had to be shortened: the V-02 overflow check is green for every new state in EN, FR and DE. Native review stays in phase 5.
- **Screenshot states.** Five new states (02 menu, 02b, 02b with running steps, 12c, 12d pending) and 12 with a revealed password. The mock accepts `latency=<ms>` in the URL so the running steps can be captured.

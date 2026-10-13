# Progress

Implementation status of MQTT Panel against `docs/SPEC.md` section 12. Updated at the end of each phase.
Last update: 13 Oct 2026 (phases 1 to 4, change sets 1 to 4 complete on macOS; Windows and Linux through CI).

## Summary

| Phase | Scope | Status |
|---|---|---|
| 1 Spike | engine, client, broker with TLS and mTLS, benchmark, Tauri shell, native-feel walk | Done on macOS. Windows and Linux builds and walks pending (CI). |
| 2 Core desktop | profiles and keychain, Connections with TLS form and trust prompt, Explorer tree and table, inspection, decoders | Done on macOS (UI with the mock engine and the desktop shell). See "UI" below. |
| 3 Publish and history | composer, templates, sent log, delete retained, repeat, request and response, charts, export | Done on macOS. See "UI" below. |
| 4 Broker and CLI | Broker screen, Add listener, client certificates, authentication, clients, log, CLI | Done on macOS. See "UI" below. |
| Change set 1 | "New connection" menu, "Connect to this app's broker", restart prompt, shared users only, password reveal, "Test" | Done on macOS (13 Oct 2026). See below. |
| Change set 4 | owner feedback on the third build: broker starts on Connect, stop button on cards, no parsed chips, no "mTLS", Security and Advanced folds, rail badges instead of the title bar pill, Broker screen reorganised | Done on macOS (13 Oct 2026). See below. |
| Change set 3 | keychain prompts: no migration, the master key is the only keychain item, a refusal is asked once per session (`keychain.refused`), the broker issues a missing server key again | Done on macOS (13 Oct 2026). V-09 is the owner's manual check. See below. |
| Change set 2 | connected cards, card menu and visible Delete, connection bar, "Local broker" follows the broker, broker-wide client certificate, live log collapsed, Send top right, bigger buttons, macOS title alignment, one keychain master key | Done on macOS (13 Oct 2026). V-08 and V-09 are the owner's manual checks. See below. |

## Change set 4 (13 Oct 2026, owner feedback on the third build): implemented

Specified in `docs/FEATURES.md` (Connections, Broker), `docs/SPEC.md` 1.2, 4 and 6, mockups 02, 02b, 02c, 02d, 12, 12b, 12c, 13, 14, 17 and the title bar of every screen, `docs/TESTPLAN.md` C-17 to C-20, B-21 to B-24, V-10. Readings awaiting the owner are under "Open" in `docs/DECISIONS.md`.

| Item | Status |
|---|---|
| Connect and Test on "Local broker" start a stopped broker first ("starting broker…", then "connecting"); a broker that cannot start fails the connection with its reason | Done 13 Oct 2026, from every entry point (form, card menu, palette, "Choose a connection") |
| Round stop button (■, "Disconnect") on connected, connecting and reconnecting cards; disconnects that connection only | Done 13 Oct 2026 |
| No parsed chips next to CA, client certificate, client key, password or PKCS#12; the plain error for a wrong file stays | Done 13 Oct 2026. `file.keychain` and `file.until` removed |
| No "mTLS" or "Mutual TLS": cards TLS, WSS, no TLS; preset "Encrypted + client certificates" (FR, DE as specified); 02b chip "client certificate" | Done 13 Oct 2026. Also the client certificate ⓘ and the "Add a listener…" ⓘ (see Open) |
| Form: Basics open; "Security" and "Advanced" folds closed at start, state kept while the app runs, summaries; no helper sentences; every ⓘ kept | Done 13 Oct 2026 |
| Title bar pill removed; play badge on Broker (clay while starting), count bubble on Connect; "unverified" chip on tab and card | Done 13 Oct 2026. Connection bar and ⌘⇧L switch menu kept |
| Broker screen: tiles Status (uptime), In, Out, Retained; Listeners line with "restart pending", "This computer only / Whole network" + ⓘ and "+ Add listener…"; table On, Kind, Port, Setup, ✕ (removes at once, restart prompt follows) | Done 13 Oct 2026 |
| Users panel (Anonymous + ⓘ, "+ Add user"; ✎ and ✕; "Local broker" tag) and Client certificates panel (ⓘ, "Import CSR…", "Issue…"; PEM, PKCS#12, ✕ revoke with confirmation; "Local broker" tag) | Done 13 Oct 2026 |
| "Advanced security" fold: client-certificate requirement, Verify against (with Import CA…), Local CA (Export CA.pem, Rotate…), Server certificate (Import…, Regenerate…) | Done 13 Oct 2026 |
| Right pane: "Clients" (no count, no "live"); "▸ Log" with "· n warnings" only when n > 0 | Done 13 Oct 2026 |
| Reachability broker-wide: `BrokerConfig.bind` in the model, `broker.json` (old per-listener values migrate, network wins), the TOML file (top-level `bind`, the listener key refused), the runtime, the adapter (`set_broker_bind`), Add listener step 2 (kind and port only) | Done 13 Oct 2026. CLI `--bind` override unchanged |
| Every explanation on the Broker screen in an ⓘ; the permanent sentences removed | Done 13 Oct 2026 |
| Tests: C-17 to C-20, B-21 to B-24, V-10 (Playwright and Vitest), Rust B-21 (manager, old `broker.json`, TOML, runtime binding, CLI, adapter follow); screenshots re-recorded | Done 13 Oct 2026. Rust 200, Vitest 65, Playwright 221, all green |

## Change set 1 (13 Oct 2026, owner feedback after the first run): implemented

Specified in `docs/FEATURES.md` (Connections, Broker), `docs/SPEC.md` section 7, mockups 02, 02b, 12, 12b, 12c, 14, 19 and `docs/TESTPLAN.md` C-12, C-13, B-06, B-12, B-15, B-16, B-17. Status per item:

| Item | Status |
|---|---|
| "New connection" menu (Another broker… / This app's broker > Use an existing listener / Add a listener…); Target row removed from the form; first-launch cards routed | Done 13 Oct 2026. Native submenu in the desktop shell (Tauri `Submenu`), DOM submenu with keyboard (arrows, Enter, Escape) in the mock |
| "Connect to this app's broker" dialog (02b): listener list, explainer, steps, user "mqtt-panel", certificate "mqttpanel-desktop" when required, CA trust, start or restart, connect, single profile "Local broker" | Done 13 Oct 2026. Adapter command `app_broker_step` (user, cert, trust, broker, connect); steps tick off in the dialog, an error stops the list with the reason |
| Restart prompt (12c): "Restart now" / "Later"; pulsing "Restart broker and apply changes"; pill and heading "restart pending"; reduced motion | Done 13 Oct 2026. Pending changes listed in the prompt ("Pending: TLS listener port 8883 → 8884."), port cell marked "pending" |
| Broker screen: "Connect Explorer here" and the row menu entry removed; Authentication above Client certificates; users table with eye and copy; per-listener login removed from UI, model, TOML config and broker runtime | Done 13 Oct 2026. TOML `login` key now refused like any unknown key; plain user passwords in the keychain beside the argon2 hash |
| Eye and copy on every password and passphrase field the app owns | Done 13 Oct 2026: users table, "mqtt-panel" password in the dialog, generated and typed PKCS#12 passphrases (issue and export), add-user and change-password dialogs, the "Local broker" profile password |
| "Test" / « Tester » / „Testen“ | Done 13 Oct 2026 |
| Tests updated: Vitest, Playwright states and overflow check (EN, FR, DE), Rust broker tests (B-06, B-12), new flows (C-12, C-13, B-15, B-16) | Done 13 Oct 2026. Rust 183, Vitest 44, Playwright 189, all green |
| Test lab scripts `tools/testlab/` (mosquitto in five modes, Paho load client) | Done, verified on macOS 13 Oct 2026 (B-17 manual run by the owner pending) |

## Change set 2 (13 Oct 2026, owner feedback on the second build): implemented

Specified in `docs/FEATURES.md` (Connections, Publish, Broker), `docs/SPEC.md` 1.2 and 6, mockups 02, 02c, 04 to 11, 12, 12c, 13, 14, 18, 19, `docs/TESTPLAN.md` C-14 to C-16, P-11, B-18, B-19, V-08, V-09.

| Item | Status |
|---|---|
| Connected card in the accent colour with "connected"; chip on the address line; reconnecting outline | Done 13 Oct 2026. A connecting card is outlined in clay too ("connecting"); dark theme uses dark text on the light accent for contrast |
| Card context menu (Connect/Disconnect, Test, Duplicate, Delete…); visible Delete in the header; Duplicate header button removed | Done 13 Oct 2026. Menu also keeps Move up / Move down; opens with right-click, the context-menu key or Shift+F10; Delete asks "Delete the connection "X"?"; a connected profile is disconnected first |
| Connection bar under the title bar; title bar pill limited to broker state and lab flags | Done 13 Oct 2026. ✕ on the active tab, on the others when hovered or focused; ⌘⇧L and the palette "Switch connection" open the Explorer on the chosen connection; the pill opens the Broker screen (or Connections when it reads "Not connected") |
| "Local broker" profile follows every broker change at once | Done 13 Oct 2026. Listener kind, port, reachability, enable state, client-certificate requirement, CA rotation and regeneration, "mqtt-panel" password, Anonymous; reconnects after every broker start or restart when it was open |
| Broker-wide client certificate control under Client certificates, with "Verify against"; read-only per listener; preset and step 2 texts | Done 13 Oct 2026. Per-listener `requireClientCert` removed from the model, `broker.json`, the TOML file (`[tls] require_client_cert`, the old listener key is refused) and the runtime |
| Live clients and log, no Refresh; log collapsed to a title strip, expands on click | Done 13 Oct 2026. State kept in the settings (`brokerLogOpen`) |
| Send top right in the Publish composer header | Done 13 Oct 2026 |
| Action buttons slightly bigger | Done 13 Oct 2026: 13 px text, 6 × 13 px padding; panel headers 46 px so all columns line up |
| macOS title bar: app name aligned with the traffic lights | Done 13 Oct 2026: `trafficLightPosition` x 16, y 24 in `tauri.conf.json` (corrected 13 Oct 2026, measured centre 22 px), title row inset 84 px on macOS only. Visual check V-08 by the owner |
| Secrets: one keychain master key, encrypted secrets file, lazy read, no keychain access on restart or start-up; no migration, refusal asked once per session, broker regenerates a missing server key (change set 3) | Done 13 Oct 2026. `secrets.enc` (XChaCha20-Poly1305, record name as associated data, nonce per record, version 1); server key kept in memory by the broker manager. Change set 3: `migrate_from`, the `migrated` field, the per-secret keychain store and the legacy key lists are gone (an old file with the field still loads); `FileSecrets` remembers a refused master key for the session and answers `keychain.refused` at once (EN, FR, DE); a broker start (app and `mqttpanel broker`) issues a missing server key again or regenerates a CA whose key is missing, logged, and fails with the reason when the store refuses; a TLS or WSS listener never starts without its material. Manual check V-09 by the owner |
| Tests: C-14 to C-16, P-11, B-18, B-19, Vitest, Playwright states and overflow check, Rust | Done 13 Oct 2026. Rust 190, Vitest 54, Playwright 207, all green |
| Tests change set 3: V-09 at crate level (refusal asked once, old `migrated` field loads, opening the store reads nothing), B-20 (server key missing, CA key missing too, refusing store, no TLS listener reads no key) | Done 13 Oct 2026. Rust 196, Vitest 54, Playwright 207, all green |

## What works

### Rust crates (`cargo test --workspace`)

| Crate | What | Tests (passed / failed / opt-in ignored) |
|---|---|---|
| `crates/engine` | Pure state machine: topic store (radix tree + flat index), ring buffers, series with casting rules, decoders (JSON, text, hex, base64, image, Sparkplug B with aliases, CBOR, MessagePack, Protobuf with user `.proto` and imports), diff, search (substring, wildcard, regex), filters, export (CSV, JSON, NDJSON, chart CSV), memory estimate, C ABI with header | 57 / 0 / 1 |
| `crates/tls` | Certificate, key and PKCS#12 parsing, fingerprints, generation (local CA, server and client certificates, CSR signing, PKCS#12 and encrypted PEM export), the F-01 test kit | 12 / 0 / 0 |
| `crates/client` | MQTT 3.1.1 and 5 over TCP, TLS, WebSocket and secure WebSocket; auto version negotiation; QoS 0 to 2; keep-alive; reconnect with subscription restore; MQTT 5 properties; plain-word errors (TESTPLAN section 7) and trust prompts (self-signed, private CA, expired, not yet valid, fingerprint changed) | 45 / 0 / 0 |
| `crates/profiles` | Profiles, settings, templates, the secrets file `secrets.enc` with one keychain master key (lazy, cached, refusal remembered for the session), export without secrets or with passphrase (Argon2id + XChaCha20-Poly1305), import from MQTT Panel, MQTTX and the predecessor tool | 15 / 0 / 0 |
| `crates/broker` | In-process broker: TCP, TLS, WS, WSS listeners (one broker-wide reachability: whole network or this computer only), presets, shared users for every listener (argon2 at rest, plain password in the keychain for reveal), the app user "mqtt-panel" and certificate "mqttpanel-desktop", anonymous gating, client certificates with revocation and one broker-wide requirement, CA rotation, CSR import, clients, counters, log, TOML config for the CLI; the server key is kept in memory across restarts; a missing server or CA key is made again on start | 46 / 0 / 1 |
| `crates/cli` | `mqttpanel sub`, `pub`, `broker`, `profiles`; SPEC 8 help texts byte for byte; exit codes 0/1/2/3 | 16 / 0 / 1 |
| `apps/desktop` | Tauri 2 shell: native menu bar (macOS conventions, labels in the app language), 94 commands, 20 Hz event ticker, native dialogs, "Connect to this app's broker" steps, the "Local broker" follow-up and reconnect | 9 / 0 / 1 |

Total: 200 passed, 0 failed, 4 opt-in ignored (13 Oct 2026, after change set 4). `cargo deny check`: advisories, bans, licences and sources ok. `npm audit`: 0 vulnerabilities.

Opt-in tests (`-- --ignored`, nightly): public brokers (`MQTTPANEL_PUBLIC_BROKERS=1`), performance (B-14, Q-02), corpus regeneration.

## Measured numbers (MacBook, macOS 27.0.1, release builds)

| Measure | Result | Target |
|---|---|---|
| Engine ingest (nested JSON, 2 000 topics) | 1.7 million msg/s | 5 000 msg/s |
| Build 100 000 topics | 60 ms | |
| Tree slice, 60 rows of 100 927 visible | 3.3 ms first, 0.009 ms cached | < 2 ms (X-01) |
| Table slice sorted by rate, 100 000 topics | 3.0 ms | |
| Search substring / wildcard / regex over 100 000 topics | 1.7 / 3.2 / 1.0 ms | < 50 ms |
| Memory estimate accuracy (Q-05) | ratio 1.000 (estimate within 8 bytes of 38.9 MB measured) | within 10 % |
| Full pipeline: Mosquitto, client, adapter, engine at 5 000 msg/s for 10 s | 50 000 / 50 000 delivered, 0 ms backlog after the last send | no backlog > 1 s (Q-02) |
| Embedded broker, 1 000 publishers + 4 subscribers, QoS 1 (B-14) | 4 600 msg/s in, 18 400 msg/s fan-out, 0 dropped | 5 000 in, 20 000 out, no drops |
| macOS app bundle size | 14 MB | about 10 MB |

The first tree slice after a structural change rebuilds the visible list (3.3 ms for 100 927 rows); cached slices are far below the 2 ms target. The UI frame-time measurement (Q-01) runs in the nightly job.

## How to run and test

See `docs/RUNNING.md` and `HANDOFF.md` section "Commands". The macOS app is built with `cd apps/desktop/src-tauri && cargo tauri build` (bundle under `target/release/bundle/macos/MQTT Panel.app`).

## UI (React in Tauri 2, `packages/ui`)

Every screen of `docs/mockups/mqtt-panel-mockups.html` was compared side by side with the app at 1280 × 700 (Playwright screenshots of the 36 states in `packages/ui/e2e/states.ts`, light and dark; after change set 4 the title bar holds the name and the mark only, the rail carries the broker badge and the count bubble). Deviations are listed in `docs/DECISIONS.md` ("13 Oct 2026, phases 2 to 4 UI").

| Mockup | Screen | Works | Not yet / differs |
|---|---|---|---|
| 01 | First launch | Empty profile list, welcome cards ("Create…" opens the New connection menu, "Start and connect" opens 02b, import), ⌘K hint | |
| 02, 02c, 02d, 19 | Connections (EN, FR) | Profile cards with search and drag reorder, state word and round stop button, chips TLS, WSS, no TLS, "unverified" when verification is skipped; "New connection ▾" menu with the "This app's broker" submenu; form with Basics open and the "Security" and "Advanced" folds (closed at start, summaries, state kept while the app runs); file pickers with Browse and drop, no parsed chips, plain error for a wrong file; every security field has its ⓘ bubble (EN, FR, DE); SNI and ALPN rows; subscription chips with X and the "no subscriptions" notice; "Test" with latency, certificate and version; Connect and Test on "Local broker" start the broker first; the "Local broker" password with eye and copy; import from other tools with a summary | |
| 02b | Connect to this app's broker | Enabled listeners, most secure first and preselected, chips (client certificate, Encrypted, unencrypted), "Add a listener…" with ⓘ; explainer (Anonymous variant); steps ticking off with the "mqtt-panel" password behind eye and copy; error stops the list | The explainer is one generic text ("if the listener requires one", "if needed") instead of the mockup's listener-specific wording |
| 03 | Trust prompt | Reason, subject, issuer, SAN, validity, SHA-256 fingerprint, key; one action for self-signed with its scope line; "Trust anything this CA signs" only for a private CA; expired marked unsafe; Connect offers the same prompt | |
| 04 | Explorer tree | Virtualised tree, child count, messages since connect, rate, inline value, retained marker, activity dot, sparkline with hover values, highlight on the row and collapsed ancestors (reduced motion: dot only), expand menu, pinned chips, views, follow and pause with missed counter, quick publish row, native context menu (pin, publish here, add to chart, copy topic / payload / CLI, export subtree, delete retained, delete retained in subtree with preview, clear history) | The root row shows the count without the words "since connect" (the tooltip says it) |
| 05 | Explorer table | Sortable, hideable, reorderable columns; "Value · last 10" sparkline; pinned rows grouped at the top; filters (retained, changed in last n min, payload contains, QoS, has value); footer counts | |
| 06 | Diff | Current against previous or any history entry, change count, highlight toggle, history with changed keys and Baseline; right-click or hover icon on a field: add to chart, copy path, copy value | |
| 07 | Properties | Negotiated version first, standard fields, MQTT 5 block only when present, reply box for request and response | Message expiry does not show "n s left when received" |
| 08 | Publish | Templates (saved on request only, folders, placeholders), composer with topic autocomplete, QoS, retain, format, CodeMirror payload with JSON validation, MQTT 5 properties, request and response with ⓘ bubble (hidden with the one-line note on 3.1.1), sent log with status, re-send, load, save as template, copy as CLI, export | The "How this works" text is in the ⓘ bubble, not an inline callout (as FEATURES asks) |
| 09 | Delete retained, repeat | Confirmation with the current retained value; repeat every n s with count, running indicator and Stop, stops on disconnect | |
| 10, 10b | Charts, field picker | Charts with several series, left and right axes, units, line / step / points, time range, pause, hover readout, CSV and PNG export, rename, duplicate, remove, other charts strip; series editor with click-to-pick in the last message; field picker with plottable fields first, text greyed, other path, target chart | |
| 11 | Export | Selected topic, subtree or visible rows; time range; CSV, JSON, NDJSON with description; MQTT 5 properties and base64 options; size estimate; native save dialog | |
| 12, 12b | Broker | Start, stop, restart; tiles Status (uptime), ↓ In, ↑ Out, Retained; Listeners line with "restart pending", the broker-wide "This computer only / Whole network" choice with ⓘ and "+ Add listener…"; listener table On, Kind ("unencrypted" only on the whole network), Port with validation, Setup, ✕; Users panel (Anonymous with confirmation and gating, ⓘ, "+ Add user", eye, copy, ✎, ✕, "Local broker" tag); Client certificates panel (ⓘ, Import CSR…, Issue…, PEM, PKCS#12, ✕ revoke, last seen, "Local broker" tag); "Advanced security" fold (requirement, Verify against, Local CA, Server certificate); Clients with disconnect; "▸ Log" strip with warnings, live log with level; one-sentence Anonymous strip | |
| 12c | Restart prompt | "Restart the broker to apply the change?" with the pending list, "Later" / "Restart now"; pulsing clay "Restart broker and apply changes" (static highlight under reduced motion or Motion off); "restart pending" on the Listeners line; "pending" on a changed port; Advanced security open, log expanded | |
| 13, 14 | Add listener | Step 1 setup with ⓘ per preset and explanation; step 2 kind and port only (refuses below 1024 with the exact message); closing note depends on context (asks to restart / starts with the broker / the Connections flow restarts and connects) | |
| 15 | Issue client certificate | Name, validity, ECDSA or RSA, PEM or PKCS#12, passphrase with Generate, eye, copy and ⓘ bubble, save CA.pem too | |
| 16, 17, 20 | Settings (EN, DE) | Appearance, language (runtime switch), data and memory with live estimate and Clear all history, security (skip verification, trusted fingerprints), updates, shortcuts (searchable), privacy, about and licences | |
| 18 | Command palette | Commands, topics and connections, fuzzy match, keyboard only | |

Empty states: "Choose a connection" on Explorer, Publish and Charts without an active connection; Charts without a chart.

Running the real app: `cargo tauri build` produces `MQTT Panel.app` (16.0 MB after change set 2); it starts, loads the UI and answers every IPC call used at start (checked with `MQTTPANEL_LOG=1`). Screen capture and accessibility automation are not available to the build agent, so the visual walk of the real window is a manual step (V-03).

## Test results (13 Oct 2026, macOS 27, after change set 4)

| Suite | Passed | Failed | Skipped |
|---|---|---|---|
| Rust workspace (`cargo test --workspace`, 7 crates, 29 test binaries) | 200 | 0 | 4 opt-in (`--ignored`) |
| UI unit and component tests (Vitest, 13 files) | 65 | 0 | 0 |
| Playwright, V-02 overflow check, 36 states × EN, FR, DE | 108 | 0 | 0 |
| Playwright, V-01 screenshots, 36 states × light, dark (EN) | 72 | 0 | 0 |
| Playwright, end-to-end flows with the mock engine | 41 | 0 | 0 |

Change set 4 adds the flows C-17 (broker starts on Connect and Test, a failing start fails the connection), C-18 (stop button), C-19 (folds), C-20 (no parsed chips, no "mTLS" in EN, FR, DE), B-21 (broker-wide reachability), B-22 (listener table), B-23 (panels, Advanced security, Add listener step 2), B-24 (verbosity) and V-10 (title bar, rail badges, "unverified"), the same items at component level in Vitest, and Rust B-21 (manager value and restart flag, runtime binding checked against a LAN address, old `broker.json` migration, TOML top-level `bind` and the refused listener key, CLI `--config` with `--bind`, the adapter's "Local broker" reconnecting after a reachability change). The Playwright flows cover C-05 with C-15 (connection bar), C-14, P-11, B-18 with B-19, C-07, C-12, C-13, X-05, X-06, X-08, X-09, X-10, X-11, X-14, D-18, D-19, D-20, D-21, P-03, P-04, S-01, S-03, V-04, V-05, B-02, B-12, B-15 (two tests, one under reduced motion), B-16 and three native-feel checks. Rust covers B-18 (broker-wide requirement in the manager, the runtime and the TOML file; plain listeners unaffected), C-16 at adapter level (the "Local broker" profile follows port, requirement, password, CA rotation, Anonymous and a switched-off listener, and reconnects after restarts), V-09 at crate level (secrets file: encryption, nonce per record, lazy key read once, a refusal asked once and answered with `keychain.refused`, an old file with the `migrated` field loads, no keychain read when the store opens; broker restarts never read the secret store), B-20 (a missing server key issues a new server certificate, a missing CA key regenerates the CA, a refusing store fails the start with no listener up), B-06 (shared users on TCP, TLS, WS and WSS; TOML `login` refused) and B-12 and C-13 at adapter level (`app_broker_step` three runs without duplicates, connected within 5 s from a stopped broker, restart only when pending, Anonymous on). Vitest covers C-14, C-15, C-16 (mock engine), P-11, B-18, B-19, C-08, C-12, D-16, D-17, D-22, P-06, P-07, T-04, T-06, T-26, X-04, X-13, B-03, B-07, B-15 and B-16 at component level, the catalogs (keys, placeholders, info bubbles, blocked words, French typography, German "du") and the formatters. V-01 produces screenshots; comparing them against stored baselines per OS is not wired yet (phase 5).

## Native-feel checklist (SPEC 1.2), macOS

| Item | Result | Evidence |
|---|---|---|
| Native menu bar, context menus, shortcuts with platform conventions | Pass | Tauri menu API: app menu with About, Settings (⌘,), Services, Hide, Quit; File, Edit, View (⌘1 to ⌘5, ⌘K), Connection, Window, Help; labels in the app language. Context menus are native popups (`Menu.popup`). Windows and Linux put Settings under Edit and Quit under File |
| Native window chrome, traffic lights, dialogs, notifications on request only | Pass | `titleBarStyle: Overlay`, native open and save dialogs (`tauri-plugin-dialog`), no notifications |
| System font, platform text rendering, tabular figures, no web fonts | Pass | `-apple-system` stack, `font-variant-numeric: tabular-nums`, no font files |
| No web-isms: no text cursor or selection on chrome, no drag images, no rubber band; data selectable | Pass | `user-select: none` and `cursor: default` on chrome, `-webkit-user-drag: none`, `overscroll-behavior: none`; tree names, payloads, tables and inputs selectable (flow test) |
| Scrollbars per platform | Pass | macOS keeps WebKit's overlay scrollbars; classic styled scrollbars on Windows and Linux only |
| Theme follows the OS live; reduced motion; high contrast | Pass | `prefers-color-scheme`, `prefers-reduced-motion` and `prefers-contrast: more` (flow tests); OS accent colour not used (optional in the spec) |
| Keyboard-first, focus rings, command palette | Pass | `:focus-visible` rings, ⌘K palette (V-04), tree and table keyboard navigation |
| Runtime prerequisites documented | n/a on macOS | WebView2 and WebKitGTK checks belong to the Windows and Linux installers (phase 5) |
| Visual regression on three OSes | Partial | Screenshots for every state in light and dark; baselines and the Windows and Linux runs are phase 5 (CI) |
| Walk of the real window by a person (V-03) | Pending | Manual step; see "Running the real app" |

## Open tasks for phase 5

- FR and DE: native review of every string (V-06, `docs/COPY_REVIEW.md`); the implementing model's drafts include shortened labels listed in `docs/DECISIONS.md`.
- Accessibility pass (V-07): axe-core in the Playwright suite, screen-reader labels on every icon button, AA contrast measured in both themes, VoiceOver and NVDA spot checks.
- Native-feel walk of the real window on macOS, Windows and Linux (V-03), including menus in FR and DE.
- V-01 visual regression: store baselines per OS and compare in CI; run the Playwright suites on Windows and Linux.
- `npm run e2e:app` (tauri-driver smoke on Windows and Linux) and the Q-01 frame-time measurement in the nightly job.
- Persistence flows not covered with the mock engine: X-07 (pinned topics and saved views survive a restart) and C-01 at UI level; covered in Rust only.
- Mockup details: "since connect" wording on the tree root row (04); "n s left when received" on Message expiry (07).
- Owner confirmations: licence allow-list additions, the change set 1 points under "Open" in `docs/DECISIONS.md`, the three small layout deviations listed under "Open" in `docs/DECISIONS.md`.
- Installers (DMG, MSI, AppImage, deb, rpm), auto-update, Homebrew cask, winget manifest; signing and notarization last.
- Website at mqtt-panel.com (EN, FR, DE, no trackers) with localised screenshots.
- `scripts/check-testplan-coverage.sh` (listed in TESTPLAN) is not written yet.
- The memory budget is reported but not enforced (engine).

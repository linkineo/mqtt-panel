# Progress

Implementation status of MQTT Panel against `docs/SPEC.md` section 12. Updated at the end of each phase.
Last update: 13 Oct 2026 (phases 1 to 4 complete on macOS; Windows and Linux through CI).

## Summary

| Phase | Scope | Status |
|---|---|---|
| 1 Spike | engine, client, broker with TLS and mTLS, benchmark, Tauri shell, native-feel walk | Done on macOS. Windows and Linux builds and walks pending (CI). |
| 2 Core desktop | profiles and keychain, Connections with TLS form and trust prompt, Explorer tree and table, inspection, decoders | Done on macOS (UI with the mock engine and the desktop shell). See "UI" below. |
| 3 Publish and history | composer, templates, sent log, delete retained, repeat, request and response, charts, export | Done on macOS. See "UI" below. |
| 4 Broker and CLI | Broker screen, Add listener, client certificates, authentication, clients, log, Connect Explorer here, CLI | Done on macOS. See "UI" below. |

## What works

### Rust crates (`cargo test --workspace`)

| Crate | What | Tests (passed / failed / opt-in ignored) |
|---|---|---|
| `crates/engine` | Pure state machine: topic store (radix tree + flat index), ring buffers, series with casting rules, decoders (JSON, text, hex, base64, image, Sparkplug B with aliases, CBOR, MessagePack, Protobuf with user `.proto` and imports), diff, search (substring, wildcard, regex), filters, export (CSV, JSON, NDJSON, chart CSV), memory estimate, C ABI with header | 57 / 0 / 1 |
| `crates/tls` | Certificate, key and PKCS#12 parsing, fingerprints, generation (local CA, server and client certificates, CSR signing, PKCS#12 and encrypted PEM export), the F-01 test kit | 12 / 0 / 0 |
| `crates/client` | MQTT 3.1.1 and 5 over TCP, TLS, WebSocket and secure WebSocket; auto version negotiation; QoS 0 to 2; keep-alive; reconnect with subscription restore; MQTT 5 properties; plain-word errors (TESTPLAN section 7) and trust prompts (self-signed, private CA, expired, not yet valid, fingerprint changed) | 45 / 0 / 0 |
| `crates/profiles` | Profiles, settings, templates, OS keychain (macOS Keychain verified), export without secrets or with passphrase (Argon2id + XChaCha20-Poly1305), import from MQTT Panel, MQTTX and the predecessor tool | 9 / 0 / 0 |
| `crates/broker` | In-process broker: TCP, TLS, WS, WSS listeners (Network / Localhost), presets, per-listener login, shared users (argon2), anonymous gating, client certificates with revocation, CA rotation, CSR import, clients, counters, log, TOML config for the CLI | 37 / 0 / 1 |
| `crates/cli` | `mqttpanel sub`, `pub`, `broker`, `profiles`; SPEC 8 help texts byte for byte; exit codes 0/1/2/3 | 15 / 0 / 1 |
| `apps/desktop` | Tauri 2 shell: native menu bar (macOS conventions, labels in the app language), 91 commands, 20 Hz event ticker, native dialogs, Connect Explorer here | 6 / 0 / 1 |

Total: 181 passed, 0 failed, 4 opt-in ignored (13 Oct 2026). `cargo deny check`: advisories, bans, licences and sources ok. `npm audit`: 0 vulnerabilities.

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

See `HANDOFF.md` section "Commands". The macOS app is built with `cd apps/desktop/src-tauri && cargo tauri build` (bundle under `target/release/bundle/macos/MQTT Panel.app`).

## UI (React in Tauri 2, `packages/ui`)

Every screen of `docs/mockups/mqtt-panel-mockups.html` was compared side by side with the app at 1280 × 700 (Playwright screenshots of the 27 states in `packages/ui/e2e/states.ts`, light and dark). Deviations are listed in `docs/DECISIONS.md` ("13 Oct 2026, phases 2 to 4 UI").

| Mockup | Screen | Works | Not yet / differs |
|---|---|---|---|
| 01 | First launch | Empty profile list, welcome cards (new connection, start local broker, import), ⌘K hint | |
| 02, 19 | Connections (EN, FR) | Profile cards with search and drag reorder; form in Basics, Security, Session, Subscriptions; Target "Another broker" / "This app's broker"; file pickers with Browse and drop, parsed summary chip; every security field has its ⓘ bubble (EN, FR, DE); SNI and ALPN rows; subscription chips with X and the "no subscriptions" notice; Try connect with latency, certificate and version; import from other tools with a summary | |
| 03 | Trust prompt | Reason, subject, issuer, SAN, validity, SHA-256 fingerprint, key; one action for self-signed with its scope line; "Trust anything this CA signs" only for a private CA; expired marked unsafe; Connect offers the same prompt | |
| 04 | Explorer tree | Virtualised tree, child count, messages since connect, rate, inline value, retained marker, activity dot, sparkline with hover values, highlight on the row and collapsed ancestors (reduced motion: dot only), expand menu, pinned chips, views, follow and pause with missed counter, quick publish row, native context menu (pin, publish here, add to chart, copy topic / payload / CLI, export subtree, delete retained, delete retained in subtree with preview, clear history) | The root row shows the count without the words "since connect" (the tooltip says it) |
| 05 | Explorer table | Sortable, hideable, reorderable columns; "Value · last 10" sparkline; pinned rows grouped at the top; filters (retained, changed in last n min, payload contains, QoS, has value); footer counts | |
| 06 | Diff | Current against previous or any history entry, change count, highlight toggle, history with changed keys and Baseline; right-click or hover icon on a field: add to chart, copy path, copy value | |
| 07 | Properties | Negotiated version first, standard fields, MQTT 5 block only when present, reply box for request and response | Message expiry does not show "n s left when received" |
| 08 | Publish | Templates (saved on request only, folders, placeholders), composer with topic autocomplete, QoS, retain, format, CodeMirror payload with JSON validation, MQTT 5 properties, request and response with ⓘ bubble (hidden with the one-line note on 3.1.1), sent log with status, re-send, load, save as template, copy as CLI, export | The "How this works" text is in the ⓘ bubble, not an inline callout (as FEATURES asks) |
| 09 | Delete retained, repeat | Confirmation with the current retained value; repeat every n s with count, running indicator and Stop, stops on disconnect | |
| 10, 10b | Charts, field picker | Charts with several series, left and right axes, units, line / step / points, time range, pause, hover readout, CSV and PNG export, rename, duplicate, remove, other charts strip; series editor with click-to-pick in the last message; field picker with plottable fields first, text greyed, other path, target chart | |
| 11 | Export | Selected topic, subtree or visible rows; time range; CSV, JSON, NDJSON with description; MQTT 5 properties and base64 options; size estimate; native save dialog | |
| 12, 12b | Broker | Start, stop, restart, status tiles with separate in and out rates, listeners with Network / Localhost, port edit with validation, per-listener login, client-cert column, row menu (also right-click) with "Connect Explorer to this listener", network warning, restart-to-apply chip, client certificates first (issue, import CSR, export PEM / PKCS#12, revoke, last seen), broker certificates collapsed with summary and Export CA.pem, authentication (Anonymous with confirmation and gating, users, verify against), clients with disconnect, live log with level | |
| 13, 14 | Add listener | Step 1 setup with ⓘ per preset and explanation; step 2 kind, port (refuses below 1024 with the exact message), reachability, optional login, client certificate state, server certificate | |
| 15 | Issue client certificate | Name, validity, ECDSA or RSA, PEM or PKCS#12, passphrase with Generate and ⓘ bubble, save CA.pem too | |
| 16, 17, 20 | Settings (EN, DE) | Appearance, language (runtime switch), data and memory with live estimate and Clear all history, security (skip verification, trusted fingerprints), updates, shortcuts (searchable), privacy, about and licences | |
| 18 | Command palette | Commands, topics and connections, fuzzy match, keyboard only | |

Empty states: "Choose a connection" on Explorer, Publish and Charts without an active connection; Charts without a chart.

Running the real app: `cargo tauri build` produces `MQTT Panel.app` (15.9 MB); it starts, loads the UI and answers every IPC call used at start (checked with `MQTTPANEL_LOG=1`). Screen capture and accessibility automation are not available to the build agent, so the visual walk of the real window is a manual step (V-03).

## Test results (13 Oct 2026, macOS 27)

| Suite | Passed | Failed | Skipped |
|---|---|---|---|
| Rust workspace (`cargo test --workspace`, 7 crates, 28 test binaries) | 181 | 0 | 4 opt-in (`--ignored`) |
| UI unit and component tests (Vitest, 10 files) | 34 | 0 | 0 |
| Playwright, V-02 overflow check, 27 states × EN, FR, DE | 81 | 0 | 0 |
| Playwright, V-01 screenshots, 27 states × light, dark (EN) | 54 | 0 | 0 |
| Playwright, end-to-end flows with the mock engine | 24 | 0 | 0 |

The Playwright flows cover C-05, C-07, X-05, X-06, X-08, X-09, X-10, X-11, X-14, D-18, D-19, D-20, D-21, P-03, P-04, S-01, S-03, V-04, V-05, B-02, B-12 and three native-feel checks. Vitest covers C-08, D-16, D-17, D-22, P-06, P-07, T-04, T-06, T-26, X-04, X-13, B-03 and B-07 at component level, the catalogs (keys, placeholders, info bubbles, blocked words, French typography, German "du") and the formatters. V-01 produces screenshots; comparing them against stored baselines per OS is not wired yet (phase 5).

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
- Owner confirmations: per-listener login semantics, licence allow-list additions, the three small layout deviations listed under "Open" in `docs/DECISIONS.md`.
- Installers (DMG, MSI, AppImage, deb, rpm), auto-update, Homebrew cask, winget manifest; signing and notarization last.
- Website at mqtt-panel.com (EN, FR, DE, no trackers) with localised screenshots.
- `scripts/check-testplan-coverage.sh` (listed in TESTPLAN) is not written yet.
- The memory budget is reported but not enforced (engine).

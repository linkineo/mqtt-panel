# Handoff

How to pick up MQTT Panel. Read `CLAUDE.md` first; it carries the standing rules.

## Status (18 Oct 2026)
- Website and branding (18 Oct 2026): the app and the repository name the website https://mqtt-panel.com only (no company name); the site is live from `website/` (see "Website" below and `docs/DECISIONS.md` "18 Oct 2026, website mqtt-panel.com and brand references"). Tests: Vitest 147, Playwright 287, `cargo test -p mqttpanel-desktop` 11 (3 ignored), all green.
- Change set 10 (master key store by build, 18 Oct 2026) is implemented except the macOS Data Protection keychain (signing step): unsigned and ad-hoc builds keep the master key in `master.key` (0600) in the data folder, builds signed with a real identity in the OS keychain, moved once between them; `MQTTPANEL_KEYSTORE=file|keychain` forces the store. Code in `crates/profiles/src/keystore.rs`. Tests: Rust 218, Vitest 147, Playwright 287, all green. Readings under "Open" in `docs/DECISIONS.md`.
- Change set 8 (owner feedback on the seventh build, 15 Oct 2026) is implemented. Model change: `Profile.topics: { <topic>: { sparklinePath?, chartFields? } }` keeps the "Use as sparkline" pick and the topic chart per topic (the engine gets the picks through `set_sparkline_paths` at connect and on every save); layout change: the connection bar is the first row of the main column (`.main-col`, right of the rail) and is always shown. Tests: Rust 205, Vitest 134, Playwright 266, all green. See `docs/PROGRESS.md` "Change set 8"; readings awaiting the owner are under "Open" in `docs/DECISIONS.md`.
- Change set 7 (live apply, 14 Oct 2026) is implemented: every broker change applies while the broker runs and drops only the sessions it invalidates; no Restart button, prompt or pending state; a listener that cannot start shows its error on its row; one ↓ menu (PEM / PKCS#12…) per client-certificate row. Runtime structure change: one task per listener (`start_listener`, `stop_listener`, `rebind` on `BrokerHandle`), TLS material in a swappable slot read at each handshake, the client-certificate rule read live, and a session registry (listener, login kind, certificate issuer and serial) behind `revalidate()`; the manager's listener changes are `async`. `restartNeeded`, `restart()` and the `broker_restart` command are removed; `BrokerStatus.listeners[].error` carries a listener's start error. Tests: Rust 203, Vitest 115, Playwright 248, all green. See `docs/PROGRESS.md` "Change set 7"; readings awaiting the owner are under "Open" in `docs/DECISIONS.md`.
- Change set 6 (owner feedback on the fifth build, 14 Oct 2026) is implemented: three-line cards with the protocol chip and the ⚠ icon, fixed field widths, "New broker…" and "Built-in broker", "Generate", « Connecter », auto name from the host, per-connection skip of certificate verification with the reworked certificate prompt. Model change: `TlsSettings.skipVerify` replaces `verifyServer` (an old `verifyServer: false` loads as `skipVerify: true`), `Settings.skipVerify` is gone, `connect_profile` and `test_profile` take an optional `skipOnce`, `trust_certificate` also takes `skipOnce` and `skipAlways`. Tests: Rust 201, Vitest 107, Playwright 241, all green. See `docs/PROGRESS.md` "Change set 6"; readings awaiting the owner are under "Open" in `docs/DECISIONS.md`.
- Change set 4 (owner feedback on the third build, 13 Oct 2026) is implemented: Connect and Test on "Local broker" start a stopped broker first; a round stop button on open cards; no parsed chips next to file fields or the password; no "mTLS" anywhere; the profile form's "Security" and "Advanced" folds; no title bar pill (play badge on the Broker rail icon, count bubble on Connect, "unverified" chip on tab and card); the Broker screen reorganised (broker-wide reachability in `BrokerConfig.bind`, listener ✕, Users and Client certificates panels, "Advanced security" fold, explanations in ⓘ only, "▸ Log"). See `docs/PROGRESS.md` "Change set 4"; the implementing model's readings awaiting the owner are under "Open" in `docs/DECISIONS.md`.
- Phases 1 to 4 implemented and tested on macOS: engine, TLS kit, client, profiles with keychain, embedded broker, CLI, Tauri shell, and the full React UI for every mockup screen. Details, test counts, the native-feel checklist and the phase 5 task list are in `docs/PROGRESS.md`.
- Change set 3 (keychain prompts, 13 Oct 2026) is implemented: no migration of per-secret keychain entries, the master key is the only keychain item, a refusal is remembered for the session (`keychain.refused`), and the broker issues a missing server key again on start (B-20). See `docs/DECISIONS.md` "13 Oct 2026, keychain prompts (change set 3)".
- Change set 2 (owner feedback on the second build, 13 Oct 2026) is implemented: connected cards in the accent colour, card menu and visible Delete, connection bar under the title bar, the "Local broker" profile follows every broker change, broker-wide client certificate setting, live clients and collapsed log, Send top right, bigger buttons, macOS title alignment, secrets in `secrets.enc` with one keychain master key. Manual checks V-08 (title bar) and V-09 (keychain prompts) are the owner's. See `docs/PROGRESS.md` "Change set 2" and the open points in `docs/DECISIONS.md`.
- Change set 1 (owner feedback, 13 Oct 2026) is implemented: "New connection" menu, "Connect to this app's broker" (02b), restart prompt (12c; replaced by live apply in change set 7), shared users only (no per-listener login anywhere), eye and copy on app-owned secrets, "Test". See `docs/PROGRESS.md` "Change set 1"; readings awaiting the owner are under "Open" in `docs/DECISIONS.md`. Test lab scripts for manual testing live in `tools/testlab/` (README there); B-17 is the owner's manual run.
- Tests: Rust 200 passed (4 opt-in ignored), Vitest 65, Playwright 221 (V-02 overflow check in EN, FR, DE on 36 states; screenshots light and dark; 41 end-to-end flows with the mock engine). All green.
- Next: phase 5 (native FR and DE review, accessibility pass, native-feel walk of the real window on all three OSes, visual regression baselines in CI, installers, auto-update, website).
- Specification and mockups (12 Oct 2026, change sets 1 to 4 on 13 Oct 2026, change sets 5 to 7 on 14 Oct 2026, change sets 8 and 9 on 15 Oct 2026, change set 10 on 18 Oct 2026): `docs/SPEC.md`, `docs/FEATURES.md`, `docs/DECISIONS.md`, `docs/mockups/mqtt-panel-mockups.html` (27 screens and states). Implementation decisions and the small mockup deviations awaiting the owner are in `docs/DECISIONS.md`.

## Ground rules for the implementer
- Build exactly what the specs and the validated mockups describe. If something is missing or contradictory, record the question in `docs/DECISIONS.md` under "Open" and ask; do not invent behaviour.
- Rust engine first (`crates/engine`): pure, no I/O, no threads, Rust API plus C ABI, tested with recorded packet fixtures. The UI never touches sockets, parsing or storage.
- Native feel is a requirement (feature N1, `docs/SPEC.md` 1.2). Walk the checklist on all three OSes in the spike.
- UI rules from the mockup review: plain words in every user-facing label (for example "New broker…" / "Built-in broker", "Server name (SNI)", "Protocol tag (ALPN)"); form rows never wrap, helper text stays short and the full explanation lives in the ⓘ bubble; every file field is a native picker with drag-and-drop; every security option has an info bubble in EN, FR and DE.
- Keep the forbidden-words guard green and never commit secrets.

## Architecture map
```
UI (React + TypeScript, in Tauri 2)  <-- EngineClient (commands in, batched events out, view slices)
Desktop adapter (Tauri IPC, keychain, dialogs, broker control, updater)
crates/engine   pure state machine: topic store, ring buffers, series with casting, decoders, diff, search, views, export
crates/client   MQTT 3.1.1 and 5 over TCP, TLS (rustls), WebSocket (candidate: rumqttc)
crates/broker   embedded broker (candidate: rmqtt library mode; fallback rumqttd), cert generation (rcgen), presets
crates/cli      mqttpanel sub | pub | broker | profiles
crates/profiles profile storage, OS keychain (keyring), import and export without secrets
```

## Phases (from docs/SPEC.md section 12)
1. Spike: engine, client, broker with TLS and mTLS, 100k-topic / 5k msg/s benchmark, Tauri shell with the Explorer tree on all three OSes walked through the native-feel checklist.
2. Core desktop: profiles and keychain, connection form with TLS, Explorer tree and table, inspection with JSON, diff, properties, decoders.
3. Publish and history: composer, templates, sent log, delete retained, series and charts, export.
4. Broker: embedded broker screen, Add listener flow with presets, client certificates, authentication, clients, log. CLI.
5. Polish and release: FR and DE catalogs with native review, accessibility, native-feel review, installers, auto-update, website.

## Commands
Step by step in `docs/RUNNING.md`.
```
cargo test --workspace                      # engine, tls, client, profiles, broker, cli, desktop adapter
npm install && npm test                     # UI component tests (Vitest)
npm run e2e                                 # Playwright flows and the V-02 overflow check (mock engine)
npm run dev                                 # UI in a browser with the mock engine (http://localhost:1420)
cd apps/desktop/src-tauri && cargo tauri dev     # run the desktop app (or: npm run tauri dev)
cd apps/desktop/src-tauri && cargo tauri build   # macOS .app and .dmg under target/release/bundle
cargo deny check                            # licences, advisories, sources
scripts/check-forbidden-words.sh            # must pass before every commit
MQTTPANEL_PUBLIC_BROKERS=1 cargo test --workspace -- --ignored   # nightly: public brokers, performance
```
`MQTTPANEL_HOME=<dir>` points the app and the CLI to another data folder (tests); `MQTTPANEL_SECRETS=memory` keeps secrets in memory instead of `secrets.enc` and the OS keychain (tests only).

## Website
The site at https://mqtt-panel.com is built from `website/` (details in `website/README.md`).
```
python3 website/build.py                    # generate website/public/ (EN, FR, DE); fails on a forbidden word
python3 website/build.py --licences         # also refresh third-party.json (cargo metadata, package-lock.json)
WRANGLER=<path to wrangler> website/deploy.sh   # build, then wrangler deploy (Worker mqtt-panel-site)
```
New clips and screenshots: `npm run dev`, then `node e2e/website-media.mjs` in `packages/ui`, then `website/encode-media.sh` (ffmpeg). The store and GitHub links are constants at the top of `build.py`; the operator details on the legal page are the owner's to fill.

## Mockups
- `docs/mockups/mqtt-panel-mockups.html` is the validated visual contract (27 screens and states: 22 validated 12 Oct 2026, 02b and 12c plus edits from change set 1, 02c and the connection bar from change set 2, 02d, the rail badges and the reorganised Broker screen from change set 4, all on 13 Oct 2026, implemented 13 Oct 2026; 02e, the Protocol and MQTT version rows, the address prefiller, the play button and the rename to "MQTT Panel broker" from change set 5, 14 Oct 2026; three-line cards, fixed field widths, "Built-in broker" and the per-connection certificate prompt from change set 6, 14 Oct 2026). The UI must match it: layout, wording, states (empty state, trust dialog, context menus, field picker, Add listener steps, Anonymous on, French and German samples). Deviations need a note in `docs/DECISIONS.md` under "Open".

## Toolchain on the build Mac (checked 12 Oct 2026)
- Present: cargo 1.98, node 26.9, npm 11.19, Xcode 27 at `/Applications/Xcode.app`, macOS 27.0.1.
- Absent: pnpm, docker, mosquitto. They may be installed as development tools with brew or npm (`npm install -g pnpm`, `brew install mosquitto`); nothing system-level.
- Build verification for the first phases is macOS only; Windows and Linux builds and the native-feel checks on those OSes go through CI.

## Open items
- Verify library candidates at kick-off (`docs/SPEC.md` section 10): rumqttc TLS and WebSocket flags, rmqtt library API and client-cert hooks, prost-reflect and protox, keyring on Linux without Secret Service, uPlot theming.
- Windows signing: cheapest hassle-free option for end users; candidate Azure Trusted Signing (verify eligibility and price). Betas are unsigned on Windows and macOS; signing and notarization are the last step before a release.
- Export compliance review before the first release (owner).
- Website at https://mqtt-panel.com (static, EN/FR/DE, no cookies or trackers): built 18 Oct 2026 in `website/`; see "Website" below.

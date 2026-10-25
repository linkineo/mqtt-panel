# Handoff

How to pick up MQTT Panel. Read `CLAUDE.md` first; it carries the standing rules. Public repository:
https://github.com/linkineo/mqtt-panel (`git clone https://github.com/linkineo/mqtt-panel.git`).

## Where things are

| Path | Content |
|---|---|
| `crates/engine` | Pure state machine: topic store, ring buffers, series with casting, decoders, diff, search, views, export (Rust API and C ABI) |
| `crates/client` | MQTT 3.1.1 and 5 over TCP, TLS (rustls, ring) and WebSocket, on rumqttc's packet codec |
| `crates/broker` | Embedded broker, live apply, presets, users, client certificates, local CA |
| `crates/tls` | Certificate, key and PKCS#12 parsing, fingerprints, certificate and CSR generation |
| `crates/profiles` | Connection profiles, `secrets.enc`, master key store (`keystore.rs`), import |
| `crates/cli` | The `mqttpanel` binary: `sub`, `pub`, `broker`, `profiles`, `mcp` |
| `crates/shim` | The MCP stdio shim that forwards to the running app's socket |
| `apps/desktop/src-tauri` | Tauri 2 shell: commands, native menus, dialogs, broker control, MCP server (`src/mcp/`), store configuration |
| `packages/ui` | React UI, i18n catalogs (`src/i18n/{en,fr,de}`), Vitest tests, Playwright suites (`e2e/`) |
| `packages/engine-client` | `EngineClient` interface, the Tauri adapter and the mock engine used by UI tests and the browser build |
| `website/` | Static site https://mqtt-panel.com (EN, FR, DE), one Cloudflare Worker |
| `scripts/` | Forbidden-words guard, app icon, Mac App Store release, MSIX build, App Store Connect scripts (`asc/`) |
| `tools/testlab/` | Manual test lab: Mosquitto in five security modes, Paho clients against the built-in broker |
| `tests/fixtures/` | Synthetic fixtures (packets, import files) |

## Documents

| File | Use |
|---|---|
| `docs/SPEC.md` | Architecture, security model, broker, CLI, libraries, release gates, phases |
| `docs/FEATURES.md` | Every feature and its exact behaviour and wording |
| `docs/TESTPLAN.md` | Test ids and acceptance criteria; tests name the id they cover |
| `docs/MCP.md`, `docs/MCP_PROMPTS.md` | The assistant interface and its prompt lab |
| `docs/DECISIONS.md` | Decisions in force and readings awaiting the owner |
| `docs/PROGRESS.md` | Status per area, last test counts, open items |
| `docs/RUNNING.md` | Running, testing and releasing step by step |
| `docs/WINDOWS_BUILD.md` | Building and testing the MSIX on a Windows machine |
| `docs/store/` | Mac App Store listing, privacy declaration, review notes |
| `docs/mockups/mqtt-panel-mockups.html` | The validated visual contract: layout, wording, states |

## Rules for the implementer

- Build exactly what the specs and the mockups describe. If something is missing or contradictory, add a one-line
  reading under "Readings awaiting the owner" in `docs/DECISIONS.md` and ask; do not invent behaviour.
- The engine stays pure: no I/O, no threads. The UI never touches sockets, parsing or storage.
- Native feel is a requirement (`docs/SPEC.md` 1.2).
- Plain words in labels; form rows never wrap; every file field is a native picker with drag-and-drop; every security
  option has an ⓘ bubble in EN, FR and DE.
- Keep the forbidden-words guard green and never commit secrets.
- MQTT Panel is source-available (Apache 2.0 with the Commons Clause); never call it "open source".

## Build and test

Step by step in `docs/RUNNING.md`.
```
cargo test --workspace                      # Rust crates and the desktop adapter
npm install && npm test                     # UI component tests (Vitest)
npm run e2e                                 # Playwright: flows, V-02 overflow check in EN/FR/DE, screenshots (mock engine)
npm run dev                                 # UI in a browser with the mock engine (http://localhost:1420)
npm run tauri dev                           # the desktop app
cargo deny check                            # licences, advisories, sources
scripts/check-forbidden-words.sh            # must pass before every commit
"<app executable>" mcp                      # the MCP stdio shim (or: mqttpanel mcp); docs/RUNNING.md section 10
MQTTPANEL_PUBLIC_BROKERS=1 cargo test --workspace -- --ignored   # nightly: public brokers, performance
```
`MQTTPANEL_HOME=<dir>` points the app and the CLI to another data folder; `MQTTPANEL_SECRETS=memory` keeps secrets in
memory (tests only); `MQTTPANEL_KEYSTORE=file|keychain` forces the master key store; `MQTTPANEL_LOG=1` prints
diagnostics without payloads.

## Release

- **Mac App Store:** `scripts/release-mac-testflight.sh` builds the sandboxed variant, signs it and uploads it to
  TestFlight; the listing is maintained with `scripts/asc/` (`docs/store/README.md`). Only the owner submits for review.
- **Microsoft Store:** MSIX built on a Windows machine with `scripts/build-msix.ps1` per `docs/WINDOWS_BUILD.md`; the
  Store signs it.
- No installers, no updater, no Linux packages. Release gate: the signed macOS build shows no keychain dialog
  (`docs/TESTPLAN.md` V-12).

## Website

```
python3 website/build.py                    # generate website/public/ (EN, FR, DE); fails on a forbidden word
python3 website/build.py --licences         # also refresh third-party.json
WRANGLER=<path to wrangler> website/deploy.sh   # build, then deploy the Worker
```
New clips and screenshots: `npm run dev`, then `node e2e/website-media.mjs` in `packages/ui`, then
`website/encode-media.sh` (ffmpeg). Store links are constants at the top of `build.py`. Details in `website/README.md`.

## Open items

See `docs/PROGRESS.md` "Open items": Windows MSIX, native FR/DE review, store submission steps, the Data Protection
keychain gate, accessibility and visual-regression work in CI. Readings awaiting the owner are in `docs/DECISIONS.md`.

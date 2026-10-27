# Progress

Implementation status of MQTT Panel against `docs/SPEC.md` section 12. Version 1.0.0.

## Status per area

| Area | Implemented and tested | Remaining |
|---|---|---|
| Engine (`crates/engine`) | Topic store, ring buffers, series with casting, decoders (JSON, CBOR, MessagePack, Sparkplug B, Protobuf with user `.proto`), diff, search, views, export; C ABI; memory linear per topic, topics cut at the profile's limit (1024 bytes by default) | The memory budget is reported, not enforced |
| Client (`crates/client`) | MQTT 3.1.1 and 5 over TCP, TLS and WebSocket; SNI and ALPN; trust prompt with pinning or CA; per-connection skip of verification; client certificate by import or by generated key and CSR | |
| Profiles and secrets (`crates/profiles`) | Profiles without secrets; `secrets.enc` with one master key; key file on unsigned builds, OS store on signed builds, one silent move between them | macOS Data Protection keychain for signed builds (release gate V-12) |
| Broker (`crates/broker`) | Listeners TCP, TLS, WS, WSS; presets; shared users and Anonymous; client certificates (issue, import CSR, revoke, export); local CA and server certificate with LAN names; live apply; clients list; live log; limits (128 topic levels, waiting connections, CONNECT size and login delay before authentication, parallel password checks), in-flight timeout, control orders on their own channel; allowed web origins with the owner prompt | |
| CLI (`crates/cli`) | `sub`, `pub`, `broker`, `profiles`, `mcp` | |
| Desktop app (Tauri 2, React) | Every mockup screen: Connections, Explorer (tree, table, inspection, History, topic chart), Publish, Charts, Export, Broker, Settings, command palette; native menus in EN, FR, DE | Native-feel walk of the real window on Windows and Linux (V-03) |
| Assistant (MCP) | Same-user socket, stdio shim, acknowledgement, secrets in the app, toast and Assistant log; 57 tools (Connections, Broker, Data, Charts, Navigation), five resources, four prompts; "Add to Claude" and "Add to ChatGPT"; in-app confirmations for the actions that weaken security or write outside the usual folders; owner-only Windows named pipe with the shim's server check | A session from Claude Desktop and the ChatGPT desktop app; the Windows pipe tests M-41 and M-42 on a Windows machine |
| Languages | EN, FR and DE catalogs complete; catalog tests (keys, placeholders, ⓘ bubbles, blocked words, French typography, German "du") | Native FR and DE review (V-06) |
| Website | https://mqtt-panel.com from `website/` (EN, FR, DE, no cookies or trackers) | Store links, official App Store badge, operator details on the legal page (owner) |
| Mac App Store | Sandboxed variant, `scripts/release-mac-testflight.sh`, TestFlight build, listing set through the API (`docs/store/README.md`) | Owner steps in the web UI: App Privacy, review contact, copyright line, submission |
| Microsoft Store | `docs/WINDOWS_BUILD.md`, `scripts/build-msix.ps1` | MSIX built and tested on a Windows machine; Store listing |

## Last test run (25 Oct 2026, macOS 27)

| Suite | Result |
|---|---|
| Rust workspace (`cargo test --workspace`) | 286 passed, 4 opt-in ignored |
| Vitest (`npm test`) | 197 passed |
| Playwright (`npm run e2e`: V-02 overflow in EN, FR, DE, V-01 screenshots light and dark, end-to-end flows with the mock engine) | 348 passed |
| MCP prompts lab (`tools/testlab/mcp_prompts/run.py`, cases C12a, C12b, C14) | 3 passed |
| `python3 website/build.py` | OK |

Opt-in tests (`-- --ignored`, nightly): public brokers (`MQTTPANEL_PUBLIC_BROKERS=1`), performance (B-14, Q-02).

## Measured numbers (MacBook, release builds)

| Measure | Result | Target |
|---|---|---|
| Engine ingest (nested JSON, 2 000 topics) | 1.7 million msg/s | 5 000 msg/s |
| Tree slice, 60 rows of 100 927 visible | 3.3 ms first, 0.009 ms cached | < 2 ms (X-01) |
| Search over 100 000 topics (substring / wildcard / regex) | 1.7 / 3.2 / 1.0 ms | < 50 ms |
| Full pipeline at 5 000 msg/s for 10 s (Mosquitto, client, adapter, engine) | 50 000 / 50 000, no backlog | no backlog > 1 s (Q-02) |
| Embedded broker, 1 000 publishers + 4 subscribers, QoS 1 (B-14) | 4 600 msg/s in, 18 400 msg/s out, 0 dropped | 5 000 in, 20 000 out |
| Memory estimate accuracy (Q-05) | within 8 bytes of 38.9 MB | within 10 % |

## Open items

- **Windows:** MSIX build and test on a Windows machine per `docs/WINDOWS_BUILD.md`, including the named-pipe tests M-41 and M-42 (the pipe code was compiled for Windows only through a check of the shim sources); Microsoft Store listing.
- **Native review:** every FR and DE string by a native reader (V-06), including the assistant texts.
- **Store submission (owner):** App Privacy "Data Not Collected", App Review contact, copyright line, then submit (`docs/store/README.md`).
- **Data Protection keychain gate:** the signed macOS build keeps the master key in the Data Protection keychain and shows no keychain dialog on a fresh account (`docs/SPEC.md` 11, `docs/TESTPLAN.md` V-12). Not implemented yet; it blocks the release.
- **Quality:** accessibility pass (V-07: axe-core, screen-reader labels, AA contrast, VoiceOver and NVDA); visual regression baselines per OS in CI (V-01); Playwright on Windows and Linux; tauri-driver smoke and Q-01 frame time in the nightly job.
- **Coverage gaps:** X-07 (pinned topics and saved views survive a restart) and C-01 at UI level are covered in Rust only; `scripts/check-testplan-coverage.sh` is not written yet.
- **Mockup details:** "since connect" wording on the tree root row (04); "n s left when received" on Message expiry (07).
- **Owner readings:** see "Readings awaiting the owner" in `docs/DECISIONS.md`.

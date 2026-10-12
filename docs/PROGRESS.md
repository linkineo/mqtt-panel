# Progress

Implementation status of MQTT Panel against `docs/SPEC.md` section 12. Updated at the end of each phase.
Last update: 13 Oct 2026 (phases 1 to 4, first implementation pass; macOS only).

## Summary

| Phase | Scope | Status |
|---|---|---|
| 1 Spike | engine, client, broker with TLS and mTLS, benchmark, Tauri shell, native-feel walk | Done on macOS. Windows and Linux builds and walks pending (CI). |
| 2 Core desktop | profiles and keychain, Connections with TLS form and trust prompt, Explorer tree and table, inspection, decoders | See "UI" below |
| 3 Publish and history | composer, templates, sent log, delete retained, repeat, request and response, charts, export | See "UI" below |
| 4 Broker and CLI | Broker screen, Add listener, client certificates, authentication, clients, log, Connect Explorer here, CLI | Backend and CLI done; screen see "UI" below |

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
| `apps/desktop` | Tauri 2 shell: native menu bar (macOS conventions), 90 commands, 20 Hz event ticker, native dialogs, Connect Explorer here | 5 / 0 / 1 |

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

## Open tasks

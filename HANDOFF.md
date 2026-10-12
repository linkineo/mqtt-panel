# Handoff

How to pick up MQTT Panel. Read `CLAUDE.md` first; it carries the standing rules.

## Status (12 Oct 2026)
- Specification phase complete for v1.0: `docs/SPEC.md`, `docs/FEATURES.md`, `docs/DECISIONS.md`.
- Next: a detailed mockup artifact is reviewed by the owner; the validated mockups are then committed to `docs/mockups/*.html`. Implementation starts only after that.
- No code yet. The repository layout in `docs/SPEC.md` section 3 is the target.

## Ground rules for the implementer
- Build exactly what the specs and the validated mockups describe. If something is missing or contradictory, record the question in `docs/DECISIONS.md` under "Open" and ask; do not invent behaviour.
- Rust engine first (`crates/engine`): pure, no I/O, no threads, Rust API plus C ABI, tested with recorded packet fixtures. The UI never touches sockets, parsing or storage.
- Native feel is a requirement (feature N1, `docs/SPEC.md` 1.2). Walk the checklist on all three OSes in the spike.
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

## Commands (once the code exists)
```
cargo test --workspace            # engine, client, broker, cli
pnpm install && pnpm test         # UI components and flows
pnpm tauri dev                    # run the desktop app
scripts/check-forbidden-words.sh  # must pass before every commit
```

## Mockups
- Validated mockups arrive as `docs/mockups/*.html` (one file per screen, light and dark, three accents). They are the visual contract; the UI must match them.
- Until they are committed, do not start UI work beyond the spike.

## Open items
- Verify library candidates at kick-off (`docs/SPEC.md` section 10): rumqttc TLS and WebSocket flags, rmqtt library API and client-cert hooks, prost-reflect and protox, keyring on Linux without Secret Service, uPlot theming.
- Windows signing: cheapest hassle-free option for end users; candidate Azure Trusted Signing (verify eligibility and price). Betas are unsigned on Windows and macOS; signing and notarization are the last step before a release.
- Export compliance review before the first release (owner).
- Website at mqtt-panel.com (static, EN/FR/DE, no trackers), built in phase 5.

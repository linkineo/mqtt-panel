# MQTT Panel

Open-source MQTT client, embedded broker and command line for Windows, macOS and Linux. Website: [mqtt-panel.com](https://mqtt-panel.com).

MQTT Panel is the tool an engineer opens to understand what is happening on a broker: which topics exist, what the last payload was, how a value moved over the last hour, and whether a device answered. It stays fast with a hundred thousand topics, treats security as a first-class feature (certificates, mutual TLS, OS keychain), and lets the same person stand up a local broker in two clicks when they have none.

**Status: pre-alpha.** Phases 1 to 4 are implemented (engine, client, broker, CLI, desktop UI for every screen of the validated mockups); phase 5 (native language review, accessibility, installers, auto-update, website) is next. See `docs/RUNNING.md` to build and start it, `docs/PROGRESS.md` for the state of each feature and `HANDOFF.md` to pick the project up.

## What it does (v1.0 scope)

- **Connections**: profiles for mqtt, mqtts, ws and wss; MQTT 3.1.1 and 5; TLS with custom CA, client certificates, self-signed brokers trusted by fingerprint; passwords and keys in the OS keychain.
- **Explore**: a topic tree that stays collapsed until you open it, with inline values, sparklines, new-message highlights and message counts; a flat, sortable table view; search with wildcards and regex; pinned topics and saved views.
- **Inspect**: JSON, text, hex, base64, Sparkplug B, CBOR, MessagePack and Protobuf (with your own `.proto` files); diff against previous messages; MQTT 5 properties.
- **Publish**: a standalone composer with QoS, retain, delete-retained and MQTT 5 properties; one-off by default, templates on request; a sent log with broker acknowledgements and one-click re-send; request and response on MQTT 5; a quick-publish row right in the Explorer.
- **Charts**: any numeric payload or JSON attribute charted automatically, several series per chart, two axes, pause, export as CSV or PNG. History stays in memory; export what you want to keep.
- **Broker**: an embedded MQTT broker managed in the same window, with guided setups (plain, encrypted, passwords, mutual TLS), a local CA, client certificates issued per device, live clients and log.
- **CLI**: `mqttpanel sub`, `pub`, `broker` and `profiles`, sharing the app's saved profiles.
- **Languages**: English, French, German. **Privacy**: no telemetry, no accounts, no network calls except to the brokers you configure.

## Platforms

Windows (MSI, winget), macOS (DMG, Homebrew cask), Linux (AppImage, deb, rpm). Built with Tauri 2, a Rust engine and a React interface. A web version is deferred; the engine is kept free of I/O so a browser build stays possible later.

## Documentation

- `docs/SPEC.md`: technical specification (toolkit decision, architecture, data model, security, embedded broker, CLI, libraries, quality and release, phases).
- `docs/FEATURES.md`: feature scope table and functional requirements.
- `docs/DECISIONS.md`: decisions taken and the decision log.
- `docs/mockups/`: validated HTML mockups (added before implementation starts).
- `docs/RUNNING.md`: build and start the app, the UI alone and the CLI.
- `docs/PROGRESS.md`: implementation status and test results.
- `docs/TESTPLAN.md`: test plan.
- `HANDOFF.md`: how to pick this project up.

## Licence

Apache License 2.0. See `LICENSE` and `NOTICE`.

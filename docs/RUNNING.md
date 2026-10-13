# Running MQTT Panel from source

How to start the desktop app, the UI alone and the command line on a development machine. Installers and auto-update come with phase 5; until then the app is built from the repository.

## Requirements

| Tool | Version | Notes |
|---|---|---|
| Rust (cargo) | 1.85 or newer | https://rustup.rs |
| Node.js and npm | Node 20 or newer | https://nodejs.org |
| Tauri CLI | 2.x | `cargo install tauri-cli --version "^2"` (gives `cargo tauri`); the npm workspace also provides `npm run tauri` |
| macOS | Xcode command line tools | `xcode-select --install` |
| Windows | Visual Studio Build Tools (C++ workload), WebView2 | WebView2 is part of Windows 11 and recent Windows 10 |
| Linux | WebKitGTK 4.1 and build tools | Debian or Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev` |

No Docker, no broker and no account are needed. The embedded broker is part of the app.

## 1. One-time setup

```
git clone git@github.com:linkineo/mqtt-panel.git
cd mqtt-panel
npm install
```

`npm install` installs the UI workspace (`packages/ui`, `packages/engine-client`, `apps/desktop`). Rust dependencies are fetched by cargo on the first build.

## 2. Start the desktop app in development mode

From the repository root:

```
npm run tauri dev
```

or, equivalently:

```
cd apps/desktop/src-tauri
cargo tauri dev
```

This starts the Vite dev server for the UI on http://localhost:1420, builds the Rust side in debug mode and opens the app window. UI edits reload live; Rust edits rebuild and restart the app. The first build takes a few minutes; later starts take seconds. Stop with Ctrl+C in the terminal.

## 3. Build and start the app bundle (release)

```
cd apps/desktop/src-tauri
cargo tauri build
```

The bundle lands under `target/release/bundle/` at the repository root:

| OS | Result |
|---|---|
| macOS | `macos/MQTT Panel.app` and a `.dmg` under `dmg/` |
| Windows | `msi/MQTT Panel_<version>_x64_en-US.msi` and `nsis/` |
| Linux | `appimage/`, `deb/`, `rpm/` |

Start it:

```
open "target/release/bundle/macos/MQTT Panel.app"          # macOS
```

On Windows and Linux open the bundle from the file manager or run the installer.

Beta builds are not signed. On macOS Gatekeeper may refuse a build that was downloaded from elsewhere; a bundle built locally opens without a prompt. A downloaded unsigned build opens with right-click, Open, then Open again. On Windows SmartScreen shows "Windows protected your PC"; choose More info, then Run anyway. Release builds will be signed (see `docs/SPEC.md` section 11).

## 4. UI alone in a browser (mock engine)

```
npm run dev
```

Opens the interface on http://localhost:1420 with a mock engine: every screen, dialog and state from the mockups is reachable without a broker. This is what the Playwright flows and the overflow check use. It never talks to a real broker.

## 5. Command line

```
cargo build --release -p mqttpanel-cli
./target/release/mqttpanel --help
./target/release/mqttpanel sub -h localhost -p 1883 -t "#"
./target/release/mqttpanel pub -h localhost -p 1883 -t demo/hello -m 'hi'
./target/release/mqttpanel broker --bind 127.0.0.1
./target/release/mqttpanel profiles list
```

The CLI shares the app's saved profiles and secrets (`secrets.enc` and its master key in the OS keychain).

## 6. Where the app stores data

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/com.remora-labs.mqttpanel/` |
| Windows | `%APPDATA%\com.remora-labs.mqttpanel\` |
| Linux | `~/.local/share/com.remora-labs.mqttpanel/` |

Profiles, settings, templates, saved views and broker configuration live there as JSON. Passwords, private keys and passphrases are in `secrets.enc` in the same folder, encrypted (XChaCha20-Poly1305) with a random master key that is the only MQTT Panel item in the OS keychain; the key is read the first time a secret is needed in a session, never at start-up. Per-secret keychain entries of earlier builds are moved into the file once and deleted. Message history stays in memory and is gone when the app closes.

Environment variables for tests and diagnostics:

| Variable | Effect |
|---|---|
| `MQTTPANEL_HOME=<dir>` | Use another data folder (tests, portable use) |
| `MQTTPANEL_SECRETS=memory` | Keep secrets in memory instead of `secrets.enc` and the keychain (tests only) |
| `MQTTPANEL_LOG=1` | Print page loads and the name of every IPC command to stderr (no payloads) |

Example, a clean sandbox that leaves the real profiles and keychain untouched:

```
MQTTPANEL_HOME=/tmp/mqttpanel-sandbox MQTTPANEL_SECRETS=memory open "target/release/bundle/macos/MQTT Panel.app"
```

(On macOS `open` passes the environment to the app.)

## 7. First steps in the app

1. On first launch the Connect screen shows three cards: new connection, start a local broker, import from another tool.
2. Quickest way to see live data: click "Start a local broker" (or Connect, "New connection", "This app's broker", "Use an existing listener"). The dialog preselects the most secure listener; "Connect" creates the user "mqtt-panel", the client certificate when the listener needs one, trusts the broker's CA, starts or restarts the broker and connects with the profile "Local broker".
3. Publish something from the Publish screen (topic `demo/temperature`, payload `{"value": 21.5}`) and watch it appear in the Explorer tree. Right-click the `value` key in the payload and choose "Add value to chart".
4. For a real broker: Connect, "New connection", "Another broker…", host and port, then "Test". Self-signed brokers offer "Trust this certificate" with the fingerprint.

## 8. Tests

```
cargo test --workspace          # Rust crates
npm test                        # Vitest, UI components
npm run e2e                     # Playwright flows and the overflow check in EN, FR and DE
cargo deny check                # licences and advisories
scripts/check-forbidden-words.sh
```

Tests that use public brokers are opt-in: `MQTTPANEL_PUBLIC_BROKERS=1 cargo test --workspace -- --ignored`.

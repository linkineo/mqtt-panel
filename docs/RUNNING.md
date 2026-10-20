# Running MQTT Panel from source

How to start the desktop app, the UI alone and the command line on a development machine. Installers and auto-update come with phase 5; until then the app is built from the repository.

## Requirements

| Tool | Version | Notes |
|---|---|---|
| Rust (cargo) | 1.88 or newer | https://rustup.rs |
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

Where the master key lives (change set 10, 18 Oct 2026): an unsigned build (dev, `npm run tauri dev`, CI, an unsigned beta) keeps the master key in `master.key` (owner-only) in the app data folder and never shows a keychain dialog. A build that still finds the old login-keychain item reads it once, writes the file and deletes the item: that is the last dialog you see. A build signed with a real identity (Developer ID, or a local signing identity for testing that path; ad-hoc does not count) keeps the key in the OS secret store (macOS: Data Protection keychain once the signing step is done, login keychain before that; Windows: DPAPI; Linux: Secret Service or the file fallback), which is silent for end users. `MQTTPANEL_KEYSTORE=file` or `=keychain` forces the store for tests. With `MQTTPANEL_LOG=1` the chosen store and a move are printed once. A `master.key` that other users can read is not used (fix it with `chmod 600`). The CLI follows the same rule on the same data folder.

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

Profiles, settings, templates, saved views and broker configuration live there as JSON. Passwords, private keys and passphrases are in `secrets.enc` in the same folder, encrypted (XChaCha20-Poly1305) with a random master key that is the only MQTT Panel item in the OS keychain; the key is read the first time a secret is needed in a session, never at start-up. That master key is the only keychain item the app reads or writes; entries of earlier test builds are ignored and can be removed in Keychain Access (service "MQTT Panel"). Message history stays in memory and is gone when the app closes.

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

External clients with the broker's own chain (T-31, Paho against `mqttpanel broker`; setup in `tools/testlab/README.md`):

```
tools/testlab/.venv/bin/python tools/testlab/external_clients.py
```

## 9. TestFlight build for macOS

`scripts/release-mac-testflight.sh` builds the sandboxed Mac App Store variant, signs it, wraps it in a signed installer
package and uploads it to App Store Connect, where TestFlight picks it up after processing. It reads `.env.local`
(copy `.env.example`): the App Store Connect key ID and issuer, the team identifier, the two identity names
("Apple Distribution" and "3rd Party Mac Developer Installer", both in the login keychain) and the path of the Mac App
Store provisioning profile kept outside the repository. The variant is defined by `apps/desktop/src-tauri/tauri.mas.conf.json`
(entitlements from `Entitlements.mas.plist`: app sandbox, network client and server, user-selected files) merged onto
`tauri.conf.json`; the build number is the UTC time of the build. The sandboxed app keeps its data under
`~/Library/Containers/com.remora-labs.mqttpanel/`, apart from the unsandboxed builds. Export compliance is answered
in App Store Connect by the owner, not in the build. Pass `--no-upload` to stop after the package.

## 10. Assistant (MCP)

MQTT Panel is an MCP server for assistants (`docs/MCP.md`). Settings, Assistant does the setup with one button per host;
this section is the same by hand. Every host starts the app's own executable with the single argument `mcp` (or the
command line `mqttpanel mcp`). Settings, Assistant, "Other hosts" shows the exact path of the executable on this
computer; on macOS it is `/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel`. The app must accept the
acknowledgement once (Settings, Assistant) before an assistant may change anything.

**Claude Desktop.** Edit `claude_desktop_config.json` (macOS `~/Library/Application Support/Claude/`, Windows
`%APPDATA%\Claude\`, Linux `~/.config/Claude/`; Claude Desktop, Settings, Developer, Edit Config opens it) and add:

```json
{
  "mcpServers": {
    "mqttpanel": { "command": "/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel", "args": ["mcp"] }
  }
}
```

Quit and reopen Claude Desktop.

**Claude Code.** `claude mcp add --scope user mqttpanel -- "/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel" mcp`
(this writes `mcpServers.mqttpanel` into `~/.claude.json`). `claude mcp list` shows it; in a session `/mcp` shows its
tools.

**ChatGPT desktop app.** It uses the Codex configuration `~/.codex/config.toml` (shared with the Codex CLI and editor
extension; also editable in the app under Settings, MCP servers):

```toml
[mcp_servers.mqttpanel]
command = "/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel"
args = ["mcp"]
```

ChatGPT on the web cannot reach a program on this computer.

**Other hosts.** Any host that starts local stdio servers takes the JSON block of Claude Desktop above.

**Check by hand.** With the app running:

```
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"terminal","version":"1"}}}' | "/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel" mcp
```

prints the server's answer (name, version, capabilities). Only one assistant can be connected at a time.

**Environment** (tests and portable use): `MQTTPANEL_HOME` (data folder, so also the socket `mcp.sock`),
`MQTTPANEL_APP` (the program the shim starts when the app is not running), `MQTTPANEL_SHIM_WAIT_S` (how long it waits,
default 20), `MQTTPANEL_HOSTS_HOME` (the home folder the Settings buttons write into).

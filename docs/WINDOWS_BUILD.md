# Building MQTT Panel for the Microsoft Store (Windows)

Written for an implementing model (a Claude instance) running on a Windows PC with this repository cloned. Follow it literally, in order. When a step fails, stop, record the exact command and output, and report it (section 8); do not work around it by changing the product.

## 1. Goal

Produce the **Microsoft Store package (MSIX)** of MQTT Panel, version from `apps/desktop/src-tauri/tauri.conf.json` (1.0.0 gives package version `1.0.0.0`), and check it on this machine.

- Distribution is the Microsoft Store and the Mac App Store only. No MSI, no NSIS installer, no winget, no updater.
- **No code-signing certificate is needed.** The MSIX is uploaded **unsigned** to Partner Center; the Store signs it. A self-signed certificate is used only to install the package on this machine for testing.
- The package is built from the Tauri release executable plus `apps/desktop/windows/AppxManifest.xml`, by `scripts/build-msix.ps1`.

## 2. Rules you must never break

- Read `CLAUDE.md` at the repository root first; it applies to you.
- Never commit secrets: no `.pfx`, `.p12`, `.cer`, private keys, passwords, Partner Center values filled into files, or a filled `AppxManifest.xml`. The Partner Center identity values are passed as script parameters only.
- Never write personal names, personal e-mail addresses, employer names or local paths (for example `C:\Users\<name>\...`) into any file of the repository, a commit message or a report meant for the repository. No company or other brand name appears anywhere; the app and the docs reference the website **mqtt-panel.com** only. The predecessor tool is never named.
- Do not touch `website/`. Do not change UI texts, the mockups or the specs; report what you think should change instead.
- Do not run the macOS scripts (`scripts/release-mac-testflight.sh`).
- Commits: only when asked, never `git add -A`, add files by name. Identity and trailer:
  ```powershell
  git -c user.name="MQTT Panel" -c user.email="noreply@mqtt-panel.com" commit
  ```
  The message ends with `Co-Authored-By: Claude <your model name> <noreply@anthropic.com>`. Do not push unless the owner asks.

## 3. Prerequisites (PowerShell, once)

Run in a normal PowerShell unless "elevated" is stated. Close and reopen PowerShell after installs so `PATH` is refreshed.

1. **Git for Windows**, if `git --version` fails:
   ```powershell
   winget install --id Git.Git -e
   ```
2. **Visual Studio 2022 Build Tools with the C++ workload** (MSVC compiler and linker):
   ```powershell
   winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
3. **Windows SDK** (gives `makeappx.exe`, `makepri.exe`, `signtool.exe` under `C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\`):
   ```powershell
   winget install --id Microsoft.WindowsSDK.10.0.26100 -e
   ```
   Check: `Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe"` lists at least one file.
4. **Rust with the MSVC toolchain** (1.88 or newer):
   ```powershell
   winget install --id Rustlang.Rustup -e
   rustup default stable-x86_64-pc-windows-msvc
   rustup update
   rustc --version
   ```
   `rustc --version` must show 1.88 or newer; `rustup show` must show `x86_64-pc-windows-msvc`.
5. **Node.js LTS** (20 or newer):
   ```powershell
   winget install --id OpenJS.NodeJS.LTS -e
   node --version
   ```
6. **WebView2 runtime**: part of Windows 11 and of updated Windows 10. Check:
   ```powershell
   Get-ItemProperty "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue | Select-Object pv
   ```
   If nothing prints: `winget install --id Microsoft.EdgeWebView2Runtime -e`.
7. **Tauri CLI**: comes through npm (`@tauri-apps/cli` in `apps/desktop`), installed by `npm ci` below. Use it as `npm run tauri -- <command>` from the repository root. Do not install another version globally.
8. **Optional, for the test lab** (`tools/testlab/`): Python 3.12 (`winget install --id Python.Python.3.12 -e`). Not needed for the package.

Script execution: run the build script with `powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 ...`; do not change the machine-wide execution policy.

## 4. Get the code and the dependencies

```powershell
git pull
git status            # must be clean before you start
npm ci
npx --workspace @mqttpanel/ui playwright install chromium
```

## 5. Build and test

From the repository root:

```powershell
cargo test --workspace
npm run typecheck
npm test                      # Vitest (packages/ui)
npm run e2e                   # Playwright against the UI with the mock engine
npm run tauri -- build --no-bundle
```

- `cargo test --workspace`: record the total passed, failed and ignored. Some tests start an external Mosquitto when one is installed and are skipped otherwise; that is expected. The Windows-only test **M-41** (`cargo test -p mqttpanel-shim m41`: the named pipe's access list holds the current user only, and the shim's server check passes for a pipe of this user) runs only here; report its result by name. The pipe code (`crates/shim/src/winsec.rs`, `listen` in `apps/desktop/src-tauri/src/mcp/server.rs`) was written without a Windows machine: a compile error there is expected to be possible, report it with the message.
- `npm run e2e` takes several minutes (the overflow suite renders every state in EN, FR and DE). Record the totals.
- `npm run tauri -- build --no-bundle` builds the UI and the release executable under `target\release\` (`mqtt-panel.exe`, or `MQTT Panel.exe` when the Tauri CLI applies `mainBinaryName`). **Do not build the MSI or NSIS bundles**: `tauri.conf.json` lists the macOS targets only, and the Store does not want them. If `--no-bundle` is refused by the installed Tauri CLI, report it.
- Start `target\release\mqtt-panel.exe` (or `MQTT Panel.exe`) once by hand: the window opens, the Settings "Updates" page shows "Updates come from the store this copy was installed from.", no console window appears.

## 6. Produce the MSIX

### 6.1 Values from Partner Center

The owner reserved the app name **MQTT Panel** in Partner Center. On the app's **Product management > Product identity** page, three values are needed. Ask the owner for them if you cannot see the page; never write them into a file in the repository:

| Script parameter | Partner Center field | Shape |
|---|---|---|
| `-IdentityName` | Package/Identity/Name | for example `12345Publisher.MQTTPanel` |
| `-Publisher` | Package/Identity/Publisher | `CN=` followed by an identifier |
| `-PublisherDisplayName` | Package/Properties/PublisherDisplayName | the publisher's display name |

### 6.2 What the template declares

`apps/desktop/windows/AppxManifest.xml` (placeholders `{{IDENTITY_NAME}}`, `{{PUBLISHER}}`, `{{PUBLISHER_DISPLAY_NAME}}`, `{{VERSION}}`):

- Display name "MQTT Panel", executable `mqtt-panel.exe`, entry point `Windows.FullTrustApplication`, x64, Windows 10 1809 (`10.0.17763.0`) or newer.
- Languages `en-US`, `fr-FR`, `de-DE` (the Store listing is in all three).
- Capabilities `internetClient`, `internetClientServer`, `privateNetworkClientServer`, plus `runFullTrust`, which every packaged Win32 desktop app needs (Partner Center asks for a one-line justification: "Desktop application built with a Win32 executable").
- No file type associations, no protocol handlers, no app execution alias, no startup task.
- Logos from `apps/desktop/src-tauri/icons/`: `StoreLogo.png`, `Square44x44Logo.png`, `Square71x71Logo.png`, `Square150x150Logo.png`, `Square310x310Logo.png`.

### 6.3 Build the package

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 `
  -IdentityName "<Package/Identity/Name>" `
  -Publisher "<CN=...>" `
  -PublisherDisplayName "<publisher display name>"
```

Add `-SkipBuild` when section 5 already built the executable. The script:

1. builds the executable (`npm run tauri -- build --no-bundle`) unless `-SkipBuild`;
2. lays out `target\msix\package\` (executable, `Assets\`, filled `AppxManifest.xml`; `target\` is ignored by git);
3. runs `makepri createconfig` and `makepri new` (writes `resources.pri`), then `makeappx pack`;
4. prints the path of the **unsigned** package: `target\msix\MQTTPanel_<version>_x64.msix`.

The same steps by hand, if the script fails and you need to find out where:

```powershell
$sdk = (Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*" -Directory | Sort-Object { [version]$_.Name } | Select-Object -Last 1).FullName + "\x64"
& "$sdk\makepri.exe" createconfig /cf target\msix\priconfig.xml /dq en-US_fr-FR_de-DE /pv 10.0.0 /o
& "$sdk\makepri.exe" new /pr target\msix\package /cf target\msix\priconfig.xml /mn target\msix\package\AppxManifest.xml /of target\msix\package\resources.pri /o
& "$sdk\makeappx.exe" pack /d target\msix\package /p target\msix\MQTTPanel_1.0.0.0_x64.msix /o
```

### 6.4 Test it on this machine

Windows installs only signed packages, so sign a **copy** with a self-signed test certificate whose subject equals the Publisher value:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 -IdentityName "..." -Publisher "CN=..." -PublisherDisplayName "..." -SkipBuild -TestSign
```

- The script creates the certificate "MQTT Panel local test" in `Cert:\CurrentUser\My` (once), exports the public part to `target\msix\test-signing.cer`, and signs `target\msix\MQTTPanel_<version>_x64_test-signed.msix` with `signtool sign /fd SHA256`. No `.pfx` file and no password are created.
- Trust the certificate once, from an **elevated** PowerShell (the script does it itself when it runs elevated):
  ```powershell
  Import-Certificate -FilePath target\msix\test-signing.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
  ```
- Install with `-Install` added to the command above, or by hand: `Add-AppxPackage -Path target\msix\MQTTPanel_1.0.0.0_x64_test-signed.msix`.
- Optional but recommended: run the **Windows App Certification Kit** on the installed package (`appcertui.exe` from the Windows SDK, "Validate Store App") and record the result.
- Remove the test install afterwards: `Get-AppxPackage *MQTTPanel* | Remove-AppxPackage`.

Then walk the checks of section 7 on the **installed** package (not on the bare executable).

### 6.5 Upload

Upload the **unsigned** `target\msix\MQTTPanel_<version>_x64.msix` (never the `_test-signed` copy) in Partner Center: the app's submission > Packages. Partner Center validates the manifest; the Store signs the package. If you cannot reach Partner Center, hand the file path to the owner. The Store listing texts (EN, FR, DE) and screenshots are the owner's and the spec model's job, not yours.

## 7. Checks on the installed package

For each item record pass or fail with a short note.

1. Starts from the Start menu; window title "MQTT Panel"; icon correct in Start, taskbar and title bar; no console window.
2. Settings: language switch EN, FR, DE at runtime; "Updates" shows the store sentence; About shows "Apache 2.0 with the Commons Clause · free to use, not for resale."
3. Connections: connect to a public test broker over `mqtt` and `mqtts`, and to the built-in broker ("Built-in broker" flow).
4. Built-in broker: starts, listeners on 1883 and 8883; the Windows Defender Firewall prompt appears for "Whole network" and the app keeps working when it is refused (localhost still works); a second device, or the CLI built with `cargo build --release -p mqttpanel-cli` (`target\release\mqttpanel.exe sub`), connects.
5. File dialogs: every Browse button opens the native dialog; drag-and-drop of a certificate file works; certificate export writes where chosen.
6. Secrets: set a broker password, quit, restart: the password is still there and no dialog appeared (DPAPI through the Windows credential store).
7. Assistant (MCP): with the app running, `mqtt-panel.exe mcp` from a terminal (inside the package path, see open point 1) connects to the app's named pipe.
8. **M-42**, pipe owned by another user: sign in as a second Windows user, create `\\.\pipe\mqttpanel-mcp-<first user>` there (for example by starting MQTT Panel as that user with the first user's name in `USERNAME`, or with a small PowerShell `NamedPipeServerStream`), then start MQTT Panel as the first user: Settings, Assistant shows that another program uses the assistant connection point; `mqtt-panel.exe mcp` run by the first user stops with the error that the pipe belongs to another user and sends nothing.

## 8. Report back to the owner and the spec model

Report in plain text (no local paths with user names, no Partner Center values): the commit you built, tool versions (`rustc`, `node`, Tauri CLI from `npm run tauri -- --version`, Windows SDK), test totals of section 5, the package path and size, the WACK result, the section 7 table, and these **open points**, each answered with what you observed:

1. **MCP named pipe.** The app listens on `\\.\pipe\mqttpanel-mcp-<user>` (`apps/desktop/src-tauri/src/mcp/server.rs`, `crates/shim`). Does the pipe work from the MSIX package (full trust)? Can an assistant host outside the package (for example Claude Desktop) start the shim, given that the package's install folder under `WindowsApps` is not meant to be referenced by path? Would an app execution alias be needed (it is not declared; do not add it, report)?
2. **DPAPI key store.** `crates/profiles/src/keystore.rs` keeps the master key in the OS store on Windows (credential manager, DPAPI, user scope). Confirm: no dialog at first start, first secret use, restart, and after reinstalling the same version; the secret survives an update to a higher version.
3. **Data folder under MSIX.** The data folder is `%APPDATA%\<app id>`; MSIX redirects writes under `AppData` to the package's private folder. Does the CLI (`mqttpanel`, run outside the package) still see the same profiles and secrets? Report the folder the packaged app actually writes to (with the user name replaced by `<user>`).
4. **File dialogs** from the packaged app: open, save and folder pickers, drag-and-drop.
5. **Broker on Windows**: listeners, firewall prompt, "port in use" message (the process name is not shown on Windows; `crates/broker/src/manager.rs`), stop and restart, TLS with client certificates.
6. **TestPlan rows that need Windows** (`docs/TESTPLAN.md`): **V-03** (native-feel checklist of SPEC section 1.2 on Windows: menu bar with Settings under Edit and Quit under File, Ctrl shortcuts, file dialogs, window chrome, classic scrollbars, live theme switch, focus rings) and the **Windows part of V-12** (no keychain or credential dialog at install, first start, first secret use, after an update and from the CLI).
7. Anything in this guide that was wrong or missing (command that failed, tool name changed), with the fix you used.

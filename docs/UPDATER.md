# Self-update for the direct macOS download

Status: specified 26 Oct 2026 (owner decision), ships in 1.0.1. Applies to one variant only.

## Scope

- Only the Developer ID DMG variant built by `scripts/release-mac-dmg.sh` carries an updater. It is a cargo feature
  `self-update` of `mqttpanel-desktop`; without the feature not one line of updater code is compiled. The Mac App
  Store build, the MSIX and developer builds never enable it (Apple and Microsoft update those; the stores refuse
  self-updating apps). `AppInfo` tells the UI which case applies (`selfUpdate: bool`).
- The updater is Tauri's official plugin (`tauri-plugin-updater`, MIT/Apache 2.0), driven from Rust only. The UI
  talks to the app's own commands (`update_status`, `update_check`, `update_install`, `update_restart`, `update_set_auto`) through the
  engine adapter like every other feature; the frontend never calls the plugin directly, so no capability changes.

## Behaviour

- **Automatic check**: at launch, once the UI is up, when `settings.updates.auto` is on (default on) and the last
  check is older than 24 hours. Silent when there is nothing new or when the request fails (one log line, no UI).
- **Manual check**: Settings → Updates, button "Check now". Results in plain words: "You have the latest version",
  "MQTT Panel 1.0.2 is available", or the error in one sentence ("Could not reach mqtt-panel.com").
- **Found**: a toast "MQTT Panel 1.0.2 is available" with the button "Open Updates" (the existing toast pattern), and
  Settings → Updates shows the version, the date, the release notes (plain text, or the link to the release page)
  and the buttons "Download and install" and "Later". A dot on the Settings rail item while an update waits.
- **Install**: download with a progress bar, signature verified by the plugin (minisign), bundle replaced, then the
  button "Restart now". Nothing installs without the click. If the bundle cannot be replaced (app running from the
  DMG or Downloads under translocation, read-only location), the panel says "Move MQTT Panel to Applications first"
  instead of offering the install.
- **Settings → Updates (store builds)**: unchanged, the sentence that updates come from the store.
- **Settings → Updates (DMG)**: the toggle "Check for updates automatically" (default on), the last check time,
  the state above, and the existing link to the release notes.
- Strings in EN, FR, DE (French « vous », German "du"); the ⓘ next to the toggle says in plain words what the check
  sends (see Privacy).

## Endpoint and files

- The app asks `https://mqtt-panel.com/update/{{target}}/{{arch}}/{{current_version}}`. The Cloudflare Worker answers
  any path under `/update/` with a 302 to `https://github.com/linkineo/mqtt-panel/releases/latest/download/latest.json`
  (`Cache-Control: no-store`); nothing is logged or stored. The address stays under the project's control, the
  content lives on the GitHub release.
- `latest.json` is produced by `scripts/release-mac-dmg.sh` next to the DMG from the updater artifacts
  (`bundle.createUpdaterArtifacts: true` in the DMG overlay `tauri.dmg.conf.json`): version, `pub_date`, `notes`
  (one line pointing to the release page) and `platforms` entries for every key the plugin may look up for a
  universal macOS binary. Verified in `tauri-plugin-updater` 2.14: the plugin looks up `darwin-<arch>-app` (the bundle
  type stamped into the binary; a DMG install reports `app`), then `darwin-<arch>`, where `<arch>` is the slice that
  runs (`aarch64` or `x86_64`); `darwin-universal` is never looked up. The manifest therefore carries
  `darwin-aarch64-app`, `darwin-aarch64`, `darwin-x86_64-app` and `darwin-x86_64`,
  all pointing to `https://github.com/linkineo/mqtt-panel/releases/download/v<version>/MQTT-Panel-macOS.app.tar.gz`
  with the content of the `.sig` file. The release assets are therefore: `MQTT-Panel-macOS.dmg`,
  `MQTT-Panel-macOS.dmg.sha256`, `MQTT-Panel-macOS.app.tar.gz`, `MQTT-Panel-macOS.app.tar.gz.sig`, `latest.json`.
- The plugin compares versions itself (semver, newer only); the endpoint has no logic. The endpoint URL placeholders
  the plugin fills are `{{current_version}}`, `{{target}}` (`darwin`), `{{arch}}` (`aarch64`, `x86_64`) and
  `{{bundle_type}}`. `requireSignedVersion` is on: the version in the signature's trusted comment (written by the
  Tauri CLI) must equal the announced version, so a forged `latest.json` cannot push an older signed archive.
- The bundler makes the updater archive from the app bundle only when the `app` target is built, so the release
  script builds `--bundles app,dmg`.
- A bundle that cannot be replaced: the app checks before offering the install (translocated path, or the folder
  holding the bundle refuses a write with anything but "permission denied"; an executable outside a `.app` bundle
  never installs). At install time the plugin reports a read-only location as an I/O error
  (`ReadOnlyFilesystem`) or `TempDirNotOnSameMountPoint`; both show "Move MQTT Panel to Applications first". A
  folder the user may not write to ("permission denied") makes the plugin ask for an administrator password, as macOS
  does when an app is moved to Applications.
- Debug and test builds accept `MQTTPANEL_UPDATE_ENDPOINT` to point the check at a local server (the test lab serves
  a `latest.json` of its own); release builds ignore the variable.

## Signing

- Two signatures: Developer ID for Gatekeeper (unchanged) and the updater's minisign key pair for the archive.
- The key pair is generated once with `cargo tauri signer generate -w ~/.tauri/mqttpanel-updater.key` by the owner
  (the password is typed by the owner, never written down here). The public key goes into `tauri.dmg.conf.json`
  (`plugins.updater.pubkey`); the private key path and password are read by the release script from `.env.local`
  (`TAURI_SIGNING_PRIVATE_KEY_PATH`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; placeholders in `.env.example`). The
  private key is a release secret: lost, no installed copy can update any more and a new DMG is needed; it is kept
  in the owner's backups, never in the repository.

## Privacy

- This is the first network call besides the configured brokers. The request carries the version, the platform
  and the architecture in the URL; no identifier, no cookie, at most one request per day plus manual checks. The
  Worker keeps nothing; GitHub then serves the file and sees the IP address like for any download.
- Written in: Settings → Updates (the ⓘ), Settings → Privacy, the website privacy page (EN/FR/DE), README,
  `docs/DECISIONS.md`, `docs/SECURITY_REVIEW.md` (new network call, signature verification).

## Versions

- One version for every variant, read from `apps/desktop/src-tauri/tauri.conf.json`; every published binary is built
  from the commit of the tag `v<version>`. Store builds may lag behind the tag; the release notes list which
  version each channel carries. 1.0.1 is the first version with the updater (the 1.0.0 DMG was never published).

## Tests

- cargo: the desktop crate builds with and without the feature; `update_status` reports `selfUpdate: false` without it.
- Vitest: Settings → Updates in the four states (store build, DMG idle, update available, installing); the toast.
- Test lab: `tools/testlab/update_server.py` serves a `latest.json` and an archive signed with a throwaway key for
  a local end-to-end run (build with the test public key, point `MQTTPANEL_UPDATE_ENDPOINT` at it, install, relaunch).
  Debug app bundles only, since release builds ignore the variable:

  ```
  cargo tauri signer generate --ci -p "" -w tools/testlab/out/update/test.key
  python3 tools/testlab/update_server.py overlay --pubkey tools/testlab/out/update/test.key.pub
  export TAURI_SIGNING_PRIVATE_KEY=tools/testlab/out/update/test.key TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
  cargo tauri build --debug --bundles app --features self-update --config tools/testlab/out/update/tauri.updatetest.conf.json
  ditto "target/debug/bundle/macos/MQTT Panel.app" "tools/testlab/out/update/installed/MQTT Panel.app"
  cargo tauri build --debug --bundles app --features self-update --config tools/testlab/out/update/tauri.updatetest.conf.json --config '{"version":"1.0.2"}'
  python3 tools/testlab/update_server.py serve --version 1.0.2 --archive "target/debug/bundle/macos/MQTT Panel.app.tar.gz"
  MQTTPANEL_UPDATE_ENDPOINT='http://127.0.0.1:8765/update/{{target}}/{{arch}}/{{current_version}}' \
    "tools/testlab/out/update/installed/MQTT Panel.app/Contents/MacOS/MQTT Panel"
  ```

  Then Settings → Updates → Check now, Download and install, Restart now: About shows 1.0.2. `--mode none`, `fail`
  and `bad-signature` give "You have the latest version", "Could not reach mqtt-panel.com." and "The download is not
  signed by MQTT Panel. Nothing was installed."; the server prints each request path (platform, architecture,
  version, nothing else). Test ids U-01 to U-11 in `docs/TESTPLAN.md`.
- Release gate: the published DMG, copied to Applications, finds and installs the next version (checked once with a
  throwaway 1.0.2 build before 1.0.1 is published).

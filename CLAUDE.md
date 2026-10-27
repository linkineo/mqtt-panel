# MQTT Panel: project instructions

MQTT Panel is built with AI assistance. These rules apply to every session working in this repository, human or AI,
whichever model runs it.

## Roles
- Specification and review: specifications, architecture and reviews (`docs/SPEC.md`, `docs/FEATURES.md`, `docs/TESTPLAN.md`, `docs/MCP.md`, the mockups).
- Implementation: code and tests built against those documents and the validated mockups in `docs/mockups/`.
- Do not use the `ios-app-workflow` skill in this repository; it belongs to a different product line.

## Brand and public material
- The official website is **https://mqtt-panel.com**. The app and the website reference the website only: copyright lines read "© 2026 mqtt-panel.com".
- No company or other brand name appears anywhere in the repository. The app mark is a decorative icon in the title bar (`aria-hidden`, no title, no text naming it). The store-bound bundle identifier `com.remora-labs.mqttpanel` is the one exception and never changes.
- Never write personal names, personal e-mail addresses, employer names or local file paths anywhere in the repository.
- One exception: the copyright holder's name appears only in the copyright line of `LICENSE` and `NOTICE`, nowhere else.
- The public repository is https://github.com/linkineo/mqtt-panel; its URL may be written in files (README, docs, website, the app's Settings).
- The predecessor tool that inspired this project is **never named** in this repository, in commit messages, release notes, the website or app strings. Write "the predecessor tool" when a reference is unavoidable.
- The website (`website/`) runs its own check in `build.py` on the generated HTML (the same patterns, the former brand pattern included); a hit fails the build.

## Languages
- English is the source language. French and German ship in v1.0.
- French uses « vous » and French typography (espaces insécables, « guillemets »). German uses "du" everywhere.
- Protocol terms stay in English where practitioners use them (QoS, retain, topic, broker, client ID). Everything else is written natively, not translated literally, and reviewed by a native reader before release.
- Demo and screenshot data is localised per language.

## Product rules
- No telemetry, no crash reporting, no accounts, no network calls except to configured brokers and the update check of the direct macOS download. No updater in store builds; the direct macOS download checks mqtt-panel.com for updates (see `docs/UPDATER.md`).
- Distribution: Mac App Store and Microsoft Store, plus a signed and notarized universal DMG for macOS attached to the GitHub release (`scripts/release-mac-dmg.sh`, asset name `MQTT-Panel-macOS.dmg`). No Windows installer outside the store, no Linux packages; GitHub is otherwise for building from source.
- Secrets (passwords, private keys, PKCS#12 passphrases) live in the encrypted secrets file whose master key sits in the OS secret store on signed builds and in a key file readable by the user only on unsigned builds; never in profile files or the repository. A signed release must never show a keychain dialog to an end user (release gate, `docs/SPEC.md` 11).
- Users never type file paths: every file field has a Browse button (native dialog) and accepts drag-and-drop.
- Every security option has an info bubble in plain words, in EN, FR and DE.
- The UI must match the validated mockups in `docs/mockups/`. Deviations go through a spec change first.

## Dependencies
- Project licence: Apache 2.0 with the Commons Clause (`LICENSE`). Never call MQTT Panel "open source"; it is source-available.
- Allowed licences: MIT, Apache 2.0, BSD, ISC, MPL 2.0. No GPL, LGPL-static or AGPL dependencies.
- `cargo deny` and `npm audit` run in CI. Lockfiles are committed.

## Commits
- Commit identity: the git identity configured in this clone (`git config user.name` / `user.email`); never pass `-c user.name=…`. That name in git metadata is the one allowed exception to the no-personal-names rule; it still never appears in files.
- Commit messages end with the line `Co-Authored-By: Claude <model name> <noreply@anthropic.com>` using the model that made the change.
- Before committing: run the tests.

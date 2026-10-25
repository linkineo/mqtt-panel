# MQTT Panel: project instructions

These rules apply to every session working in this repository, whichever model runs it.

## Roles
- Specifications, architecture and reviews are written by the spec model (Fable). Implementation is done by the implementing model (Opus), against `docs/SPEC.md`, `docs/FEATURES.md` and the validated mockups in `docs/mockups/`.
- Do not use the `ios-app-workflow` skill in this repository. It is for a different product line and the owner has disabled it here.

## Brand and public material
- The official website is **https://mqtt-panel.com** (owner, 18 Oct 2026). The app and the website reference the website only: copyright lines read "© 2026 mqtt-panel.com".
- The words "mqtt-panel.com" appear nowhere user-facing (copyright, About, Settings, website, store texts, README, NOTICE). The app mark stays as a decorative icon in the app's title bar (`aria-hidden`, no title, no text naming it). Historical entries in `docs/DECISIONS.md` and internal identifiers such as the bundle ID `com.remora-labs.mqttpanel` stay as they are.
- Never write personal names, personal e-mail addresses, employer names or local file paths anywhere in the repository.
- One exception (owner, 25 Oct 2026): the owner's name appears only in the copyright line of `LICENSE` and `NOTICE`, nowhere else. `scripts/check-forbidden-words.sh` skips those two files for the personal-name patterns only.
- The public repository is https://github.com/linkineo/mqtt-panel; its URL may be written in files (README, docs, website, the app's Settings). The forbidden-words patterns about the e-mail address stay.
- The predecessor tool that inspired this project is **never named** in this repository, in commit messages, release notes, the website or app strings. Write "the predecessor tool" when a reference is unavoidable.
- Run `scripts/check-forbidden-words.sh` before every commit. A hit blocks the commit.
- The website (`website/`) runs its own check in `build.py` on the generated HTML (the same patterns plus "mqtt-panel.com"); a hit fails the build.

## Languages
- English is the source language. French and German ship in v1.0.
- French uses « vous » and French typography (espaces insécables, « guillemets »). German uses "du" everywhere.
- Protocol terms stay in English where practitioners use them (QoS, retain, topic, broker, client ID). Everything else is written natively, not translated literally, and reviewed by a native reader before release.
- Demo and screenshot data is localised per language.

## Product rules
- No telemetry, no crash reporting, no accounts, no network calls except to configured brokers. No updater: the Mac App Store and the Microsoft Store update the app.
- Distribution: Mac App Store and Microsoft Store only. No downloadable installers, no Linux packages; GitHub is for building from source.
- Secrets (passwords, private keys, PKCS#12 passphrases) live in the encrypted secrets file whose master key sits in the OS secret store on signed builds and in an owner-only key file on unsigned builds (change set 10); never in profile files or the repository. A signed release must never show a keychain dialog to an end user (release gate, `docs/SPEC.md` 11).
- Users never type file paths: every file field has a Browse button (native dialog) and accepts drag-and-drop.
- Every security option has an info bubble in plain words, in EN, FR and DE.
- The UI must match the validated mockups in `docs/mockups/`. Deviations go through a spec change first.

## Dependencies
- Project licence: Apache 2.0 with the Commons Clause (`LICENSE`). Never call MQTT Panel "open source"; it is source-available.
- Allowed licences: MIT, Apache 2.0, BSD, ISC, MPL 2.0. No GPL, LGPL-static or AGPL dependencies.
- `cargo deny` and `npm audit` run in CI. Lockfiles are committed.

## Commits
- Commit identity: the git identity configured in this clone by the owner (`git config user.name` / `user.email`); never pass `-c user.name=…`. The owner's own name in git metadata is the one allowed exception to the no-personal-names rule; it still never appears in files.
- Commit messages end with the line `Co-Authored-By: Claude <model name> <noreply@anthropic.com>` using the model that made the change.
- Before committing: `scripts/check-forbidden-words.sh`, then tests.

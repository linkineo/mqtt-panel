# MQTT Panel: project instructions

These rules apply to every session working in this repository, whichever model runs it.

## Roles
- Specifications, architecture and reviews are written by the spec model (Fable). Implementation is done by the implementing model (Opus), against `docs/SPEC.md`, `docs/FEATURES.md` and the validated mockups in `docs/mockups/`.
- Do not use the `ios-app-workflow` skill in this repository. It is for a different product line and the owner has disabled it here.

## Brand and public material
- The brand is **mqtt-panel.com** (mqtt-panel.com). Never write personal names, personal e-mail addresses, employer names or local file paths anywhere in the repository.
- The predecessor tool that inspired this project is **never named** in this repository, in commit messages, release notes, the website or app strings. Write "the predecessor tool" when a reference is unavoidable.
- Run `scripts/check-forbidden-words.sh` before every commit. A hit blocks the commit.

## Languages
- English is the source language. French and German ship in v1.0.
- French uses « vous » and French typography (espaces insécables, « guillemets »). German uses "du" everywhere.
- Protocol terms stay in English where practitioners use them (QoS, retain, topic, broker, client ID). Everything else is written natively, not translated literally, and reviewed by a native reader before release.
- Demo and screenshot data is localised per language.

## Product rules
- No telemetry, no crash reporting, no accounts, no network calls except to configured brokers and, if enabled, the update feed.
- Secrets (passwords, private keys, PKCS#12 passphrases) live in the OS keychain, never in profile files or the repository.
- Users never type file paths: every file field has a Browse button (native dialog) and accepts drag-and-drop.
- Every security option has an info bubble in plain words, in EN, FR and DE.
- The UI must match the validated mockups in `docs/mockups/`. Deviations go through a spec change first.

## Dependencies
- Allowed licences: MIT, Apache 2.0, BSD, ISC, MPL 2.0. No GPL, LGPL-static or AGPL dependencies.
- `cargo deny` and `npm audit` run in CI. Lockfiles are committed.

## Commits
- Commit messages end with the line `Co-Authored-By: Claude <model name> <noreply@anthropic.com>` using the model that made the change.
- Before committing: `scripts/check-forbidden-words.sh`, then tests.

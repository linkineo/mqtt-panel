#!/usr/bin/env bash
# Builds the direct-download macOS variant of MQTT Panel: a universal (Apple silicon + Intel) app signed with a
# Developer ID identity under the hardened runtime, notarized with the App Store Connect key, stapled and wrapped in
# a DMG that is notarized and stapled as well. Settings come from .env.local (see .env.example); the key stays in
# ~/.appstoreconnect/private_keys and the signing identity in the login keychain. Nothing here is printed or copied
# into the repository. The result goes to the GitHub release; the Mac App Store variant has its own script.
# This variant alone carries the updater (docs/UPDATER.md): it is built with the cargo feature self-update and the
# overlay tauri.dmg.conf.json, the updater archive is signed with the minisign key from .env.local, and latest.json is
# written next to it. Release assets, all in target/release-dmg/: MQTT-Panel-macOS.dmg, MQTT-Panel-macOS.dmg.sha256,
# MQTT-Panel-macOS.app.tar.gz, MQTT-Panel-macOS.app.tar.gz.sig, latest.json.
#
#   scripts/release-mac-dmg.sh            build, sign, notarize, verify
#   scripts/release-mac-dmg.sh --arm64    Apple silicon only (faster, for a local check)
#   scripts/release-mac-dmg.sh --verify <dmg>   only run the checks on an existing DMG
set -euo pipefail
cd "$(dirname "$0")/.."

verify() {   # mounts the DMG and checks signature, hardened runtime, notarization tickets, architectures and Gatekeeper
  local dmg="$1" mnt
  mnt=$(mktemp -d)
  hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mnt" >/dev/null 2>&1
  trap 'hdiutil detach "$mnt" -quiet || true' RETURN
  local app="$mnt/MQTT Panel.app"
  codesign --verify --deep --strict --verbose=2 "$app"
  local info; info=$(codesign -d -vv "$app" 2>&1)   # captured once: grep -q closing a pipe would trip pipefail
  grep -q "^Authority=Developer ID Application" <<<"$info" || { echo "not signed with Developer ID"; return 1; }
  grep -q "flags=0x10000(runtime)" <<<"$info" || { echo "hardened runtime missing"; return 1; }
  echo "architectures: $(lipo -archs "$app/Contents/MacOS/MQTT Panel")"
  xcrun stapler validate "$app"
  xcrun stapler validate "$dmg"
  local gk; gk=$(spctl --assess --type execute -vv "$app" 2>&1)
  grep -q "source=Notarized Developer ID" <<<"$gk" || { echo "Gatekeeper does not accept the app: $gk"; return 1; }
  gk=$(spctl --assess --type open --context context:primary-signature -vv "$dmg" 2>&1)
  grep -q "source=Notarized Developer ID" <<<"$gk" || { echo "Gatekeeper does not accept the DMG: $gk"; return 1; }
  echo "verified: $dmg"
}
if [ "${1:-}" = "--verify" ]; then verify "$2"; exit; fi
[ -f .env.local ] || { echo "missing .env.local (copy .env.example)"; exit 2; }
set -a; . ./.env.local; set +a
: "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${APPLE_TEAM_ID:?}" "${APPLE_DEVELOPER_ID_IDENTITY:?}"
: "${TAURI_SIGNING_PRIVATE_KEY_PATH:?}" "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD?}"
TAURI_SIGNING_PRIVATE_KEY_PATH="${TAURI_SIGNING_PRIVATE_KEY_PATH/#\~/$HOME}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"

KEY_PATH="${ASC_KEY_PATH:-$HOME/.appstoreconnect/private_keys/AuthKey_$ASC_KEY_ID.p8}"
[ -f "$KEY_PATH" ] || { echo "App Store Connect key not found: $KEY_PATH"; exit 2; }
security find-identity -v -p codesigning | grep -q "$APPLE_DEVELOPER_ID_IDENTITY" \
  || { echo "signing identity not in the keychain: $APPLE_DEVELOPER_ID_IDENTITY"; exit 2; }

SRC=apps/desktop/src-tauri
DMG_CONF="$SRC/tauri.dmg.conf.json"
# The updater public key is inserted by the owner; a build with the placeholder could never update.
grep -q "UPDATER_PUBKEY_PLACEHOLDER" "$DMG_CONF" \
  && { echo "$DMG_CONF still holds UPDATER_PUBKEY_PLACEHOLDER: insert the updater public key first (docs/UPDATER.md)"; exit 2; }
[ -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" ] || { echo "updater signing key not found: $TAURI_SIGNING_PRIVATE_KEY_PATH"; exit 2; }
VERSION=$(python3 -c "import json;print(json.load(open('$SRC/tauri.conf.json'))['version'])")
if [ "${1:-}" = "--arm64" ]; then TARGET=aarch64-apple-darwin; SUFFIX=aarch64; else TARGET=universal-apple-darwin; SUFFIX=universal; fi
echo "MQTT Panel $VERSION: building the Developer ID variant ($TARGET)"

# The bundler signs with the identity, notarizes the app with the key (notarytool --wait) and staples the ticket
# before it builds the DMG.
export APPLE_SIGNING_IDENTITY="$APPLE_DEVELOPER_ID_IDENTITY"
export APPLE_API_KEY="$ASC_KEY_ID" APPLE_API_ISSUER="$ASC_ISSUER_ID" APPLE_API_KEY_PATH="$KEY_PATH"
# The CLI signs the updater archive with TAURI_SIGNING_PRIVATE_KEY (a key string or the path of the key file).
# The path must be absolute: the CLI resolves it from the Tauri directory, and treats a path it cannot find as the key
# text itself.
export TAURI_SIGNING_PRIVATE_KEY="$(cd "$(dirname "$TAURI_SIGNING_PRIVATE_KEY_PATH")" && pwd)/$(basename "$TAURI_SIGNING_PRIVATE_KEY_PATH")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD
# "app" as well as "dmg": the bundler makes the updater archive from the app bundle only when the app target is built.
cargo tauri build --target "$TARGET" --bundles app,dmg --features self-update --config "$DMG_CONF"

BUNDLE="target/$TARGET/release/bundle"
APP="$BUNDLE/macos/MQTT Panel.app"
DMG_SRC="$BUNDLE/dmg/MQTT Panel_${VERSION}_$SUFFIX.dmg"
ARCHIVE_SRC="$BUNDLE/macos/MQTT Panel.app.tar.gz"
OUT_DIR=target/release-dmg
ASSET=MQTT-Panel-macOS   # fixed asset names: the download links and the updater point at releases/latest/download
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR/$ASSET.dmg" "$OUT_DIR/$ASSET.dmg.sha256" "$OUT_DIR/$ASSET.app.tar.gz" "$OUT_DIR/$ASSET.app.tar.gz.sig" "$OUT_DIR/latest.json"
OUT="$OUT_DIR/$ASSET.dmg"
cp "$DMG_SRC" "$OUT"
[ -f "$ARCHIVE_SRC" ] && [ -f "$ARCHIVE_SRC.sig" ] || { echo "updater archive or signature missing: $ARCHIVE_SRC"; exit 1; }
cp "$ARCHIVE_SRC" "$OUT_DIR/$ASSET.app.tar.gz"
cp "$ARCHIVE_SRC.sig" "$OUT_DIR/$ASSET.app.tar.gz.sig"
tar -tzf "$OUT_DIR/$ASSET.app.tar.gz" | grep -q "^MQTT Panel.app/Contents/MacOS/MQTT Panel$" \
  || { echo "the updater archive does not hold MQTT Panel.app"; exit 1; }

echo "notarizing the DMG"
xcrun notarytool submit "$OUT" --key "$KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" --wait
xcrun stapler staple "$OUT"

verify "$OUT"
shasum -a 256 "$OUT" | sed "s#$OUT_DIR/##" > "$OUT.sha256"

# latest.json for the updater (docs/UPDATER.md). The plugin looks up darwin-<arch>-app, then darwin-<arch>, where
# <arch> is the slice that runs (aarch64 or x86_64 in the universal binary); every key points to the same archive.
python3 - "$OUT_DIR" "$ASSET" "$VERSION" "$SUFFIX" <<'PY'
import datetime, json, sys
out, asset, version, suffix = sys.argv[1:5]
signature = open(f"{out}/{asset}.app.tar.gz.sig").read().strip()
url = f"https://github.com/linkineo/mqtt-panel/releases/download/v{version}/{asset}.app.tar.gz"
archs = ["aarch64", "x86_64"] if suffix == "universal" else ["aarch64"]
platforms = {f"darwin-{a}{kind}": {"signature": signature, "url": url} for a in archs for kind in ("-app", "")}
manifest = {
    "version": version,
    "notes": f"Release notes: https://github.com/linkineo/mqtt-panel/releases/tag/v{version}",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": platforms,
}
with open(f"{out}/latest.json", "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY

echo "release assets in $OUT_DIR:"
ls -l "$OUT_DIR/$ASSET.dmg" "$OUT_DIR/$ASSET.dmg.sha256" "$OUT_DIR/$ASSET.app.tar.gz" "$OUT_DIR/$ASSET.app.tar.gz.sig" "$OUT_DIR/latest.json"
cat "$OUT.sha256"

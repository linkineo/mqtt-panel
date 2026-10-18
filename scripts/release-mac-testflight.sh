#!/usr/bin/env bash
# Builds the sandboxed Mac App Store variant of MQTT Panel, signs it, wraps it in a signed installer package and
# uploads it to App Store Connect for TestFlight. Settings come from .env.local (see .env.example); the App Store
# Connect key stays in ~/.appstoreconnect/private_keys and the signing keys in the login keychain. Nothing here is
# printed or copied into the repository.
#
#   scripts/release-mac-testflight.sh            build, package and upload
#   scripts/release-mac-testflight.sh --no-upload build and package only
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f .env.local ] || { echo "missing .env.local (copy .env.example)"; exit 2; }
set -a; . ./.env.local; set +a
: "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${APPLE_TEAM_ID:?}" "${APPLE_SIGNING_IDENTITY:?}" "${APPLE_INSTALLER_IDENTITY:?}" "${MQTTPANEL_PROVISIONING_PROFILE:?}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
scripts/check-forbidden-words.sh

SRC=apps/desktop/src-tauri
PROFILE="${MQTTPANEL_PROVISIONING_PROFILE/#\~/$HOME}"
[ -f "$PROFILE" ] || { echo "provisioning profile not found: $PROFILE"; exit 2; }
mkdir -p "$SRC/signing"
cp "$PROFILE" "$SRC/signing/embedded.provisionprofile"
chmod 644 "$SRC/signing/embedded.provisionprofile"   # the bundle must stay readable by every user (App Store check 90255)
sed "s/\$(TEAM_ID)/$APPLE_TEAM_ID/g" "$SRC/Entitlements.mas.plist" > "$SRC/signing/Entitlements.plist"

VERSION=$(python3 -c "import json;print(json.load(open('$SRC/tauri.conf.json'))['version'])")
BUILD=$(date -u +%Y%m%d%H%M)   # CFBundleVersion: unique and increasing per upload
echo "MQTT Panel $VERSION ($BUILD): building the Mac App Store variant"

# The overlay adds the sandbox entitlements and the embedded profile; the inline config sets the build number.
export APPLE_SIGNING_IDENTITY
cargo tauri build --bundles app \
  --config "$SRC/tauri.mas.conf.json" \
  --config "{\"bundle\":{\"macOS\":{\"bundleVersion\":\"$BUILD\"}}}"

APP="target/release/bundle/macos/MQTT Panel.app"
chmod -R u+rwX,go+rX "$APP"   # file modes are not part of the signature; every file must be readable by every user
codesign --verify --deep --strict "$APP"
codesign -d --entitlements :- "$APP" 2>/dev/null | grep -q "com.apple.security.app-sandbox" || { echo "sandbox entitlement missing"; exit 1; }
OUT="target/release/bundle/macos/MQTT Panel $VERSION-$BUILD.pkg"
productbuild --component "$APP" /Applications --sign "$APPLE_INSTALLER_IDENTITY" "$OUT"
echo "package: $OUT"

if [ "${1:-}" = "--no-upload" ]; then exit 0; fi
xcrun altool --upload-app -f "$OUT" -t macos --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
echo "uploaded $VERSION ($BUILD); App Store Connect processes it in a few minutes, then TestFlight lists it."

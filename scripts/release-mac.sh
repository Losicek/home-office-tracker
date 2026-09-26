#!/bin/bash
# Sestaví podepsanou a notarizovanou univerzální (Apple Silicon + Intel) verzi
# pro macOS a zkopíruje .dmg do „HomeOfficeTracker Instalace“.
#
# Jednorázová příprava (viz README → Podepisování):
#   1. certifikát „Developer ID Application“ v klíčence (Xcode → Settings →
#      Accounts → Manage Certificates → + → Developer ID Application)
#   2. xcrun notarytool store-credentials "homeoffice-notary" \
#        --apple-id <apple-id> --team-id W3LT8G9294
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"

TEAM_ID="${APPLE_TEAM_ID:-W3LT8G9294}"
NOTARY_PROFILE="${NOTARY_PROFILE:-homeoffice-notary}"
# Podle otisku (SHA-1), ne jména: stejný certifikát bývá v klíčence víckrát
# a codesign pak hlásí „ambiguous“.
CERT_LINE=$(security find-identity -v -p codesigning \
  | grep "Developer ID Application: .*($TEAM_ID)" | head -1)
IDENTITY=$(echo "$CERT_LINE" | awk '{print $2}')
if [ -z "$IDENTITY" ]; then
  echo "Chybí certifikát „Developer ID Application“ pro tým $TEAM_ID." >&2
  exit 1
fi
echo "Podpis: $(echo "$CERT_LINE" | cut -d'"' -f2) [$IDENTITY]"

VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
export APPLE_SIGNING_IDENTITY="$IDENTITY"
npm run tauri build -- --target universal-apple-darwin

BUNDLE=src-tauri/target.nosync/universal-apple-darwin/release/bundle
APP="$BUNDLE/macos/Home Office Tracker.app"
DMG=$(ls "$BUNDLE"/dmg/*.dmg | head -1)

codesign --verify --deep --strict "$APP"
codesign --force --sign "$IDENTITY" --timestamp "$DMG"

echo "Notarizace (trvá obvykle pár minut)…"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"
spctl --assess --type open --context context:primary-signature -v "$DMG"

OUT="$(cd .. && pwd)/HomeOfficeTracker Instalace"
mkdir -p "$OUT"
cp "$DMG" "$OUT/Home Office Tracker $VERSION - Mac.dmg"
echo "Hotovo: $OUT/Home Office Tracker $VERSION - Mac.dmg"

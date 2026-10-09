#!/bin/bash
set -euo pipefail
project=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project"
: "${CRAFT_SIGN_IDENTITY:?Developer ID identity required}"
: "${APPSTORE_API_KEY_FILE:?Notarization API key file required}"
: "${APPSTORE_API_KEY_ID:?Notarization API key ID required}"
: "${APPSTORE_API_ISSUER_ID:?Notarization issuer ID required}"
: "${SPARKLE_PRIVATE_KEY_FILE:?Sparkle private key file required}"
: "${CRAFT_RELEASE_TAG:?Release tag required}"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
[[ "$CRAFT_RELEASE_TAG" == "v$version" ]] || { echo 'Release tag must match Cargo.toml version' >&2; exit 1; }
[[ "$CRAFT_SIGN_IDENTITY" != '-' ]] || { echo 'Release builds require Developer ID signing' >&2; exit 1; }
CRAFT_MANAGER_ARCH=universal ./scripts/package-macos.sh
bundle="$project/dist/macos/Craft Library.app"
dmg="$project/dist/macos/Craft-Library-$version-macos-universal.dmg"
notarize() {
    xcrun notarytool submit "$1" --key "$APPSTORE_API_KEY_FILE" --key-id "$APPSTORE_API_KEY_ID" --issuer "$APPSTORE_API_ISSUER_ID" --wait --output-format json > "$2"
    [[ "$(plutil -extract status raw -o - "$2")" == Accepted ]] || { echo "Apple did not accept notarization of $(basename "$1")" >&2; exit 1; }
}
# Notarize and staple the app first so it also opens offline once copied out of the DMG.
rm "$dmg"
upload="$project/dist/macos/notarize-app.zip"
ditto -c -k --norsrc --noextattr --keepParent "$bundle" "$upload"
notarize "$upload" dist/macos/notarization.json
rm "$upload"
xcrun stapler staple "$bundle"
xcrun stapler validate "$bundle"
codesign --verify --deep --strict "$bundle"
spctl --assess --type execute --verbose=2 "$bundle"
# Then wrap the stapled app in a signed DMG and notarize and staple the DMG itself.
./scripts/make-dmg.sh "$bundle" "$dmg"
notarize "$dmg" dist/macos/notarization-dmg.json
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
repository=${CRAFT_MANAGER_REPOSITORY:-henrikogaard/craft-apps-manager}
vendor/Sparkle/bin/generate_appcast --ed-key-file "$SPARKLE_PRIVATE_KEY_FILE" --download-url-prefix "https://github.com/$repository/releases/download/$CRAFT_RELEASE_TAG/" --maximum-deltas 0 dist/macos
(cd dist/macos && shasum -a 256 "$(basename "$dmg")" > SHA256SUMS)

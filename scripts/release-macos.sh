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
bundle="$project/dist/macos/Craft Apps Manager.app"
archive="$project/dist/macos/Craft-Apps-Manager-$version-macos-universal.zip"
xcrun notarytool submit "$archive" --key "$APPSTORE_API_KEY_FILE" --key-id "$APPSTORE_API_KEY_ID" --issuer "$APPSTORE_API_ISSUER_ID" --wait --output-format json > dist/macos/notarization.json
[[ "$(plutil -extract status raw -o - dist/macos/notarization.json)" == Accepted ]] || { echo 'Apple did not accept notarization' >&2; exit 1; }
xcrun stapler staple "$bundle"
xcrun stapler validate "$bundle"
codesign --verify --deep --strict "$bundle"
spctl --assess --type execute --verbose=2 "$bundle"
rm "$archive"
ditto -c -k --norsrc --noextattr --keepParent "$bundle" "$archive"
repository=${CRAFT_MANAGER_REPOSITORY:-henrikogaard/craft-apps-manager}
vendor/Sparkle/bin/generate_appcast --ed-key-file "$SPARKLE_PRIVATE_KEY_FILE" --download-url-prefix "https://github.com/$repository/releases/download/$CRAFT_RELEASE_TAG/" --maximum-deltas 0 dist/macos
(cd dist/macos && shasum -a 256 "$(basename "$archive")" > SHA256SUMS)

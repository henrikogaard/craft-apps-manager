#!/bin/sh
# Wraps an app bundle in a compressed DMG with an Applications shortcut for drag-to-install.
# Usage: make-dmg.sh <bundle.app> <output.dmg>
# Signs the DMG when CRAFT_SIGN_IDENTITY names a real identity (not ad-hoc "-").
set -eu
bundle=$1
dmg=$2
[ -d "$bundle" ] || { echo "Missing app bundle: $bundle" >&2; exit 1; }
[ ! -e "$dmg" ] || { echo "Package already exists: $dmg" >&2; exit 1; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
ditto --noextattr --norsrc "$bundle" "$stage/$(basename "$bundle")"
ln -s /Applications "$stage/Applications"
# hdiutil occasionally reports "Resource busy" on CI; retry a few times.
attempt=1
until hdiutil create -quiet -volname "Craft Library" -srcfolder "$stage" -fs HFS+ -format UDZO -imagekey zlib-level=9 "$dmg"; do
    [ "$attempt" -lt 3 ] || { echo "Could not create $dmg" >&2; exit 1; }
    attempt=$((attempt + 1))
    sleep 5
done
identity=${CRAFT_SIGN_IDENTITY:--}
if [ "$identity" != - ]; then
    codesign --force --timestamp --sign "$identity" "$dmg"
    codesign --verify --strict "$dmg"
fi
hdiutil verify -quiet "$dmg"

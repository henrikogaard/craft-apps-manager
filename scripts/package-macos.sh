#!/bin/sh
# Builds a universal (Apple silicon + Intel) Craft Library.app and a DMG to install it.
# Set CRAFT_MANAGER_REPOSITORY=owner/name to self-update from a fork's releases.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-"$project/dist/macos"}
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$project/Cargo.toml" | head -1)
case "$version" in ''|*[!0-9.]*) echo "Invalid package version" >&2; exit 1;; esac
cd "$project"
./scripts/fetch-sparkle.sh
arch=${CRAFT_MANAGER_ARCH:-universal}
case "$arch" in
    universal) targets="aarch64-apple-darwin x86_64-apple-darwin" ;;
    arm64) targets="aarch64-apple-darwin" ;;
    x64) targets="x86_64-apple-darwin" ;;
    *) echo "CRAFT_MANAGER_ARCH must be universal, arm64 or x64" >&2; exit 1 ;;
esac
public_key=${SPARKLE_PUBLIC_ED_KEY:-$(plutil -extract publicKey raw -o - config/sparkle.json)}
feed_url=${SPARKLE_FEED_URL:-$(plutil -extract feedURL raw -o - config/sparkle.json)}
for target in $targets; do
    cargo build --release --locked --target "$target"
done
mkdir -p "$output"
output=$(CDPATH= cd -- "$output" && pwd)
bundle="$output/Craft Library.app"
dmg="$output/Craft-Library-$version-macos-$arch.dmg"
[ ! -e "$dmg" ] || { echo "Package already exists: $dmg" >&2; exit 1; }
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
if [ "$arch" = universal ]; then
    lipo -create -output "$bundle/Contents/MacOS/craft-apps-manager" \
        target/aarch64-apple-darwin/release/craft-apps-manager \
        target/x86_64-apple-darwin/release/craft-apps-manager
else
    cp "target/$targets/release/craft-apps-manager" "$bundle/Contents/MacOS/craft-apps-manager"
fi
iconset=$(mktemp -d)/icon.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/icon.icns"
rm -rf "$(dirname "$iconset")"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Craft Library</string>
  <key>CFBundleDisplayName</key><string>Craft Library</string>
  <key>NSHumanReadableCopyright</key><string>© 2026 Henrik Øgård; original project CryptoKey98 and Craft Apps Updater contributors</string>
  <key>CFBundleIdentifier</key><string>io.github.craft-apps-manager</string>
  <key>CFBundleExecutable</key><string>craft-apps-manager</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>SUFeedURL</key><string>$feed_url</string>
  <key>SUPublicEDKey</key><string>$public_key</string>
  <key>SUEnableAutomaticChecks</key><true/>
  <key>SUAutomaticallyUpdate</key><true/>
  <key>SUScheduledCheckInterval</key><integer>86400</integer>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$bundle/Contents/Info.plist"
for name in README.md CHANGELOG.md LICENSE THIRD-PARTY-NOTICES.txt; do
    cp "$name" "$bundle/Contents/Resources/"
done
mkdir -p "$bundle/Contents/Resources/licenses/app-icons"
cp assets/app-icons/*.txt "$bundle/Contents/Resources/licenses/app-icons/"
mkdir -p "$bundle/Contents/Resources/licenses/fonts"
cp assets/fonts/*-OFL.txt "$bundle/Contents/Resources/licenses/fonts/"
mkdir -p "$bundle/Contents/Frameworks" "$bundle/Contents/Resources/licenses/sparkle"
ditto --noextattr --norsrc vendor/Sparkle/Sparkle.framework "$bundle/Contents/Frameworks/Sparkle.framework"
cp vendor/Sparkle/LICENSE "$bundle/Contents/Resources/licenses/sparkle/LICENSE"
python3 scripts/sign-bundle.py "$bundle" "${CRAFT_SIGN_IDENTITY:--}"
./scripts/make-dmg.sh "$bundle" "$dmg"
printf '%s\n' "$bundle" "$dmg"

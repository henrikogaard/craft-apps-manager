#!/bin/sh
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
config="$project/config/sparkle.json"
expected=$(plutil -extract sha256 raw -o - "$config")
destination="$project/vendor/Sparkle"
if [ -f "$destination/.download-sha256" ] && [ "$(cat "$destination/.download-sha256")" = "$expected" ]; then exit 0; fi
archive=$(mktemp -t craft-sparkle)
trap 'rm -f "$archive"' EXIT HUP INT TERM
curl --fail --location --silent --show-error "$(plutil -extract url raw -o - "$config")" -o "$archive"
actual=$(shasum -a 256 "$archive" | cut -d ' ' -f 1)
[ "$actual" = "$expected" ] || { echo 'Sparkle download checksum mismatch' >&2; exit 1; }
mkdir -p "$destination"
tar -xJf "$archive" -C "$destination"
printf '%s' "$expected" > "$destination/.download-sha256"

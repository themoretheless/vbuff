#!/bin/sh
set -eu

binary="${1:?usage: package-macos.sh <binary> <app-bundle>}"
bundle="${2:?usage: package-macos.sh <binary> <app-bundle>}"
contents="$bundle/Contents"

rm -rf "$bundle"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$binary" "$contents/MacOS/vbuff"
chmod 755 "$contents/MacOS/vbuff"
cp packaging/macos/Info.plist "$contents/Info.plist"
plutil -lint "$contents/Info.plist"

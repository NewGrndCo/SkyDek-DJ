#!/usr/bin/env bash
set -euo pipefail

dmg="$(find "${1:?Pass the directory containing the DMG}" -name '*.dmg' -print -quit)"
test -n "$dmg"
mount_dir="$(mktemp -d)"
trap 'hdiutil detach "$mount_dir"; rmdir "$mount_dir"' EXIT
hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg"
app="$(find "$mount_dir" -maxdepth 1 -name '*.app' -print -quit)"
test -n "$app"
encoder="$app/Contents/Resources/resources/encoder/skydek-mp3-encoder"
test -x "$encoder"
test -f "$app/Contents/Resources/project.icns"
test -n "$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$app/Contents/Info.plist")"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleDocumentTypes:0:CFBundleTypeIconFile' "$app/Contents/Info.plist")" = project.icns
lipo "$app/Contents/MacOS/skydek-dj" -verify_arch arm64 x86_64
lipo "$encoder" -verify_arch arm64 x86_64
codesign --verify --deep "$app"
echo "Verified universal macOS DMG: $dmg"

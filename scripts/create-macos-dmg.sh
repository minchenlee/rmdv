#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <rmdv.app> <output.dmg> <background.png>" >&2
  exit 2
fi

APP="$1"
OUTPUT="$2"
BACKGROUND="$3"

if [ ! -d "$APP" ] || [[ "$APP" != *.app ]]; then
  echo "Expected an existing .app bundle: $APP" >&2
  exit 1
fi
if [ ! -f "$BACKGROUND" ]; then
  echo "DMG background image not found: $BACKGROUND" >&2
  exit 1
fi
if ! command -v hdiutil >/dev/null 2>&1 || ! command -v osascript >/dev/null 2>&1; then
  echo "hdiutil and osascript are required to create a customized macOS DMG" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT")"
WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rmdv-dmg.XXXXXX")
RW_DMG="$WORK_DIR/rmdv-rw.dmg"
DEVICE=""

cleanup() {
  if [ -n "$DEVICE" ]; then
    hdiutil detach "$DEVICE" >/dev/null 2>&1 || hdiutil detach "$DEVICE" -force >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

APP_SIZE_MB=$(du -sm "$APP" | awk 'NR == 1 { print $1 }')
if ! [[ "$APP_SIZE_MB" =~ ^[0-9]+$ ]]; then
  echo "Could not measure the app bundle size" >&2
  exit 1
fi
IMAGE_SIZE_MB=$((APP_SIZE_MB + 32))
if [ "$IMAGE_SIZE_MB" -lt 64 ]; then
  IMAGE_SIZE_MB=64
fi
hdiutil create -size "${IMAGE_SIZE_MB}m" -fs HFS+ -volname rmdv -ov "$RW_DMG" >/dev/null
ATTACH_OUTPUT=$(hdiutil attach "$RW_DMG" -readwrite -noverify -noautoopen)
DEVICE=$(printf '%s\n' "$ATTACH_OUTPUT" | awk '$2 == "Apple_HFS" { print $1; exit }')
VOLUME=$(printf '%s\n' "$ATTACH_OUTPUT" | awk '$2 == "Apple_HFS" { $1 = ""; $2 = ""; sub(/^[[:space:]]+/, ""); print; exit }')
if [ -z "$DEVICE" ] || [ -z "$VOLUME" ]; then
  echo "Could not locate the mounted rmdv volume" >&2
  exit 1
fi
VOLUME_NAME=$(basename "$VOLUME")

mkdir -p "$VOLUME/.background"
ditto --norsrc "$APP" "$VOLUME/rmdv.app"
ln -s /Applications "$VOLUME/Applications"
cp "$BACKGROUND" "$VOLUME/.background/dmg-background.png"
if command -v SetFile >/dev/null 2>&1; then
  SetFile -a V "$VOLUME/.background"
else
  chflags hidden "$VOLUME/.background"
fi

# Finder stores icon positions, view options, and the background in the
# volume's .DS_Store. The temporary read-write image lets us persist those
# settings before converting to the compressed release DMG.
osascript <<APPLESCRIPT
tell application "Finder"
    tell disk "$VOLUME_NAME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set pathbar visible of container window to false
        set bounds of container window to {120, 100, 1320, 820}
        set viewOptions to icon view options of container window
        set arrangement of viewOptions to not arranged
        set icon size of viewOptions to 112
        set text size of viewOptions to 14
        set background picture of viewOptions to file ".background:dmg-background.png"
        set position of item "rmdv.app" of container window to {360, 335}
        set position of item "Applications" of container window to {840, 335}
        close container window
        open
        update without registering applications
        delay 1
        close container window
    end tell
end tell
APPLESCRIPT

sync
hdiutil detach "$DEVICE" >/dev/null
DEVICE=""
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$OUTPUT" >/dev/null
echo "Created $OUTPUT"

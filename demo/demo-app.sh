# Sourced by record-a.sh and record-b.sh. Wraps the rmdv binary in a
# throwaway .app so LaunchServices can bring its window to the front.
# A bare binary started from a terminal cannot activate itself on macOS 14+
# (cooperative activation), so its window can open behind the terminal or
# on another Space, and `--focus` is refused.
# Needs: RMDV, WORK, HOME, RMDV_SOCKET. Defines APP, launch_app, front_app.
APP="$WORK/rmdv-demo.app"
mkdir -p "$APP/Contents/MacOS"
cp "$RMDV" "$APP/Contents/MacOS/rmdv"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>rmdv</string>
  <key>CFBundleIdentifier</key><string>com.minchenlee.rmdv.demo</string>
  <key>CFBundleName</key><string>rmdv</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

# launch_app <path>: start a new instance with the isolated HOME and socket,
# then wait until its socket answers.
launch_app() {
  open -n -a "$APP" --env HOME="$HOME" --env RMDV_SOCKET="$RMDV_SOCKET" --args "$1"
  for _ in $(seq 1 75); do [[ -S "$RMDV_SOCKET" ]] && return 0; sleep 0.2; done
  echo "rmdv did not start (no socket at $RMDV_SOCKET)" >&2; return 1
}

# front_app: bring the demo window to the front (and to its Space).
front_app() { open -a "$APP"; }

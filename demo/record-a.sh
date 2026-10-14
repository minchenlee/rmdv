#!/usr/bin/env bash
# rmdv demo A (README GIF, ~15 s). Start the screen recording, then run this.
# Uses an isolated HOME and socket; never touches your own rmdv or prefs.
# Needs: cliclick + Accessibility for your terminal (for the ⌘⇧M keystroke).
# Usage: ./demo/record-a.sh [path/to/rmdv]     (default: target/release/rmdv)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RMDV="${1:-$REPO/target/release/rmdv}"
[[ -x "$RMDV" ]] || { echo "binary not found: $RMDV" >&2; exit 1; }
command -v cliclick >/dev/null || { echo "need cliclick: brew install cliclick" >&2; exit 1; }

WORK="$(mktemp -d /private/tmp/rmdv-demo-a.XXXXXX)"
export HOME="$WORK/home"; mkdir -p "$HOME"
export RMDV_SOCKET="$WORK/a.sock"
VAULT="$WORK/demo"
mkdir -p "$VAULT"
rsync -a --exclude AGENTS.md --exclude '*.sh' --exclude storyboard "$REPO/demo/" "$VAULT/"
DOC="$VAULT/showcase/ingest-pipeline.md"

r() { "$RMDV" "$@" >/dev/null; }
front() {
  osascript -e 'tell application "System Events" to set frontmost of (first process whose name starts with "rmdv") to true' >/dev/null 2>&1
  sleep 0.6
}
key() { front; cliclick -w 80 "$@"; }
cleanup() { "$RMDV" close >/dev/null 2>&1 || true; }
trap cleanup EXIT

# 0-2 s: open the folder from the CLI; the window appears at once
"$RMDV" "$VAULT" >/dev/null 2>&1 &
for _ in $(seq 1 50); do [[ -S "$RMDV_SOCKET" ]] && break; sleep 0.2; done
sleep 1.5; r resize 1280 800
r open "$VAULT/README.md" --focus
sleep 2
# 2-6 s: the document with the Mermaid diagram and the equation
r open "$DOC" --focus --section "Ingest Pipeline/Architecture"
sleep 2
r goto --section "Ingest Pipeline/Capacity Model"
sleep 2
# 6-9 s: Full Mindmap, zoom, pick a file
r open "$VAULT/README.md" --focus
sleep 0.5
key kd:cmd kd:shift t:m ku:shift ku:cmd
sleep 1.5
key t:=; sleep 0.8; key t:=; sleep 1
key kp:enter
sleep 1.5
# 9-14 s: an agent runs the CLI; the window jumps to a section
r open "$DOC" --focus
sleep 0.8
echo '$ rmdv goto --section "Ingest Pipeline/Failure Handling/Replay"'
r goto --section "Ingest Pipeline/Failure Handling/Replay"
# 14-16 s: hold the result
sleep 2.5

#!/usr/bin/env bash
# rmdv demo A (README GIF, about 15 s). You are present; this script prints
# what to press and waits for Enter. It sends no keystrokes itself.
# Uses an isolated HOME and socket. It never touches your own rmdv or prefs.
# Every rmdv call uses --focus on purpose: the recording needs the window in
# front, and you are at the machine. Run it only when you are ready to record.
# Usage: ./demo/record-a.sh [path/to/rmdv]     (default: target/release/rmdv)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RMDV="${1:-$REPO/target/release/rmdv}"
[[ -x "$RMDV" ]] || { echo "binary not found: $RMDV" >&2; exit 1; }

WORK="$(mktemp -d /private/tmp/rmdv-demo-a.XXXXXX)"
export HOME="$WORK/home"; mkdir -p "$HOME"
export RMDV_SOCKET="$WORK/a.sock"
VAULT="$WORK/demo"
mkdir -p "$VAULT"
rsync -a --exclude AGENTS.md --exclude '*.sh' --exclude storyboard "$REPO/demo/" "$VAULT/"
source "$REPO/demo/demo-app.sh"
DOC="$VAULT/showcase/ingest-pipeline.md"

r() { "$RMDV" "$@" >/dev/null; }
# After each Enter the terminal is in front, so bring rmdv back.
pause() { printf '\n>>> %s\n    Press Enter when done. ' "$1"; read -r _; front_app; }
trap '"$RMDV" close >/dev/null 2>&1 || true' EXIT

echo "Start your screen recording now. The first step opens the window."
printf "Ready to record? Press Enter. "; read -r _

# 0-2 s: open the folder; the window appears at once
launch_app "$VAULT"
sleep 1.5; r resize 1280 800; front_app
r open "$VAULT/README.md" --focus
sleep 2
# 2-6 s: the document with the Mermaid diagram and the equation
r open "$DOC" --focus --section "Ingest Pipeline/Architecture"
sleep 2
r goto --section "Ingest Pipeline/Capacity Model" --focus
sleep 2
# 6-9 s: Full Mindmap has no CLI command, so you press the keys
r open "$VAULT/README.md" --focus
pause "Press ⌘⇧M now (Full Mindmap). Press = twice to zoom. Press Enter in rmdv to open a file."
# 9-14 s: an agent runs the CLI; the window jumps to a section
r open "$DOC" --focus
sleep 0.8
echo '$ rmdv goto --section "Ingest Pipeline/Failure Handling/Replay"'
r goto --section "Ingest Pipeline/Failure Handling/Replay" --focus
# 14-16 s: hold the result
sleep 2.5
echo "Stop the recording."

#!/usr/bin/env bash
# rmdv demo B' (agent scenario). Left: Claude Code. Right: rmdv.
# This script only stages the scene. You record, and you paste the prompt
# into Claude Code yourself, so the agent really runs.
# Usage: ./demo/record-b.sh [path/to/rmdv]
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RMDV="${1:-$REPO/target/release/rmdv}"
[[ -x "$RMDV" ]] || { echo "binary not found: $RMDV" >&2; exit 1; }

WORK="$(mktemp -d /private/tmp/rmdv-demo-b.XXXXXX)"
export HOME="$WORK/home"; mkdir -p "$HOME"
export RMDV_SOCKET="$WORK/b.sock"
SCENE="$WORK/scene"
mkdir -p "$SCENE"
cp "$REPO/demo/showcase/ingest-pipeline.md" "$SCENE/ingest-pipeline.md"

"$RMDV" "$SCENE" >/dev/null 2>&1 &
for _ in $(seq 1 50); do [[ -S "$RMDV_SOCKET" ]] && break; sleep 0.2; done
sleep 1.5
"$RMDV" resize 1280 800 >/dev/null
"$RMDV" open "$SCENE/ingest-pipeline.md" --focus >/dev/null

cat <<MSG
Scene ready in: $SCENE
rmdv shows the file. rmdv reloads a file when it changes on disk (polled every 300 ms).

In another terminal, start Claude Code in that folder with the same socket:
  cd "$SCENE" && RMDV_SOCKET="$RMDV_SOCKET" claude

Prompt to paste (one sentence):
  Add a "Retry Budget" subsection under "Failure Handling" that says each event gets five attempts, then run: rmdv goto --section "Ingest Pipeline/Failure Handling/Retry Budget"

Stop rmdv when done:  RMDV_SOCKET="$RMDV_SOCKET" "$RMDV" close
MSG

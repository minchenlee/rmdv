#!/usr/bin/env bash
# rmdv auto-tour — drives the running app with REAL keystrokes so a keycaster
# (KeyCastr) shows every shortcut. You record the rmdv window with Screen Studio
# while this runs hands-off.
#
# Requires: cliclick (brew install cliclick) + Accessibility granted to the
# terminal host (Zed). rmdv release binary built.
#
# Usage:
#   ./demo/tour.sh                 # full run
#   PACE=1.6 ./demo/tour.sh        # slower (default beat gap)
#   ./demo/tour.sh --dry           # print beats, send no keys
#
# It launches rmdv on demo/ itself. Move/resize the window first if you want,
# then re-run — it reuses the running instance.

set -uo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RMDV="$REPO/target/release/rmdv"
DEMO="$REPO/demo"
PACE="${PACE:-1.6}"        # seconds to hold on each beat
READ="${READ:-3.0}"        # longer hold when there's content to read
DRY=0; [[ "${1:-}" == "--dry" ]] && DRY=1

[[ -x "$RMDV" ]] || { echo "build first: cargo build --release"; exit 1; }
command -v cliclick >/dev/null || { echo "need cliclick: brew install cliclick"; exit 1; }

say()  { printf '\n▶ %s\n' "$*"; }
hold() { sleep "${1:-$PACE}"; }

# bring rmdv to the front so keystrokes land on it (and not on Zed)
front() {
  osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "rmdv") to true' 2>/dev/null
  sleep 0.5
}
# force a known view mode over IPC (recovers if a keystroke ever drops)
viewmode() { [[ $DRY == 0 ]] && "$RMDV" mode view >/dev/null 2>&1; sleep 0.4; }
# stage a file silently in the background (no window raise, no keystroke)
stage() { "$RMDV" open "$DEMO/$1" --no-focus >/dev/null 2>&1; sleep 0.5; }
# a real key combo via cliclick — KeyCastr shows THIS. usage: key cmd m  |  key cmd shift f
key() {
  front
  if [[ $DRY == 1 ]]; then echo "   [key] $*"; return; fi
  local mods=() k=""
  for a in "$@"; do case "$a" in cmd|shift|alt|ctrl) mods+=("$a");; *) k="$a";; esac; done
  for m in "${mods[@]}"; do cliclick "kd:$m"; done
  cliclick "t:$k" 2>/dev/null || cliclick "kp:$k"
  for ((i=${#mods[@]}-1;i>=0;i--)); do cliclick "ku:${mods[$i]}"; done
}
# a bare key press (arrows, esc, enter, return). usage: tap esc | tap arrow-down
tap() { front; [[ $DRY == 1 ]] && { echo "   [tap] $*"; return; }; cliclick "kp:$*"; }
# close any lingering overlay (command palette / search) before the next beat
dismiss() { front; [[ $DRY == 1 ]] && { echo "   [esc]"; return; }; cliclick kp:esc; sleep 0.3; }
# type literal text (for fuzzy-find / search queries)
typ() { front; [[ $DRY == 1 ]] && { echo "   [type] $*"; return; }; cliclick -w 60 "t:$*"; }

# ── launch on the vault, sidebar README ───────────────────────────────
say "Launch rmdv on the demo vault"
if [[ $DRY == 0 ]]; then
  nohup "$RMDV" open "$DEMO/README.md" --focus >/tmp/rmdv-tour.log 2>&1 &
  sleep 2.8
  viewmode      # kill any stale mindmap/edit mode from a reused instance
  front
fi
hold "$READ"   # 0. cold open — README + sidebar + breadcrumb

# ── 1. fuzzy file finder — Cmd+P ──────────────────────────────────────
# Show the finder (keycaster sees ⌘P + the query), Esc to close, then do the
# actual file switch over IPC — far more reliable than driving the match list.
say "Cmd+P → fuzzy file finder"
key cmd p ;           hold 0.8
typ "oauth" ;         hold 1.2
dismiss ;             hold 0.4
stage "reference/api/v2/auth/oauth.md" ; viewmode ; front ; hold "$PACE"
say "Cmd+Down / Cmd+Up → walk headings"
key cmd arrow-down ;  hold 0.7
key cmd arrow-down ;  hold 0.7
key cmd arrow-up ;    hold "$PACE"

# ── 2. markdown kitchen sink ──────────────────────────────────────────
say "Markdown kitchen sink (staged via sidebar file)"
stage "guide/features/markdown/syntax.md"
front ; hold "$READ"          # table / task list / nested lists / code blocks

# ── 3. diagrams ───────────────────────────────────────────────────────
say "Mermaid diagrams"
stage "guide/features/diagrams/mermaid/flowcharts.md" ; front ; hold "$READ"
say "Graphviz DOT"
stage "guide/features/diagrams/graphviz/dot.md" ; front ; hold "$READ"

# ── 4. math + 5. tex ──────────────────────────────────────────────────
say "Block math"
stage "guide/features/math/equations.md" ; front ; hold "$READ"
say "Full .tex document"
stage "papers/research/relativity.tex" ; front ; hold "$READ"
say "A real PDF, read as Markdown (local PDFium, no cloud)"
stage "papers/research/NIST.SP.800-63-4-excerpt.pdf" ; front ; hold "$READ"

# ── 6. images ─────────────────────────────────────────────────────────
say "Images (local + remote). MANUAL: click an image for the zoom modal."
stage "guide/features/images/gallery.md" ; front ; hold "$READ"

# ── 7. document mind map — Cmd+M ──────────────────────────────────────
say "Cmd+M → document mind map, arrow-walk the tree"
stage "guide/features/mindmap/document-mindmap.md" ; viewmode ; front ; hold 1.0
key cmd m ;            hold 1.2
tap arrow-right ;     hold 0.6
tap arrow-down ;      hold 0.6
tap arrow-down ;      hold 0.6
tap arrow-right ;     hold "$PACE"
key cmd m ;           hold "$PACE"   # back to view

# ── 8. data mind map — JSON then YAML ─────────────────────────────────
say "Cmd+M on JSON → data mind map"
stage "reference/data/config.json" ; viewmode ; front ; hold 1.0
key cmd m ;           hold 1.2
tap arrow-right ;     hold 0.6
tap arrow-down ;      hold "$PACE"
key cmd m ;           hold 1.0
say "Cmd+M on YAML → data mind map (non-string keys 200 / true)"
stage "reference/data/settings.yaml" ; viewmode ; front ; hold 1.0
key cmd m ;           hold 1.2
tap arrow-right ;     hold 0.6
tap arrow-down ;      hold "$READ"
key cmd m ;           hold "$PACE"

# ── 9. edit mode — Cmd+E ──────────────────────────────────────────────
say "Cmd+E → edit mode (live highlight), type, Cmd+E to exit"
stage "projects/2026/q2/notes/standup.md" ; viewmode ; front ; hold 1.0
key cmd e ;           hold 1.2
typ "- [ ] recorded the demo" ; hold "$READ"
key cmd e ;          hold "$PACE"   # Cmd+E toggles back (Esc stays in the editor)
viewmode

# ── 10. vault search — Cmd+Shift+F ────────────────────────────────────
say "Cmd+Shift+F → vault search 'mindmap'"
key cmd shift f ;    hold 1.0
typ "mindmap" ;      hold "$READ"
tap arrow-down ;     hold 0.6
tap arrow-down ;     hold 0.6
tap return ;         hold "$PACE"   # opens the hit
tap esc ;            hold "$PACE"

# ── 11. close on README ───────────────────────────────────────────────
say "Back to README to close"
stage "README.md" ; front ; hold "$READ"

say "Tour complete."

#!/usr/bin/env bash
# Clippy baseline gate.
#
# Runs `cargo clippy --all-targets` (default features, lints capped at warn), counts clippy warnings
# per lint, and compares with clippy-baseline.txt ("<lint> <count>" per line).
# Fails when any lint count grows. Prints a hint when a count shrinks.
#
#   scripts/clippy-baseline.sh            check against the baseline
#   scripts/clippy-baseline.sh --update   rewrite the baseline from the current run
set -euo pipefail

cd "$(dirname "$0")/.."
BASELINE="clippy-baseline.txt"
MODE="check"
case "${1:-}" in
  "") ;;
  --update) MODE="update" ;;
  *) echo "usage: $0 [--update]" >&2; exit 2 ;;
esac

JSON="$(mktemp)"
CUR="$(mktemp)"
trap 'rm -f "$JSON" "$CUR"' EXIT

set +e
# --cap-lints warn: deny-by-default lints with old hits must not stop the build;
# they are counted like any other warning.
cargo clippy --all-targets --message-format=json -- --cap-lints warn >"$JSON"
STATUS=$?
set -e

python3 - "$JSON" >"$CUR" <<'PY' || { echo "clippy-baseline: clippy reported errors (exit $STATUS)" >&2; exit 1; }
import json, sys
seen = set()
counts = {}
errors = []
for raw in open(sys.argv[1]):
    try:
        m = json.loads(raw)
    except ValueError:
        continue
    if m.get("reason") != "compiler-message":
        continue
    msg = m["message"]
    if msg.get("level") == "error":
        errors.append(msg.get("rendered", ""))
    code = (msg.get("code") or {}).get("code") or ""
    if not code.startswith("clippy::"):
        continue
    span = next((s for s in msg.get("spans", []) if s.get("is_primary")), None)
    key = (code, span["file_name"], span["line_start"], span["column_start"]) if span else (code, msg["message"])
    if key in seen:  # the lib is checked twice (lib and lib test)
        continue
    seen.add(key)
    counts[code] = counts.get(code, 0) + 1
if errors:
    sys.stderr.write("\n".join(errors) + "\n")
    sys.exit(3)
for k in sorted(counts):
    print(k, counts[k])
PY

if [ "$STATUS" -ne 0 ]; then
  echo "clippy-baseline: cargo clippy exited $STATUS (build failure?)" >&2
  exit 1
fi

if [ "$MODE" = "update" ]; then
  cp "$CUR" "$BASELINE"
  echo "clippy-baseline: wrote $BASELINE ($(wc -l <"$BASELINE" | tr -d ' ') lints)"
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "clippy-baseline: $BASELINE missing; run $0 --update" >&2
  exit 1
fi

python3 - "$BASELINE" "$CUR" <<'PY'
import sys
def load(path):
    d = {}
    for line in open(path):
        line = line.strip()
        if line:
            k, v = line.rsplit(" ", 1)
            d[k] = int(v)
    return d
base, cur = load(sys.argv[1]), load(sys.argv[2])
worse, better = [], []
for k in sorted(set(base) | set(cur)):
    b, c = base.get(k, 0), cur.get(k, 0)
    if c > b:
        worse.append(f"  {k}: {b} -> {c} (+{c-b})")
    elif c < b:
        better.append(f"  {k}: {b} -> {c} ({c-b})")
if better:
    print("clippy-baseline: counts went down; run scripts/clippy-baseline.sh --update to lock it in:")
    print("\n".join(better))
if worse:
    print("clippy-baseline: FAIL, new clippy warnings:")
    print("\n".join(worse))
    sys.exit(1)
print("clippy-baseline: OK")
PY

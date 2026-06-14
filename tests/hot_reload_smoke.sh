#!/usr/bin/env bash
# Manual smoke test: open a doc, edit a paragraph in another shell, confirm
# the cache hit count doesn't grow.
#
#   cargo build --release
#   ./tests/hot_reload_smoke.sh
#
# Then in another terminal: edit /tmp/hr.md (change a paragraph, save).
# In the rmdv terminal you should see hl_cache_hits incrementing each reload
# without hl_cache_len growing.
set -euo pipefail
TMP=/tmp/hr.md
cat > "$TMP" <<'EOF'
# Hot reload demo

Some paragraph.

```rust
fn main() { println!("hi"); }
```

Another paragraph.
EOF
exec ./target/release/rmdv "$TMP"

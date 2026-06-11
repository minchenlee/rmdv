//! Compact undo/redo snapshot stack for the raw-mode editor.
//!
//! The editor pushes a full-document `String` per edit action; storing those
//! verbatim costs `depth × doc_size` (200 × 1 MB = 200 MB on a large doc).
//! `SnapshotStack` keeps only the TOP entry as full text; every entry below it
//! is a reverse byte-delta against its successor: `(prefix, suffix, middle)`
//! where the older text = `newer[..prefix] + middle + newer[newer.len()-suffix..]`.
//! Keystroke edits shrink to a few bytes each, so a full 200-deep history on a
//! 1 MB doc costs ~1 MB instead of ~200 MB.
//!
//! Pop materializes exactly one delta (O(doc) splice — same order as the
//! `Content::with_text` rebuild the caller does with the result), so undo and
//! redo behave byte-for-byte identically to the old `Vec<String>`.

/// One stored snapshot: either full text (only ever the top entry) or a
/// reverse delta against the entry above it (its successor state).
#[derive(Debug)]
enum Entry {
    Full(String),
    Delta {
        /// Bytes shared with the successor text at the start.
        prefix: usize,
        /// Bytes shared with the successor text at the end.
        suffix: usize,
        /// The older text's bytes between the shared prefix and suffix.
        middle: Box<[u8]>,
    },
}

#[derive(Debug, Default)]
pub struct SnapshotStack {
    /// Oldest first. Invariant: if non-empty, the last entry is `Full`.
    entries: Vec<Entry>,
}

impl SnapshotStack {
    /// Push a new snapshot. The previous top is re-encoded as a delta against
    /// `text`, which becomes the new full top.
    pub fn push(&mut self, text: String) {
        if let Some(slot) = self.entries.last_mut() {
            let Entry::Full(old) = slot else {
                unreachable!("SnapshotStack invariant: top entry is Full");
            };
            *slot = encode_delta(old, &text);
        }
        self.entries.push(Entry::Full(text));
    }

    /// Push unless `text` equals the current top (the editor's per-keystroke
    /// dedupe). Returns whether a snapshot was pushed. Equality falls out of
    /// the delta scan itself — same lengths and an empty middle mean
    /// byte-identical — so this is one O(doc) pass, matching the cost of the
    /// plain `last() != text` check the old `Vec<String>` history did.
    pub fn push_if_changed(&mut self, text: String) -> bool {
        let Some(slot) = self.entries.last_mut() else {
            self.entries.push(Entry::Full(text));
            return true;
        };
        let Entry::Full(old) = slot else {
            unreachable!("SnapshotStack invariant: top entry is Full");
        };
        let delta = encode_delta(old, &text);
        if let Entry::Delta { middle, .. } = &delta {
            if middle.is_empty() && old.len() == text.len() {
                return false;
            }
        }
        *slot = delta;
        self.entries.push(Entry::Full(text));
        true
    }

    /// Pop the most recent snapshot, materializing the next entry (if any)
    /// back to full text so the invariant holds.
    pub fn pop(&mut self) -> Option<String> {
        let top = match self.entries.pop()? {
            Entry::Full(s) => s,
            Entry::Delta { .. } => unreachable!("SnapshotStack invariant: top entry is Full"),
        };
        if let Some(slot) = self.entries.last_mut() {
            if let Entry::Delta {
                prefix,
                suffix,
                middle,
            } = slot
            {
                *slot = Entry::Full(apply_delta(&top, *prefix, *suffix, middle));
            }
        }
        Some(top)
    }

    /// Drop the oldest snapshot (depth cap). The oldest entry is a delta no
    /// other entry references — each delta only needs its SUCCESSOR — so this
    /// is always safe.
    pub fn drop_oldest(&mut self) {
        if !self.entries.is_empty() {
            self.entries.remove(0);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Encode `old` as a reverse delta against its successor `new`.
fn encode_delta(old: &str, new: &str) -> Entry {
    let a = old.as_bytes();
    let b = new.as_bytes();
    let max = a.len().min(b.len());
    let mut prefix = 0;
    while prefix < max && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < max - prefix && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix] {
        suffix += 1;
    }
    Entry::Delta {
        prefix,
        suffix,
        middle: a[prefix..a.len() - suffix].into(),
    }
}

/// Reconstruct the older text from its successor `new` plus a reverse delta.
/// `prefix`/`suffix` are common-byte counts of the two original strings, so
/// the spliced bytes are exactly the older string's original bytes — valid
/// UTF-8 by construction.
fn apply_delta(new: &str, prefix: usize, suffix: usize, middle: &[u8]) -> String {
    let b = new.as_bytes();
    let mut out = Vec::with_capacity(prefix + middle.len() + suffix);
    out.extend_from_slice(&b[..prefix]);
    out.extend_from_slice(middle);
    out.extend_from_slice(&b[b.len() - suffix..]);
    String::from_utf8(out).expect("delta splice reproduces the original UTF-8 text")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_round_trips() {
        let mut s = SnapshotStack::default();
        s.push("hello".into());
        s.push("hello world".into());
        s.push("help world".into());
        assert_eq!(s.pop().as_deref(), Some("help world"));
        assert_eq!(s.pop().as_deref(), Some("hello world"));
        assert_eq!(s.pop().as_deref(), Some("hello"));
        assert_eq!(s.pop(), None);
    }

    fn floor_boundary(s: &str, i: usize) -> usize {
        let mut i = i.min(s.len());
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    #[test]
    fn matches_vec_string_model_through_random_edits() {
        // Reference model: plain Vec<String>, as app.rs used before.
        let mut model: Vec<String> = Vec::new();
        let mut stack = SnapshotStack::default();
        let mut doc = String::from("# Title\n\nSome paragraph with text.\n");
        // Deterministic pseudo-random edit script (no RNG in tests).
        for i in 0..500usize {
            let snapshot = doc.clone();
            let model_pushed = model.last().map(|s| s != &snapshot).unwrap_or(true);
            let stack_pushed = stack.push_if_changed(snapshot.clone());
            assert_eq!(stack_pushed, model_pushed, "dedupe divergence at step {i}");
            if model_pushed {
                model.push(snapshot);
                if model.len() > 50 {
                    model.remove(0);
                    stack.drop_oldest();
                }
            }
            // Mutate doc: insert, delete, or replace at varying positions.
            let pos = floor_boundary(&doc, (i * 37) % doc.len().max(1));
            match i % 3 {
                0 => doc.insert_str(pos, "x✓y"),
                1 => {
                    let end = floor_boundary(&doc, pos + 3);
                    doc.replace_range(pos..end, "");
                }
                _ => {
                    let end = floor_boundary(&doc, pos + 2);
                    doc.replace_range(pos..end, "Ω");
                }
            }
        }
        assert_eq!(stack.len(), model.len());
        while let Some(expect) = model.pop() {
            assert_eq!(stack.pop(), Some(expect));
        }
        assert!(stack.is_empty());
    }

    #[test]
    fn unicode_boundary_deltas_round_trip() {
        let mut s = SnapshotStack::default();
        // é (C3 A9) -> è (C3 A8): byte prefix splits mid-codepoint.
        s.push("café".into());
        s.push("cafè".into());
        assert_eq!(s.pop().as_deref(), Some("cafè"));
        assert_eq!(s.pop().as_deref(), Some("café"));
    }

    #[test]
    fn identical_consecutive_snapshots() {
        let mut s = SnapshotStack::default();
        s.push("same".into());
        s.push("same".into());
        assert_eq!(s.pop().as_deref(), Some("same"));
        assert_eq!(s.pop().as_deref(), Some("same"));
    }

    #[test]
    fn push_if_changed_dedupes_identical_top() {
        let mut s = SnapshotStack::default();
        assert!(s.push_if_changed("a".into()), "first push always lands");
        assert!(!s.push_if_changed("a".into()), "identical text deduped");
        assert!(s.push_if_changed("ab".into()), "longer text pushes");
        // Same length, different bytes: middle is non-empty, must push.
        assert!(s.push_if_changed("xb".into()));
        assert_eq!(s.len(), 3);
        assert_eq!(s.pop().as_deref(), Some("xb"));
        assert_eq!(s.pop().as_deref(), Some("ab"));
        assert_eq!(s.pop().as_deref(), Some("a"));
    }

    #[test]
    fn overlapping_prefix_suffix_repeated_text() {
        let mut s = SnapshotStack::default();
        s.push("aaaa".into());
        s.push("aaaaaa".into());
        s.push("aaa".into());
        assert_eq!(s.pop().as_deref(), Some("aaa"));
        assert_eq!(s.pop().as_deref(), Some("aaaaaa"));
        assert_eq!(s.pop().as_deref(), Some("aaaa"));
    }

    #[test]
    fn empty_strings() {
        let mut s = SnapshotStack::default();
        s.push(String::new());
        s.push("text".into());
        s.push(String::new());
        assert_eq!(s.pop().as_deref(), Some(""));
        assert_eq!(s.pop().as_deref(), Some("text"));
        assert_eq!(s.pop().as_deref(), Some(""));
    }

    #[test]
    fn drop_oldest_keeps_remaining_chain_intact() {
        let mut s = SnapshotStack::default();
        for i in 0..10 {
            s.push(format!("doc version {i} content"));
        }
        for _ in 0..5 {
            s.drop_oldest();
        }
        assert_eq!(s.len(), 5);
        for i in (5..10).rev() {
            assert_eq!(s.pop(), Some(format!("doc version {i} content")));
        }
        assert_eq!(s.pop(), None);
    }

    #[test]
    fn keystroke_deltas_stay_small() {
        let base = "x".repeat(100_000);
        let mut s = SnapshotStack::default();
        let mut doc = base.clone();
        for i in 0..100 {
            s.push(doc.clone());
            doc.push_str(&format!("{i}"));
        }
        // All but the top entry are deltas of a few bytes; retained bytes
        // must be near one doc, not 100 docs.
        let retained: usize = s
            .entries
            .iter()
            .map(|e| match e {
                Entry::Full(s) => s.len(),
                Entry::Delta { middle, .. } => middle.len() + 16,
            })
            .sum();
        assert!(
            retained < 2 * base.len(),
            "retained {retained} bytes for {} snapshots of a {} byte doc",
            s.len(),
            base.len()
        );
    }
}

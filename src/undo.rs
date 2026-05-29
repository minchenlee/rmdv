//! Delta-based undo for the editor.
//!
//! Instead of snapshotting the whole document on every keystroke, we store a
//! minimal `Edit` describing the single contiguous span that changed between
//! two document versions. This keeps undo/redo memory proportional to the size
//! of each edit rather than the size of the document.
//!
//! An `Edit` records the byte `start` of the changed span plus the `removed`
//! and `inserted` text at that span. Going *backwards* (undo: new -> old)
//! replaces `inserted` with `removed`; going *forwards* (redo: old -> new)
//! replaces `removed` with `inserted`.

/// A minimal contiguous edit between two document versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Byte offset where the changed span begins (a char boundary in both versions).
    pub start: usize,
    /// Text present in the *old* version at `start`.
    pub removed: String,
    /// Text present in the *new* version at `start`.
    pub inserted: String,
}

/// Compute the minimal contiguous edit turning `old` into `new` by trimming the
/// common prefix and suffix. Returns `None` when the strings are identical.
pub fn diff(old: &str, new: &str) -> Option<Edit> {
    if old == new {
        return None;
    }
    let ob = old.as_bytes();
    let nb = new.as_bytes();

    // Common prefix length (byte count), snapped back to a char boundary.
    let max_pre = ob.len().min(nb.len());
    let mut pre = 0;
    while pre < max_pre && ob[pre] == nb[pre] {
        pre += 1;
    }
    while pre > 0 && (!old.is_char_boundary(pre) || !new.is_char_boundary(pre)) {
        pre -= 1;
    }

    // Common suffix length, not overlapping the prefix, snapped to char boundary.
    let max_suf = (ob.len() - pre).min(nb.len() - pre);
    let mut suf = 0;
    while suf < max_suf && ob[ob.len() - 1 - suf] == nb[nb.len() - 1 - suf] {
        suf += 1;
    }
    let mut o_end = ob.len() - suf;
    let mut n_end = nb.len() - suf;
    while o_end < ob.len()
        && (!old.is_char_boundary(o_end) || !new.is_char_boundary(n_end))
        && o_end > pre
        && n_end > pre
    {
        o_end += 1;
        n_end += 1;
    }

    Some(Edit {
        start: pre,
        removed: old[pre..o_end].to_string(),
        inserted: new[pre..n_end].to_string(),
    })
}

/// Apply `edit` forward (old -> new) to `text`: replace `removed` at `start`
/// with `inserted`. `text` must be the old version.
pub fn redo(text: &str, edit: &Edit) -> String {
    let mut out = String::with_capacity(text.len() + edit.inserted.len());
    out.push_str(&text[..edit.start]);
    out.push_str(&edit.inserted);
    out.push_str(&text[edit.start + edit.removed.len()..]);
    out
}

/// Apply `edit` backward (new -> old) to `text`: replace `inserted` at `start`
/// with `removed`. `text` must be the new version.
pub fn undo(text: &str, edit: &Edit) -> String {
    let mut out = String::with_capacity(text.len() + edit.removed.len());
    out.push_str(&text[..edit.start]);
    out.push_str(&edit.removed);
    out.push_str(&text[edit.start + edit.inserted.len()..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_have_no_diff() {
        assert_eq!(diff("hello", "hello"), None);
    }

    #[test]
    fn insertion_in_middle() {
        let e = diff("hello world", "hello big world").unwrap();
        assert_eq!(e.start, 6);
        assert_eq!(e.removed, "");
        assert_eq!(e.inserted, "big ");
    }

    #[test]
    fn deletion_in_middle() {
        let e = diff("hello big world", "hello world").unwrap();
        assert_eq!(e.removed, "big ");
        assert_eq!(e.inserted, "");
    }

    #[test]
    fn replacement() {
        let e = diff("the cat sat", "the dog sat").unwrap();
        assert_eq!(e.removed, "cat");
        assert_eq!(e.inserted, "dog");
    }

    #[test]
    fn append_at_end() {
        let e = diff("abc", "abcdef").unwrap();
        assert_eq!(e.start, 3);
        assert_eq!(e.removed, "");
        assert_eq!(e.inserted, "def");
    }

    #[test]
    fn prepend_at_start() {
        let e = diff("abc", "xyabc").unwrap();
        assert_eq!(e.start, 0);
        assert_eq!(e.removed, "");
        assert_eq!(e.inserted, "xy");
    }

    #[test]
    fn full_clear() {
        let e = diff("content", "").unwrap();
        assert_eq!(e.start, 0);
        assert_eq!(e.removed, "content");
        assert_eq!(e.inserted, "");
    }

    #[test]
    fn redo_reconstructs_new() {
        let old = "hello world";
        let new = "hello big world";
        let e = diff(old, new).unwrap();
        assert_eq!(redo(old, &e), new);
    }

    #[test]
    fn undo_reconstructs_old() {
        let old = "hello world";
        let new = "hello big world";
        let e = diff(old, new).unwrap();
        assert_eq!(undo(new, &e), old);
    }

    #[test]
    fn roundtrip_unicode() {
        // Multi-byte chars must not split; diff snaps to char boundaries.
        let old = "café ☕ ok";
        let new = "café ☕☕ ok";
        let e = diff(old, new).unwrap();
        assert_eq!(redo(old, &e), new);
        assert_eq!(undo(new, &e), old);
    }

    #[test]
    fn multi_edit_undo_redo_chain() {
        // Simulate three sequential edits with a history of diffs, then undo
        // all the way back and redo all the way forward.
        let v0 = "one";
        let v1 = "one two";
        let v2 = "one two three";
        let v3 = "ZERO one two three";

        let d1 = diff(v0, v1).unwrap();
        let d2 = diff(v1, v2).unwrap();
        let d3 = diff(v2, v3).unwrap();

        // Undo chain: v3 -> v2 -> v1 -> v0
        let back2 = undo(v3, &d3);
        assert_eq!(back2, v2);
        let back1 = undo(&back2, &d2);
        assert_eq!(back1, v1);
        let back0 = undo(&back1, &d1);
        assert_eq!(back0, v0);

        // Redo chain: v0 -> v1 -> v2 -> v3
        let fwd1 = redo(&back0, &d1);
        assert_eq!(fwd1, v1);
        let fwd2 = redo(&fwd1, &d2);
        assert_eq!(fwd2, v2);
        let fwd3 = redo(&fwd2, &d3);
        assert_eq!(fwd3, v3);
    }

    #[test]
    fn repeated_substring_suffix_match() {
        // Common-suffix trimming must not over-match across the change.
        let old = "aaa";
        let new = "aa";
        let e = diff(old, new).unwrap();
        assert_eq!(redo(old, &e), new);
        assert_eq!(undo(new, &e), old);
    }
}

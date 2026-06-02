//! Vault-wide (cross-file) full-text search.
//!
//! On-demand raw-text grep over the walked `workspace_files` corpus — no index,
//! no persistent memory. Each query reads every file fresh and reports a hit per
//! matching line, landing the reader on the exact line via `PendingNav.line`.

use std::path::{Path, PathBuf};

use crate::ast::HlSpan;
use crate::ipc::lines::build_byte_to_line;
use crate::search::find_all;

/// One source line shown in a hit's context window.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextLine {
    /// 1-based source line number.
    pub number: u32,
    /// Full line text (untrimmed; the page clips long lines).
    pub text: String,
    /// True for the line containing the match.
    pub is_match: bool,
    /// Markdown syntax-highlight spans over `text` (byte ranges), computed once
    /// here so the render path doesn't re-parse every frame.
    pub spans: Vec<HlSpan>,
}

/// One match in one file, with surrounding context lines.
#[derive(Debug, Clone)]
pub struct VaultHit {
    pub path: PathBuf,
    /// 1-based line number of the match, matching the `Cmd::Goto` convention.
    pub line: u32,
    /// Char offset of the match start within its line.
    pub col_start: usize,
    /// Char offset of the match end within its line.
    pub col_end: usize,
    /// The match line plus up to `CONTEXT` lines each side, in order.
    pub context: Vec<ContextLine>,
}

/// Result of one query, tagged with the request `seq` so the UI can drop a
/// result whose query has since been superseded.
#[derive(Debug, Clone)]
pub struct VaultResults {
    pub hits: Vec<VaultHit>,
    pub truncated: bool,
    pub seq: u64,
}

/// Hard cap on total hits across all files.
pub const MAX_HITS: usize = 200;

/// Number of context lines shown above and below each match line.
pub const CONTEXT: usize = 2;

/// Scan one file's text for `query`, appending hits to `out`. Returns `true` if
/// the `MAX_HITS` cap was reached (caller should stop scanning further files).
fn scan_text(text: &str, query: &str, path: &Path, out: &mut Vec<VaultHit>) -> bool {
    let offsets = find_all(text, query);
    if offsets.is_empty() {
        return false;
    }
    // Split once per file; reused across every match in this file.
    let lines: Vec<&str> = text.split('\n').collect();
    let table = build_byte_to_line(text);
    let q_chars = query.chars().count();
    for off in offsets {
        if out.len() >= MAX_HITS {
            return true;
        }
        let line = table.line_for_byte(off);
        let li = (line as usize).saturating_sub(1); // 0-based index into `lines`
        // Line start comes from the byte→line table (no extra backward scan).
        let line_start = table.line_start(line).unwrap_or(0);
        let col_start = text[line_start..off].chars().count();
        let col_end = col_start + q_chars;

        let lo = li.saturating_sub(CONTEXT);
        let hi = (li + CONTEXT).min(lines.len().saturating_sub(1));
        let context = (lo..=hi)
            .map(|i| {
                let text = lines[i].trim_end_matches('\r').to_string();
                // Highlight only the match line (Zed-style): context lines render
                // as dim plain text, so skipping their highlight cuts the span
                // count ~5× and avoids extra tree-sitter parses per hit.
                let spans = if i == li {
                    crate::highlight::highlight("md", &text)
                } else {
                    Vec::new()
                };
                ContextLine {
                    number: (i + 1) as u32,
                    text,
                    is_match: i == li,
                    spans,
                }
            })
            .collect();

        out.push(VaultHit {
            path: path.to_path_buf(),
            line,
            col_start,
            col_end,
            context,
        });
    }
    false
}

/// Search every file in `files` for `query`. Reads files on the async runtime;
/// debounced by an initial sleep so rapid keystrokes coalesce. Unreadable or
/// non-UTF8 files are skipped. `seq` is echoed back unchanged for staleness.
pub async fn run(files: Vec<PathBuf>, query: String, seq: u64) -> VaultResults {
    if query.is_empty() {
        return VaultResults {
            hits: Vec::new(),
            truncated: false,
            seq,
        };
    }

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    use futures::stream::{self, StreamExt};

    // Read + scan files concurrently (I/O latency overlaps) but keep results in
    // file-walk order via `buffered`, so the UI's contiguous-per-file grouping
    // still holds. Each file scans into its own bucket and reports whether its
    // own scan hit the per-file cap. We drain the stream in order and stop
    // feeding results once the global MAX_HITS cap is reached, so we don't merge
    // the long tail (the in-flight `buffered` futures still finish, but their
    // buckets are dropped).
    const READ_CONCURRENCY: usize = 16;
    let mut stream = stream::iter(files.iter().cloned())
        .map(|path| {
            let query = query.clone();
            async move {
                let Ok(bytes) = tokio::fs::read(&path).await else {
                    return (Vec::new(), false);
                };
                let Ok(text) = String::from_utf8(bytes) else {
                    return (Vec::new(), false);
                };
                let mut local = Vec::new();
                let capped = scan_text(&text, &query, &path, &mut local);
                (local, capped)
            }
        })
        .buffered(READ_CONCURRENCY);

    let mut hits = Vec::new();
    let mut truncated = false;
    while let Some((mut bucket, capped)) = stream.next().await {
        // A file whose own scan hit the per-file cap means matches were dropped
        // within that file, regardless of the global cap.
        truncated |= capped;
        if bucket.is_empty() {
            continue;
        }
        let room = MAX_HITS - hits.len();
        if bucket.len() > room {
            // This bucket overflows the global cap: keep what fits, drop the rest.
            bucket.truncate(room);
            hits.append(&mut bucket);
            truncated = true;
            break;
        }
        hits.append(&mut bucket);
        if hits.len() == MAX_HITS {
            // Exactly full. Any further non-empty bucket would be dropped, so
            // that — and only that — counts as truncation (an exact fill with no
            // remaining matches is not truncated).
            break;
        }
    }
    if hits.len() == MAX_HITS && !truncated {
        // We stopped at an exact fill; flag truncation iff more matches remain.
        while let Some((bucket, capped)) = stream.next().await {
            if !bucket.is_empty() || capped {
                truncated = true;
                break;
            }
        }
    }

    VaultResults {
        hits,
        truncated,
        seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn scan(text: &str, query: &str) -> Vec<VaultHit> {
        let mut out = Vec::new();
        scan_text(text, query, &p("f.md"), &mut out);
        out
    }

    #[test]
    fn match_reports_correct_line() {
        let text = "alpha\nbeta needle here\ngamma";
        let hits = scan(text, "needle");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn length_changing_lowercase_does_not_panic_or_misreport() {
        // 'İ' (U+0130) lowercases to "i̇" (2 chars / 3 bytes) — longer than the
        // 1-char / 2-byte original. find_all used to return offsets into the
        // lowercased copy, which indexed past / off-boundary in the original
        // and panicked. Offsets must now be original-relative.
        let text = "İİİ needle\nİx";
        let hits = scan(text, "needle");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
        // "İİİ " is 4 chars before the match.
        assert_eq!(hits[0].col_start, 4);
        let m = hits[0].context.iter().find(|c| c.is_match).unwrap();
        assert_eq!(&m.text[m.text.char_indices().nth(4).unwrap().0..], "needle");
    }

    #[test]
    fn first_line_match_is_line_one() {
        let hits = scan("needle on line one\nsecond", "needle");
        assert_eq!(hits[0].line, 1);
    }

    #[test]
    fn multiple_matches_distinct_lines() {
        let text = "needle\nfiller\nneedle again\nneedle";
        let hits = scan(text, "needle");
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[1].line, 3);
        assert_eq!(hits[2].line, 4);
    }

    #[test]
    fn case_insensitive() {
        let hits = scan("The NEEDLE is here", "needle");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn no_match_empty() {
        assert!(scan("nothing to see", "needle").is_empty());
    }

    #[test]
    fn match_line_flagged_in_context() {
        // lines 1..=5; match on line 3 → context is lines 1..=5, line 3 flagged.
        let text = "l1\nl2\nneedle line3\nl4\nl5";
        let hits = scan(text, "needle");
        let ctx = &hits[0].context;
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx.iter().filter(|c| c.is_match).count(), 1);
        let m = ctx.iter().find(|c| c.is_match).unwrap();
        assert_eq!(m.number, 3);
        assert_eq!(m.text, "needle line3");
        // Numbers are sequential 1-based.
        assert_eq!(ctx.iter().map(|c| c.number).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn context_clamped_at_file_start() {
        // Match on line 1: 0 lines above, up to CONTEXT below.
        let text = "needle\nl2\nl3\nl4";
        let hits = scan(text, "needle");
        let ctx = &hits[0].context;
        assert_eq!(ctx.first().unwrap().number, 1);
        assert!(ctx.first().unwrap().is_match);
        assert_eq!(ctx.len(), 1 + CONTEXT); // match + 2 below
    }

    #[test]
    fn context_clamped_at_file_end() {
        // Match on the last line: up to CONTEXT above, 0 below.
        let text = "l1\nl2\nl3\nneedle";
        let hits = scan(text, "needle");
        let ctx = &hits[0].context;
        assert_eq!(ctx.last().unwrap().number, 4);
        assert!(ctx.last().unwrap().is_match);
        assert_eq!(ctx.len(), 1 + CONTEXT); // match + 2 above
    }

    #[test]
    fn col_span_locates_match_within_line() {
        let hits = scan("xx needle yy", "needle");
        assert_eq!(hits[0].col_start, 3);
        assert_eq!(hits[0].col_end, 9); // "needle" is 6 chars
    }

    #[test]
    fn col_span_multibyte_safe() {
        // "café " is 5 chars; match starts at char offset 5.
        let hits = scan("café needle", "needle");
        assert_eq!(hits[0].col_start, 5);
        assert_eq!(hits[0].col_end, 11);
    }

    #[test]
    fn adjacent_matches_each_keep_own_context() {
        // Matches on lines 2 and 3; each hit carries its own window.
        let text = "l1\nneedle two\nneedle three\nl4";
        let hits = scan(text, "needle");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line, 2);
        assert_eq!(hits[1].line, 3);
        assert!(hits[0].context.iter().any(|c| c.is_match && c.number == 2));
        assert!(hits[1].context.iter().any(|c| c.is_match && c.number == 3));
    }

    #[test]
    fn cap_truncates() {
        let text = "needle\n".repeat(MAX_HITS + 50);
        let mut out = Vec::new();
        let hit_cap = scan_text(&text, "needle", &p("f.md"), &mut out);
        assert!(hit_cap);
        assert_eq!(out.len(), MAX_HITS);
    }

    #[test]
    fn run_empty_query_returns_empty_with_seq() {
        let r = futures::executor::block_on(run(vec![p("nope.md")], String::new(), 42));
        assert!(r.hits.is_empty());
        assert!(!r.truncated);
        assert_eq!(r.seq, 42);
    }

    #[test]
    fn run_single_file_over_cap_reports_truncated() {
        // A single file with more than MAX_HITS matches: scan_text caps the
        // file's own bucket at MAX_HITS and drops the rest, so `run` must still
        // report `truncated` even though the merge sees only one (full) bucket.
        // Regression for the false-negative where scan_text's capped return was
        // discarded by the per-file-bucket refactor.
        let dir = std::env::temp_dir().join(format!("mdv_vault_run_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.md");
        std::fs::write(&file, "needle\n".repeat(MAX_HITS + 50)).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let r = rt.block_on(run(vec![file.clone()], "needle".to_string(), 1));
        assert_eq!(r.hits.len(), MAX_HITS);
        assert!(r.truncated, "single file over cap must report truncated");

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn run_exact_cap_no_remainder_not_truncated() {
        // Exactly MAX_HITS matches across the corpus with nothing left over must
        // NOT be flagged truncated (guards against an over-eager exact-fill flag).
        let dir =
            std::env::temp_dir().join(format!("mdv_vault_exact_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("exact.md");
        std::fs::write(&file, "needle\n".repeat(MAX_HITS)).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let r = rt.block_on(run(vec![file.clone()], "needle".to_string(), 1));
        assert_eq!(r.hits.len(), MAX_HITS);
        assert!(!r.truncated, "exact fill with no remainder is not truncated");

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }
}

//! Vault-wide (cross-file) full-text search.
//!
//! On-demand raw-text grep over the walked `workspace_files` corpus — no index,
//! no persistent memory. Each query reads every file fresh and reports a hit per
//! matching line, landing the reader on the exact line via `PendingNav.line`.

use std::path::{Path, PathBuf};

use crate::ipc::lines::build_byte_to_line;
use crate::search::find_all;

/// One source line shown in a hit's context window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    /// 1-based source line number.
    pub number: u32,
    /// Full line text (untrimmed; the page clips long lines).
    pub text: String,
    /// True for the line containing the match.
    pub is_match: bool,
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
        let line_start = text[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col_start = text[line_start..off].chars().count();
        let col_end = col_start + q_chars;

        let lo = li.saturating_sub(CONTEXT);
        let hi = (li + CONTEXT).min(lines.len().saturating_sub(1));
        let context = (lo..=hi)
            .map(|i| ContextLine {
                number: (i + 1) as u32,
                text: lines[i].trim_end_matches('\r').to_string(),
                is_match: i == li,
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

    let mut hits = Vec::new();
    let mut truncated = false;
    for path in &files {
        let Ok(bytes) = tokio::fs::read(path).await else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        if scan_text(&text, &query, path, &mut hits) {
            truncated = true;
            break;
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
}

//! Vault-wide (cross-file) full-text search.
//!
//! On-demand raw-text grep over the walked `workspace_files` corpus — no index,
//! no persistent memory. Each query reads every file fresh and reports a hit per
//! matching line, landing the reader on the exact line via `PendingNav.line`.

use std::path::{Path, PathBuf};

use crate::ipc::lines::build_byte_to_line;
use crate::search::find_all;

/// One matching line in one file.
#[derive(Debug, Clone)]
pub struct VaultHit {
    pub path: PathBuf,
    /// 1-based line number, matching the `Cmd::Goto` convention.
    pub line: u32,
    /// The matched line, trimmed to a window centred on the match.
    pub snippet: String,
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

/// Max snippet length in characters; the match is windowed to stay visible.
const SNIPPET_LEN: usize = 80;

/// Scan one file's text for `query`, appending hits to `out`. Returns `true` if
/// the `MAX_HITS` cap was reached (caller should stop scanning further files).
fn scan_text(text: &str, query: &str, path: &Path, out: &mut Vec<VaultHit>) -> bool {
    let offsets = find_all(text, query);
    if offsets.is_empty() {
        return false;
    }
    let table = build_byte_to_line(text);
    for off in offsets {
        if out.len() >= MAX_HITS {
            return true;
        }
        let line = table.line_for_byte(off);
        let snippet = line_snippet(text, off, query.len());
        out.push(VaultHit {
            path: path.to_path_buf(),
            line,
            snippet,
        });
    }
    false
}

/// Extract the source line containing byte `off`, trimmed to a window around the
/// match so the matched text stays visible within `SNIPPET_LEN` chars.
fn line_snippet(text: &str, off: usize, match_len: usize) -> String {
    let line_start = text[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[off..]
        .find('\n')
        .map(|i| off + i)
        .unwrap_or(text.len());
    let line = text[line_start..line_end].trim_end();

    // Offset of the match within the (untrimmed) line.
    let match_in_line = off - line_start;

    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= SNIPPET_LEN {
        return line.trim_start().to_string();
    }

    // Window centred on the match, clamped to the line bounds. Use char counts,
    // not byte offsets, so multibyte text isn't split mid-codepoint.
    let match_char = line[..match_in_line.min(line.len())].chars().count();
    let match_chars = query_char_len(line, match_in_line, match_len);
    let half = SNIPPET_LEN.saturating_sub(match_chars) / 2;
    let start = match_char.saturating_sub(half);
    let end = (start + SNIPPET_LEN).min(chars.len());
    let start = end.saturating_sub(SNIPPET_LEN);

    let mut s = String::new();
    if start > 0 {
        s.push('…');
    }
    s.extend(&chars[start..end]);
    if end < chars.len() {
        s.push('…');
    }
    s
}

/// Char length of the matched span (clamped to the line).
fn query_char_len(line: &str, match_in_line: usize, match_len: usize) -> usize {
    let end = (match_in_line + match_len).min(line.len());
    line[match_in_line.min(line.len())..end].chars().count()
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
    fn snippet_contains_match_short_line() {
        let hits = scan("short needle line", "needle");
        assert!(hits[0].snippet.contains("needle"));
    }

    #[test]
    fn snippet_windowed_on_long_line() {
        let prefix = "x".repeat(200);
        let suffix = "y".repeat(200);
        let text = format!("{prefix} needle {suffix}");
        let hits = scan(&text, "needle");
        assert!(hits[0].snippet.contains("needle"));
        // Windowed: far shorter than the full 400+ char line, with ellipses.
        assert!(hits[0].snippet.chars().count() <= SNIPPET_LEN + 2);
        assert!(hits[0].snippet.starts_with('…'));
        assert!(hits[0].snippet.ends_with('…'));
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
    fn multibyte_line_not_split() {
        // Match after a run of multibyte chars; snippet must be valid UTF-8
        // (String guarantees this — assertion is that it doesn't panic).
        let text = format!("{} needle tail", "café ".repeat(60));
        let hits = scan(&text, "needle");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("needle"));
    }

    #[test]
    fn run_empty_query_returns_empty_with_seq() {
        let r = futures::executor::block_on(run(vec![p("nope.md")], String::new(), 42));
        assert!(r.hits.is_empty());
        assert!(!r.truncated);
        assert_eq!(r.seq, 42);
    }
}

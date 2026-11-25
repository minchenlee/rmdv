//! Architecture boundary test.
//!
//! The parser and model layer (and the IPC types) must not depend on the UI:
//! no `iced` (or `iced_*`) crate and no `crate::app`. This test scans the
//! source of those modules and fails on any such reference in code.
//! Comments and string/char literal contents are ignored.

use std::fs;
use std::path::{Path, PathBuf};

const HINT: &str = "this module must not depend on the UI; move the needed type to the model layer";

/// Files that must stay UI-free. Directories are scanned recursively.
const GUARDED: &[&str] = &[
    "src/parser.rs",
    "src/ast.rs",
    "src/tex.rs",
    "src/tree.rs",
    "src/ipc",
];

/// Replace comments and string/char literal contents with spaces.
/// Newlines are kept so line numbers stay correct.
fn strip_non_code(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut out = String::with_capacity(src.len());
    let blank = |ch: char| if ch == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < n {
        let ch = c[i];
        // line comment
        if ch == '/' && i + 1 < n && c[i + 1] == '/' {
            while i < n && c[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // block comment (nested)
        if ch == '/' && i + 1 < n && c[i + 1] == '*' {
            let mut depth = 0;
            while i < n {
                if c[i] == '/' && i + 1 < n && c[i + 1] == '*' {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if c[i] == '*' && i + 1 < n && c[i + 1] == '/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(c[i]));
                    i += 1;
                }
            }
            continue;
        }
        // raw string: r"..", r#".."#, br#".."#
        if (ch == 'r' || ((ch == 'b' || ch == 'c') && i + 1 < n && c[i + 1] == 'r'))
            && (i == 0 || !(c[i - 1].is_alphanumeric() || c[i - 1] == '_'))
        {
            let mut j = i + if ch == 'r' { 1 } else { 2 };
            let mut hashes = 0;
            while j < n && c[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && c[j] == '"' {
                for _ in i..=j {
                    out.push(' ');
                }
                j += 1;
                while j < n {
                    if c[j] == '"' && (0..hashes).all(|k| j + 1 + k < n && c[j + 1 + k] == '#') {
                        for _ in 0..=hashes {
                            out.push(' ');
                        }
                        j += 1 + hashes;
                        break;
                    }
                    out.push(blank(c[j]));
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        // normal string
        if ch == '"' {
            out.push(' ');
            i += 1;
            while i < n && c[i] != '"' {
                if c[i] == '\\' && i + 1 < n {
                    out.push(' ');
                    out.push(blank(c[i + 1]));
                    i += 2;
                } else {
                    out.push(blank(c[i]));
                    i += 1;
                }
            }
            if i < n {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // char literal (not a lifetime)
        if ch == '\'' {
            if i + 1 < n && c[i + 1] == '\\' {
                let mut j = i + 2;
                while j < n && c[j] != '\'' && c[j] != '\n' {
                    j += 1;
                }
                if j < n && c[j] == '\'' {
                    for _ in i..=j {
                        out.push(' ');
                    }
                    i = j + 1;
                    continue;
                }
            } else if i + 2 < n && c[i + 2] == '\'' && c[i + 1] != '\'' {
                out.push_str("   ");
                i += 3;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn is_ident(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Return (line, message) for each forbidden reference in `src`.
fn violations(src: &str) -> Vec<(usize, String)> {
    let code: Vec<char> = strip_non_code(src).chars().collect();
    let n = code.len();
    let line_at = |pos: usize| code[..pos].iter().filter(|&&c| c == '\n').count() + 1;
    let skip_ws = |mut p: usize| {
        while p < n && code[p].is_whitespace() {
            p += 1;
        }
        p
    };
    // Position after an identifier (a raw `r#` prefix is allowed), if `word` is next.
    let ident_at = |p: usize, word: &str| -> Option<usize> {
        let mut p = p;
        if p + 1 < n && code[p] == 'r' && code[p + 1] == '#' {
            p += 2;
        }
        let w: Vec<char> = word.chars().collect();
        if p + w.len() <= n
            && code[p..p + w.len()] == w[..]
            && !(p + w.len() < n && is_ident(code[p + w.len()]))
        {
            Some(p + w.len())
        } else {
            None
        }
    };
    let mut found = Vec::new();
    let mut i = 0;
    while i < n {
        if !(is_ident(code[i]) && (i == 0 || !is_ident(code[i - 1]))) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < n && is_ident(code[end]) {
            end += 1;
        }
        let word: String = code[i..end].iter().collect();
        // `iced` or `iced_*` as a whole identifier.
        if word == "iced" || word.starts_with("iced_") {
            found.push((line_at(i), format!("references `{word}`")));
        }
        // `crate::app`, `crate::{app::X}`, `crate::{a, app::X}`, raw `r#app`; any whitespace.
        if word == "crate" {
            let p = skip_ws(end);
            if p + 1 < n && code[p] == ':' && code[p + 1] == ':' {
                let p = skip_ws(p + 2);
                let mut hit = ident_at(p, "app").is_some();
                if !hit && p < n && code[p] == '{' {
                    // Scan the group: items at depth 1 start after `{` or `,`.
                    let (mut depth, mut q, mut item_start) = (0usize, p, true);
                    while q < n {
                        match code[q] {
                            '{' => {
                                depth += 1;
                                item_start = depth == 1;
                            }
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            ',' => item_start = depth == 1,
                            c if c.is_whitespace() => {}
                            _ => {
                                if item_start && depth == 1 && ident_at(q, "app").is_some() {
                                    hit = true;
                                    break;
                                }
                                item_start = false;
                            }
                        }
                        q += 1;
                    }
                }
                if hit {
                    found.push((line_at(i), "references `crate::app`".to_string()));
                }
            }
        }
        i = end;
    }
    found
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for e in entries {
            collect(&e, out);
        }
    } else if path.extension().map_or(false, |x| x == "rs") {
        out.push(path.to_path_buf());
    }
}

#[test]
fn model_layer_does_not_depend_on_ui() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for g in GUARDED {
        let p = root.join(g);
        assert!(
            p.exists(),
            "guarded path {g} does not exist; update tests/arch.rs"
        );
        collect(&p, &mut files);
    }
    assert!(!files.is_empty());
    let mut report = Vec::new();
    for f in &files {
        let src = fs::read_to_string(f).unwrap();
        for (line, msg) in violations(&src) {
            let rel = f.strip_prefix(root).unwrap_or(f);
            report.push(format!("{}:{line}: {msg}: {HINT}", rel.display()));
        }
    }
    assert!(
        report.is_empty(),
        "architecture violations:\n{}",
        report.join("\n")
    );
}

#[test]
fn scanner_catches_iced_use() {
    let v = violations("use iced::widget;\n");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].0, 1);
}

#[test]
fn scanner_catches_iced_subcrate_and_app() {
    assert_eq!(violations("fn f() {}\nuse iced_core::Color;\n")[0].0, 2);
    assert_eq!(violations("use crate::app::App;\n").len(), 1);
    assert_eq!(violations("use crate :: app::App;\n").len(), 1);
    assert_eq!(violations("use crate\n    ::app::App;\n").len(), 1);
    assert_eq!(violations("use crate::{app::App};\n").len(), 1);
    assert_eq!(violations("use crate::{parser, app::App};\n").len(), 1);
    assert_eq!(violations("use crate::r#app::App;\n").len(), 1);
    assert_eq!(violations("let x: iced::Color;\n").len(), 1);
}

#[test]
fn scanner_ignores_comments_strings_and_lookalikes() {
    let src = "/// uses iced_math here\n// use iced::x;\n/* crate::app /* nested iced */ */\n\
               let s = \"use iced::widget;\";\nlet r = r#\"crate::app\"#;\nlet c = '\\'';\n\
               fn f<'a>(x: &'a str) {}\nuse crate::application::X;\nlet serviced = 1;\n\
               let k = cr#\"a\" iced \"b\"#;\nuse crate::{parser::P, ast::{app_like}};\n";
    assert!(violations(src).is_empty(), "{:?}", violations(src));
}

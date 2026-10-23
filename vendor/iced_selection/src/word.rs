//! Word boundaries for double-click and word-drag selection (rmdv).
//!
//! UAX #29 has no dictionary for Chinese or Japanese, so it makes every
//! ideograph and every kana its own word. On macOS, each run of Chinese or
//! Japanese text is split again with Core Foundation's `CFStringTokenizer`
//! (`kCFStringTokenizerUnitWordBoundary`), which has one. Other text, and
//! every run on other platforms, keeps UAX #29.
//!
//! Positions are grapheme indices into one line, as in
//! [`SelectionEnd`](crate::selection::SelectionEnd).
//!
//! A word drag asks for the word around the pointer on every move, over the
//! same line (often a whole paragraph). The last few lines' words are kept,
//! and the system tokenizer is made once per thread, so a drag segments
//! each line once.

use std::cell::RefCell;
use std::rc::Rc;

use unicode_segmentation::UnicodeSegmentation;

/// One word or one run of spaces: graphemes `start..end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    start: usize,
    end: usize,
    blank: bool,
}

/// Whether `c` is written without spaces between words: Han ideographs,
/// kana, and their marks.
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3005..=0x3007          // 々 〆 〇
            | 0x3021..=0x3029    // Hangzhou numerals
            | 0x3031..=0x3035    // kana repeat marks
            | 0x303B..=0x303C    // 〻 〼
            | 0x3040..=0x309F    // Hiragana
            | 0x30A0..=0x30FF    // Katakana (with ー)
            | 0x31F0..=0x31FF    // Katakana phonetic extensions
            | 0x3400..=0x4DBF    // CJK Extension A
            | 0x4E00..=0x9FFF    // CJK Unified Ideographs
            | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
            | 0xFF66..=0xFF9F    // Halfwidth Katakana
            | 0x20000..=0x323AF // CJK Extensions B to H
    )
}

fn all_cjk(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_cjk)
}

/// Lines whose words are kept (a drag across blocks touches a few).
const KEPT_LINES: usize = 4;

thread_local! {
    /// The words of the lines segmented last, the latest first.
    static KEPT: RefCell<Vec<(String, Rc<[Segment]>)>> =
        const { RefCell::new(Vec::new()) };
}

/// [`segment`] for `text`, from the kept lines when it is one of them.
fn segments(text: &str) -> Rc<[Segment]> {
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        if let Some(at) = kept.iter().position(|(line, _)| line == text) {
            let hit = kept.remove(at);
            let segments = Rc::clone(&hit.1);
            kept.insert(0, hit);
            return segments;
        }
        let segments: Rc<[Segment]> = segment(text).into();
        kept.truncate(KEPT_LINES - 1);
        kept.insert(0, (text.to_owned(), Rc::clone(&segments)));
        segments
    })
}

/// The words and space runs of `text`, in order, covering all of it.
fn segment(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut at = 0;
    // A run of Chinese or Japanese words: its start in graphemes and bytes.
    let mut run: Option<(usize, usize)> = None;
    for (byte, word) in text.split_word_bound_indices() {
        let len = word.graphemes(true).count();
        if all_cjk(word) {
            if run.is_none() {
                run = Some((at, byte));
            }
        } else {
            if let Some((start, from)) = run.take() {
                split_run(&text[from..byte], start, &mut out);
            }
            out.push(Segment {
                start: at,
                end: at + len,
                blank: word.trim_start().is_empty(),
            });
        }
        at += len;
    }
    if let Some((start, from)) = run {
        split_run(&text[from..], start, &mut out);
    }
    out
}

/// Splits a run of Chinese or Japanese text that starts at grapheme
/// `start` into words.
fn split_run(run: &str, start: usize, out: &mut Vec<Segment>) {
    let bounds = dictionary_bounds(run).unwrap_or_else(|| {
        // UAX #29: every ideograph alone, katakana together.
        let mut at = 0;
        run.split_word_bounds()
            .map(|w| {
                at += w.graphemes(true).count();
                at
            })
            .collect()
    });
    let mut from = 0;
    for end in bounds {
        if end > from {
            out.push(Segment {
                start: start + from,
                end: start + end,
                blank: false,
            });
            from = end;
        }
    }
}

/// The word ends inside `run`, in graphemes from its start, by the system
/// dictionary; `None` where there is none.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn dictionary_bounds(run: &str) -> Option<Vec<usize>> {
    use objc2_core_foundation::{
        CFIndex, CFRange, CFRetained, CFString, CFStringTokenizer,
        CFStringTokenizerTokenType, kCFStringTokenizerUnitWordBoundary,
    };

    // UTF-16 offset of each grapheme boundary, the end included.
    let mut utf16 = vec![0usize];
    for g in run.graphemes(true) {
        let last = *utf16.last()?;
        utf16.push(last + g.encode_utf16().count());
    }
    let total = *utf16.last()?;
    let string = CFString::from_str(run);
    let range = CFRange {
        location: 0,
        length: CFIndex::try_from(total).ok()?,
    };
    thread_local! {
        /// Making a tokenizer costs far more than tokenizing a short run,
        /// so each thread makes one and points it at each run.
        static TOKENIZER: RefCell<Option<CFRetained<CFStringTokenizer>>> =
            const { RefCell::new(None) };
    }
    let tokenizer = TOKENIZER.with(|cell| {
        let mut cell = cell.borrow_mut();
        match cell.as_ref() {
            // SAFETY: `string` is a valid CFString and `range` lies
            // inside it.
            Some(tokenizer) => unsafe {
                tokenizer.set_string(Some(&string), range)
            },
            None => {
                // SAFETY: as above; a null allocator and locale select the
                // defaults.
                *cell = unsafe {
                    CFStringTokenizer::new(
                        None,
                        Some(&string),
                        range,
                        kCFStringTokenizerUnitWordBoundary,
                        None,
                    )
                };
            }
        }
        cell.clone()
    })?;
    let mut ends = Vec::new();
    while tokenizer.advance_to_next_token() != CFStringTokenizerTokenType::None
    {
        let token = tokenizer.current_token_range();
        let end = usize::try_from(token.location + token.length).ok()?;
        // A token end inside a grapheme moves to the grapheme's end.
        let grapheme = utf16.partition_point(|&offset| offset < end);
        ends.push(grapheme.min(utf16.len() - 1));
    }
    let last = utf16.len() - 1;
    if ends.last() != Some(&last) {
        ends.push(last);
    }
    Some(ends)
}

#[cfg(not(target_os = "macos"))]
fn dictionary_bounds(_run: &str) -> Option<Vec<usize>> {
    None
}

/// The word or space run that holds grapheme `grapheme` of `text` (the last
/// one when `grapheme` is past the end), as `(start, end)`.
pub fn word_around(text: &str, grapheme: usize) -> (usize, usize) {
    let segments = segments(text);
    segments
        .iter()
        .find(|s| s.start <= grapheme && grapheme < s.end)
        .or(segments.last())
        .map_or((grapheme, grapheme), |s| (s.start, s.end))
}

/// The start of the last word that begins before `index`, skipping spaces;
/// 0 when there is none.
pub fn previous_start_of_word(text: &str, index: usize) -> usize {
    segments(text)
        .iter()
        .rev()
        .find(|s| s.start < index && !s.blank)
        .map_or(0, |s| s.start)
}

/// The end of the first word that ends after `index`, skipping spaces; the
/// end of `text` when there is none.
pub fn next_end_of_word(text: &str, index: usize) -> usize {
    let segments = segments(text);
    segments
        .iter()
        .find(|s| s.end > index && !s.blank)
        .or(segments.last())
        .map_or(index, |s| s.end.max(index))
}

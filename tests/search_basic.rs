#[test]
fn finds_all_case_insensitive() {
    let m = rmdv::search::find_all("Hello hello HELLO", "hello");
    assert_eq!(m, vec![0, 6, 12]);
}

#[test]
fn empty_needle_returns_no_matches() {
    let m = rmdv::search::find_all("abc", "");
    assert!(m.is_empty());
}

#[test]
fn no_match_returns_empty() {
    let m = rmdv::search::find_all("abc", "xyz");
    assert!(m.is_empty());
}

#[test]
fn count_all_lowered_matches_find_all_len() {
    // Includes non-length-preserving lowercasing (İ, ẞ) and overlap-adjacent
    // repeats, the cases find_all's contract calls out.
    let cases: &[(&str, &str)] = &[
        ("Hello hello HELLO", "hello"),
        ("abc", "xyz"),
        ("abc", ""),
        ("aaaa", "aa"),
        ("İstanbul İzmir", "i\u{307}"),
        ("STRAẞE strasse", "ss"),
        ("ΣΣΣ", "σ"),
        ("mixed CASE Mixed case", "mixed"),
    ];
    for (h, n) in cases {
        assert_eq!(
            rmdv::search::count_all_lowered(h, &n.to_lowercase()),
            rmdv::search::find_all(h, n).len(),
            "haystack={h:?} needle={n:?}"
        );
    }
}

/// The original offset-map implementation, kept as the oracle for `find_all`.
fn find_all_reference(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let n = needle.to_lowercase();
    let mut h = String::new();
    let mut map = Vec::new();
    for (orig_off, ch) in haystack.char_indices() {
        for lc in ch.to_lowercase() {
            let before = h.len();
            h.push(lc);
            map.extend(std::iter::repeat(orig_off).take(h.len() - before));
        }
    }
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(idx) = h[start..].find(&n) {
        out.push(map[start + idx]);
        start += idx + n.len();
    }
    out
}

#[test]
fn find_all_matches_the_offset_map_reference() {
    // Deterministic strings over an alphabet that mixes ASCII with scalars whose
    // lowercase form changes length (İ, ẞ, Σ) or is multi-byte (é, 日).
    let alphabet = [
        'a', 'A', 's', 'S', 'i', 'İ', 'ẞ', 'Σ', 'σ', 'é', 'É', '日', ' ', '\n',
    ];
    let needles = [
        "a", "aa", "s", "ss", "i\u{307}", "σ", "é", "日a", "A S", "\n",
    ];
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    for len in 0..64 {
        for _ in 0..8 {
            let haystack: String = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    alphabet[(seed % alphabet.len() as u64) as usize]
                })
                .collect();
            for needle in needles {
                assert_eq!(
                    rmdv::search::find_all(&haystack, needle),
                    find_all_reference(&haystack, needle),
                    "haystack={haystack:?} needle={needle:?}"
                );
            }
        }
    }
}

#[test]
fn ascii_text_never_matches_a_non_ascii_needle() {
    assert!(rmdv::search::find_all("plain ascii text", "é").is_empty());
    assert_eq!(rmdv::search::find_all("Plain ASCII", "ascii"), vec![6]);
}

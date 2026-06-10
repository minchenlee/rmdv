#[test]
fn finds_all_case_insensitive() {
    let m = mdv::search::find_all("Hello hello HELLO", "hello");
    assert_eq!(m, vec![0, 6, 12]);
}

#[test]
fn empty_needle_returns_no_matches() {
    let m = mdv::search::find_all("abc", "");
    assert!(m.is_empty());
}

#[test]
fn no_match_returns_empty() {
    let m = mdv::search::find_all("abc", "xyz");
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
            mdv::search::count_all_lowered(h, &n.to_lowercase()),
            mdv::search::find_all(h, n).len(),
            "haystack={h:?} needle={n:?}"
        );
    }
}

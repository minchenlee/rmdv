use rmdv::highlight::HlCache;

#[test]
fn second_call_returns_same_spans_without_recomputing() {
    let mut c = HlCache::default();
    let a = c.highlight("rust", "fn main() {}");
    let len_after_first = c.len();
    let b = c.highlight("rust", "fn main() {}");
    assert_eq!(a, b);
    assert_eq!(c.len(), len_after_first, "cache should not grow on hit");
}

#[test]
fn changed_code_makes_new_entry() {
    let mut c = HlCache::default();
    c.highlight("rust", "fn main() {}");
    c.highlight("rust", "fn other() {}");
    assert_eq!(c.len(), 2);
}

#[test]
fn lru_evicts_past_capacity() {
    let mut c = HlCache::default();
    for i in 0..250 {
        c.highlight("rust", &format!("fn f{}() {{}}", i));
    }
    assert!(c.len() <= 200, "cache must respect CACHE_MAX");
}

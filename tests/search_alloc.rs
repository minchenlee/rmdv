//! Peak-allocation bound for `search::find_all`, which vault search runs over
//! whole files. This file owns its binary, so its counting allocator sees only
//! these tests; they run one at a time behind a lock.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(live, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Bytes allocated at peak while `f` runs, above what was live before it.
fn peak_extra(f: impl FnOnce()) -> usize {
    let base = LIVE.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    f();
    PEAK.load(Ordering::SeqCst) - base
}

const SIZE: usize = 8 * 1024 * 1024;

#[test]
fn ascii_search_allocates_at_most_one_copy() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut text = "lorem ipsum dolor sit amet\n".repeat(SIZE / 27);
    text.push_str("NEEDLE");
    let extra = peak_extra(|| {
        let hits = rmdv::search::find_all(&text, "needle");
        assert_eq!(hits.len(), 1);
    });
    assert!(
        extra <= SIZE + SIZE / 8,
        "peak extra {extra} bytes for {SIZE}"
    );
}

#[test]
fn unicode_search_allocates_at_most_one_copy() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut text = "Grüße aus Köln — ΣΣ 日本\n".repeat(SIZE / 32);
    text.push_str("NEEDLE");
    let extra = peak_extra(|| {
        let hits = rmdv::search::find_all(&text, "needle");
        assert_eq!(hits.len(), 1);
    });
    assert!(
        extra <= text.len() + text.len() / 8,
        "peak extra {extra} bytes for {}",
        text.len()
    );
}

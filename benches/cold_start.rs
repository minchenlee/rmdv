//! Measures cold-start cost of the heavy initialization functions we control.
//! Iced window creation isn't included (would require a real event loop); this
//! captures the parts mdv can actually optimize: font loading, parser warmup,
//! and the first markdown render path.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn font_load(c: &mut Criterion) {
    c.bench_function("font_system_load_system_fonts", |b| {
        b.iter(|| {
            let fs = iced::advanced::graphics::text::font_system();
            let mut guard = fs.write().unwrap();
            guard.raw().db_mut().load_system_fonts();
            black_box(());
        });
    });
}

fn parse_1mb(c: &mut Criterion) {
    let src = generate_doc(10_000);
    c.bench_function("parse_10k_lines", |b| {
        b.iter(|| {
            let (blocks, offsets) = mdv::parser::parse(black_box(&src));
            black_box((blocks, offsets));
        });
    });
}

fn generate_doc(lines: usize) -> String {
    let mut s = String::with_capacity(lines * 80);
    for i in 0..lines {
        if i % 50 == 0 {
            s.push_str(&format!("\n## Section {}\n\n", i / 50));
        } else if i % 10 == 0 {
            s.push_str("```rust\nfn hello() { println!(\"hi\"); }\n```\n\n");
        } else {
            s.push_str("This is a paragraph with **bold** and *italic* and a [link](https://example.com).\n");
        }
    }
    s
}

criterion_group!(benches, font_load, parse_1mb);
criterion_main!(benches);

//! Spike smoke test: `cargo run --example pdf_smoke --features pdf -- <file.pdf>`
//! Prints the extracted markdown. Proves PDFium dlopen + extraction work.
fn main() {
    let path = std::env::args().nth(1).expect("usage: pdf_smoke <file.pdf>");
    match rmdv::pdf::pdf_to_markdown(std::path::Path::new(&path)) {
        Ok(md) => {
            eprintln!("--- OK: {} chars of markdown ---", md.len());
            println!("{md}");
        }
        Err(e) => {
            eprintln!("--- ERR: {e} ---");
            std::process::exit(1);
        }
    }
}

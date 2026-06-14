use rmdv::ast::{Block, Inline};

#[test]
fn paragraph_holds_inlines() {
    let p = Block::Paragraph(vec![Inline::Text("hi".into())]);
    match p {
        Block::Paragraph(v) => assert_eq!(v.len(), 1),
        _ => panic!("not a paragraph"),
    }
}

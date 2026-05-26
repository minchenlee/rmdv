use mdv::ast::{Block, DiagramKind, Inline};
use mdv::parser::parse;

fn load() -> Vec<(mdv::ast::BlockId, Block)> {
    let src = std::fs::read_to_string("tests/fixtures/diagrams.md").unwrap();
    let (blocks, _offsets) = parse(&src);
    blocks
}

fn diagrams_of(blocks: &[(mdv::ast::BlockId, Block)], want: DiagramKind) -> Vec<(&String, u64)> {
    blocks
        .iter()
        .filter_map(|(_, b)| match b {
            Block::Diagram { kind, source, hash } if *kind == want => Some((source, *hash)),
            _ => None,
        })
        .collect()
}

#[test]
fn routes_mermaid_to_diagram() {
    let blocks = load();
    let mermaids = diagrams_of(&blocks, DiagramKind::Mermaid);
    assert_eq!(
        mermaids.len(),
        3,
        "expected 3 mermaid diagrams (flowchart, sequence, broken)"
    );

    let sources: Vec<&str> = mermaids.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        sources
            .iter()
            .any(|s| s.contains("graph LR") && s.contains("A --> B")),
        "flowchart source not found in {sources:?}"
    );
    assert!(
        sources
            .iter()
            .any(|s| s.contains("sequenceDiagram") && s.contains("Alice->>Bob")),
        "sequence source not found in {sources:?}"
    );
    assert!(
        sources
            .iter()
            .any(|s| s.contains("not actually mermaid syntax %%%")),
        "broken-mermaid source not found in {sources:?}"
    );
}

#[test]
fn routes_dot_and_graphviz_to_diagram() {
    let blocks = load();
    let dots = diagrams_of(&blocks, DiagramKind::Dot);
    assert_eq!(dots.len(), 2, "expected 2 dot diagrams (dot, graphviz)");

    let sources: Vec<&str> = dots.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        sources
            .iter()
            .any(|s| s.contains("digraph G") && s.contains("a -> b")),
        "dot source not found in {sources:?}"
    );
    assert!(
        sources
            .iter()
            .any(|s| s.contains("digraph H") && s.contains("x -> y")),
        "graphviz source not found in {sources:?}"
    );
}

#[test]
fn regular_code_block_unchanged() {
    let blocks = load();
    let has_rust = blocks.iter().any(|(_, b)| {
        matches!(
            b,
            Block::CodeBlock { lang: Some(l), .. } if l == "rust"
        )
    });
    assert!(has_rust, "expected at least one CodeBlock with lang=rust");

    // And make sure no rust-tagged block accidentally became a Diagram.
    let has_rust_diagram = blocks.iter().any(|(_, b)| {
        matches!(
            b,
            Block::Diagram { source, .. } if source.contains("fn main()")
        )
    });
    assert!(
        !has_rust_diagram,
        "rust code block must NOT be routed to Diagram"
    );
}

#[test]
fn diagram_hash_is_stable_for_same_source() {
    let a = load();
    let b = load();

    let diagrams_a: Vec<_> = a
        .iter()
        .filter_map(|(_, b)| match b {
            Block::Diagram { kind, source, hash } => Some((kind.clone(), source.clone(), *hash)),
            _ => None,
        })
        .collect();
    let diagrams_b: Vec<_> = b
        .iter()
        .filter_map(|(_, b)| match b {
            Block::Diagram { kind, source, hash } => Some((kind.clone(), source.clone(), *hash)),
            _ => None,
        })
        .collect();

    assert_eq!(diagrams_a.len(), diagrams_b.len());
    for (x, y) in diagrams_a.iter().zip(diagrams_b.iter()) {
        assert_eq!(x.0, y.0, "kind mismatch");
        assert_eq!(x.1, y.1, "source mismatch");
        assert_eq!(x.2, y.2, "hash not deterministic for source {:?}", x.1);
    }
}

#[test]
fn diagram_hash_differs_for_different_source() {
    let blocks = load();
    let mermaids = diagrams_of(&blocks, DiagramKind::Mermaid);

    let flowchart_hash = mermaids
        .iter()
        .find(|(s, _)| s.contains("graph LR"))
        .map(|(_, h)| *h)
        .expect("flowchart present");
    let sequence_hash = mermaids
        .iter()
        .find(|(s, _)| s.contains("sequenceDiagram"))
        .map(|(_, h)| *h)
        .expect("sequence present");

    assert_ne!(
        flowchart_hash, sequence_hash,
        "flowchart and sequence mermaid diagrams should hash differently"
    );
}

#[test]
fn display_math_preserves_paragraph_order() {
    let src = "This is display math:\n$$x$$\nAnd this follows";
    let (blocks, _offsets) = parse(src);
    let blocks: Vec<_> = blocks.into_iter().map(|(_, block)| block).collect();

    assert!(
        matches!(
            &blocks[..],
            [
                Block::Paragraph(before),
                Block::Diagram {
                    kind: DiagramKind::Math,
                    source,
                    ..
                },
                Block::Paragraph(after),
            ] if before == &vec![
                    Inline::Text("This is display math:".into()),
                    Inline::Text("\n".into()),
                ]
                && source == "x"
                && after == &vec![
                    Inline::Text("\n".into()),
                    Inline::Text("And this follows".into()),
                ]
        ),
        "unexpected blocks: {blocks:#?}"
    );
}

#[test]
fn display_math_split_blocks_keep_source_offsets() {
    let src = "A\n$$x$$\nB";
    let (blocks, offsets) = parse(src);
    let table = mdv::ipc::lines::build_byte_to_line(src);
    let lines: Vec<u32> = offsets
        .iter()
        .map(|&offset| table.line_for_byte(offset as usize))
        .collect();

    assert_eq!(blocks.len(), offsets.len());
    assert_eq!(offsets, vec![0, 2, 8]);
    assert_eq!(lines, vec![1, 2, 3]);
}

#[test]
fn display_math_stays_inside_list_items_and_blockquotes() {
    let src = "> - Before\n>   $$x$$\n>   After";
    let (blocks, _offsets) = parse(src);
    let blocks: Vec<_> = blocks.into_iter().map(|(_, block)| block).collect();

    assert!(
        matches!(
            &blocks[..],
            [
                Block::Blockquote(quote),
            ] if matches!(
                &quote[..],
                [
                    Block::List { items, .. },
                ] if items.len() == 1
                    && matches!(
                        &items[0].blocks[..],
                        [
                            Block::Paragraph(before),
                            Block::Diagram {
                                kind: DiagramKind::Math,
                                source,
                                ..
                            },
                            Block::Paragraph(after),
                        ] if before == &vec![
                                Inline::Text("Before".into()),
                                Inline::Text("\n".into()),
                            ]
                            && source == "x"
                            && after == &vec![
                                Inline::Text("\n".into()),
                                Inline::Text("After".into()),
                            ]
                    )
            )
        ),
        "unexpected blocks: {blocks:#?}"
    );
}

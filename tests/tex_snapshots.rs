use rmdv::ast::Block;
use rmdv::tex::parse;

#[test]
fn parses_academic_paper_subset() {
    let src = std::fs::read_to_string("tests/fixtures/paper.tex").unwrap();
    let (blocks, offsets) = parse(&src);
    insta::assert_yaml_snapshot!("paper", blocks);
    // Offset vec is one-per-block and monotonic (line-nav relies on this).
    assert_eq!(offsets.len(), blocks.len());
    assert!(offsets.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn align_environment_reaches_iced_math_with_wrapper() {
    // The whole point of the iced_math fix: align must arrive as a Math diagram
    // with its \begin{align} wrapper intact so the upstream renderer lays it out
    // as multiple rows instead of collapsing to one line.
    let src = "\\begin{align} a &= b \\\\ c &= d \\end{align}";
    let (blocks, _) = parse(src);
    let math = blocks.iter().find_map(|(_, b)| match b {
        Block::Diagram { source, .. } => Some(source.clone()),
        _ => None,
    });
    let math = math.expect("align should produce a Math diagram block");
    assert!(
        math.contains("\\begin{align}"),
        "align wrapper must be preserved verbatim, got: {math:?}"
    );
}

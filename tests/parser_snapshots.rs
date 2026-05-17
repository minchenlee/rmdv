use mdv::parser::parse;

#[test]
fn parses_basic_headings_and_paragraph() {
    let src = std::fs::read_to_string("tests/fixtures/basic.md").unwrap();
    let (blocks, _offsets) = parse(&src);
    insta::assert_yaml_snapshot!("basic", blocks);
}

#[test]
fn parses_gfm_constructs() {
    let src = std::fs::read_to_string("tests/fixtures/gfm.md").unwrap();
    let (blocks, _offsets) = mdv::parser::parse(&src);
    insta::assert_yaml_snapshot!("gfm", blocks);
}

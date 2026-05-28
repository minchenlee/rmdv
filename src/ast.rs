use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize)]
pub struct BlockId(pub u64);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Block {
    Heading {
        level: u8,
        id: String,
        inlines: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    CodeBlock {
        lang: Option<String>,
        code: String,
        spans: Vec<HlSpan>,
    },
    Blockquote(Vec<Block>),
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Image {
        url: String,
        alt: String,
    },
    Diagram {
        kind: DiagramKind,
        source: String,
        hash: u64,
    },
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum DiagramKind {
    Mermaid,
    Dot,
    Math,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { url: String, children: Vec<Inline> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ListItem {
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HlSpan {
    pub range: Range<usize>,
    pub style: HlStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HlStyle {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Operator,
    Constant,
    Variable,
    Punctuation,
}

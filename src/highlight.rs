use crate::ast::{HlSpan, HlStyle};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor};

/// Per-language cache of the parsed `Language` plus its **compiled** highlight
/// `Query`. Compiling a tree-sitter query (parsing the `.scm` DSL) is expensive;
/// caching the `Arc<Query>` turns per-call recompilation into a one-time cost.
type LangEntry = (Language, std::sync::Arc<Query>);

fn lang_cache() -> &'static Mutex<HashMap<String, Option<LangEntry>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<LangEntry>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn highlight(lang: &str, code: &str) -> Vec<HlSpan> {
    let key = lang.trim().to_ascii_lowercase();
    let entry = {
        let mut guard = lang_cache().lock().unwrap();
        guard
            .entry(key.clone())
            .or_insert_with(|| {
                let (language, queries) = lang_for(&key)?;
                let query = Query::new(&language, queries).ok()?;
                Some((language, std::sync::Arc::new(query)))
            })
            .clone()
    };
    let Some((language, query)) = entry else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(code, None) else {
        return Vec::new();
    };
    let mut cursor = QueryCursor::new();
    let mut out: Vec<HlSpan> = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = &query.capture_names()[cap.index as usize];
            let style = capture_to_style(name);
            if matches!(style, HlStyle::Plain) {
                continue;
            }
            let r = cap.node.byte_range();
            if r.start >= r.end {
                continue;
            }
            out.push(HlSpan { range: r, style });
        }
    }
    // Innermost / most specific capture wins: shorter ranges first at any start,
    // then earlier starts overall. The renderer's cursor walk drops anything that
    // overlaps a span already claimed.
    out.sort_by_key(|s| (s.range.start, s.range.end - s.range.start));
    out
}

/// C++ highlights = C base query + C++-specific query (the upstream `.scm`
/// files are split this way). Combined once and leaked to `'static`.
fn cpp_highlight_query() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    Q.get_or_init(|| {
        format!(
            "{}\n{}",
            tree_sitter_c::HIGHLIGHT_QUERY,
            tree_sitter_cpp::HIGHLIGHT_QUERY
        )
    })
    .as_str()
}

fn lang_for(name: &str) -> Option<(Language, &'static str)> {
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "rust" | "rs" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
        )),
        "python" | "py" => Some((
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
        )),
        "js" | "javascript" | "jsx" | "mjs" | "cjs" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        )),
        "ts" | "typescript" | "tsx" => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        )),
        "go" => Some((
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
        )),
        "c" | "h" => Some((
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
        )),
        "cpp" | "c++" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some((
            tree_sitter_cpp::LANGUAGE.into(),
            // The C++ grammar's highlight query only covers C++-specific nodes;
            // it inherits the C query for the shared subset. Concatenate both.
            cpp_highlight_query(),
        )),
        "java" => Some((
            tree_sitter_java::LANGUAGE.into(),
            tree_sitter_java::HIGHLIGHTS_QUERY,
        )),
        "sql" => Some((
            tree_sitter_sequel::LANGUAGE.into(),
            tree_sitter_sequel::HIGHLIGHTS_QUERY,
        )),
        "sh" | "bash" | "shell" | "zsh" => Some((
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
        )),
        "json" => Some((
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
        )),
        "html" | "htm" => Some((
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
        )),
        "md" | "markdown" => Some((
            tree_sitter_md::LANGUAGE.into(),
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        )),
        "yaml" | "yml" => Some((
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
        )),
        "toml" => Some((
            tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        )),
        _ => None,
    }
}

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

const CACHE_MAX: usize = 200;

#[derive(Default)]
pub struct HlCache {
    map: HashMap<(String, u64), Vec<HlSpan>>,
    order: VecDeque<(String, u64)>,
}

impl HlCache {
    pub fn highlight(&mut self, lang: &str, code: &str) -> Vec<HlSpan> {
        let mut h = DefaultHasher::new();
        code.hash(&mut h);
        let key = (lang.to_ascii_lowercase(), h.finish());
        if let Some(v) = self.map.get(&key) {
            return v.clone();
        }
        let spans = highlight(&key.0, code);
        if self.map.len() >= CACHE_MAX {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, spans.clone());
        spans
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

fn capture_to_style(name: &str) -> HlStyle {
    let base = name.split('.').next().unwrap_or(name);
    match base {
        "keyword" => HlStyle::Keyword,
        "type" => HlStyle::Type,
        "function" | "method" => HlStyle::Function,
        "string" => HlStyle::String,
        "number" => HlStyle::Number,
        "comment" => HlStyle::Comment,
        "operator" => HlStyle::Operator,
        "constant" => HlStyle::Constant,
        "variable" => HlStyle::Variable,
        "punctuation" => HlStyle::Punctuation,
        _ => HlStyle::Plain,
    }
}

#[cfg(test)]
mod ts_smoke {
    #[test]
    fn new_grammars_emit_spans() {
        for (lang, code) in [
            (
                "cpp",
                "#include <vector>\nint main() { std::vector<int> v; return 0; }",
            ),
            (
                "java",
                "public class A { public static void main(String[] a) { int x = 1; } }",
            ),
            (
                "sql",
                "SELECT id, name FROM users WHERE age > 18 ORDER BY name;",
            ),
        ] {
            let spans = super::highlight(lang, code);
            assert!(!spans.is_empty(), "{lang} produced no highlight spans");
        }
    }
}

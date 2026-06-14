use crate::ast::BlockId;
use crate::mindmap::{fit_label_for_node, MNode};
use iced::Size;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Normalized view over both JSON and YAML so one walker serves both.
/// `Scalar(text, is_string)` keeps the source type so a string value like `"1"`
/// renders quoted while a number `1` stays bare.
#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Scalar(String, bool),
    Array(Vec<DataValue>),
    Object(Vec<(String, DataValue)>),
}

/// One step from the document root toward a node's value.
#[derive(Debug, Clone, PartialEq)]
pub enum PathSeg {
    Key(String),
    Index(usize),
}

const MAX_DEPTH: usize = 64;

/// Returns (text, is_string) for a JSON scalar.
fn json_scalar(v: &serde_json::Value) -> (String, bool) {
    match v {
        serde_json::Value::String(s) => (s.clone(), true),
        serde_json::Value::Null => ("null".to_string(), false),
        other => (other.to_string(), false),
    }
}

pub(crate) fn from_json(v: &serde_json::Value) -> DataValue {
    match v {
        serde_json::Value::Array(a) => DataValue::Array(a.iter().map(from_json).collect()),
        serde_json::Value::Object(o) => {
            DataValue::Object(o.iter().map(|(k, val)| (k.clone(), from_json(val))).collect())
        }
        scalar => {
            let (text, is_str) = json_scalar(scalar);
            DataValue::Scalar(text, is_str)
        }
    }
}

/// Returns (text, is_string) for a YAML scalar.
fn yaml_scalar(v: &serde_yaml::Value) -> (String, bool) {
    match v {
        serde_yaml::Value::String(s) => (s.clone(), true),
        serde_yaml::Value::Bool(b) => (b.to_string(), false),
        serde_yaml::Value::Number(n) => (n.to_string(), false),
        serde_yaml::Value::Null => ("null".to_string(), false),
        // Tagged / sequence / mapping handled before this is called.
        _ => (String::new(), false),
    }
}

pub(crate) fn from_yaml(v: &serde_yaml::Value) -> DataValue {
    match v {
        serde_yaml::Value::Sequence(a) => DataValue::Array(a.iter().map(from_yaml).collect()),
        serde_yaml::Value::Mapping(m) => DataValue::Object(
            m.iter()
                .map(|(k, val)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s.clone(),
                        other => yaml_scalar(other).0,
                    };
                    (key, from_yaml(val))
                })
                .collect(),
        ),
        scalar => {
            let (text, is_str) = yaml_scalar(scalar);
            DataValue::Scalar(text, is_str)
        }
    }
}

/// Render a scalar for a label: strings get quotes, everything else stays bare.
fn scalar_label(text: &str, is_string: bool) -> String {
    if is_string {
        format!("\"{text}\"")
    } else {
        text.to_string()
    }
}

struct Builder<'a> {
    nodes: Vec<MNode>,
    paths: HashMap<BlockId, Vec<PathSeg>>,
    collapsed: &'a HashSet<BlockId>,
    next_id: u64,
}

impl<'a> Builder<'a> {
    fn mint(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Push a node, return its index. `full` is the untruncated label text.
    fn push(&mut self, full: String, level: u8, path: Vec<PathSeg>) -> usize {
        let id = self.mint();
        let (label, truncated) = fit_label_for_node(&full);
        self.paths.insert(id, path);
        let idx = self.nodes.len();
        self.nodes.push(MNode {
            id: Some(id),
            label,
            full_label: full,
            truncated,
            level,
            children: Vec::new(),
            has_hidden_children: false,
            x: 0.0,
            y: 0.0,
        });
        idx
    }

    /// Walk `value`, attaching produced child nodes to `parent_idx`.
    fn walk_children(&mut self, parent_idx: usize, value: &DataValue, level: u8, path: &[PathSeg]) {
        if level as usize >= MAX_DEPTH {
            let idx = self.push("…".to_string(), level, path.to_vec());
            self.nodes[parent_idx].children.push(idx);
            return;
        }
        match value {
            DataValue::Object(fields) => {
                for (key, val) in fields {
                    let mut child_path = path.to_vec();
                    child_path.push(PathSeg::Key(key.clone()));
                    self.add_member(parent_idx, key.clone(), val, level, child_path);
                }
            }
            DataValue::Array(elems) => {
                for (i, val) in elems.iter().enumerate() {
                    let mut child_path = path.to_vec();
                    child_path.push(PathSeg::Index(i));
                    self.add_member(parent_idx, format!("[{i}]"), val, level, child_path);
                }
            }
            DataValue::Scalar(..) => {} // scalars attached by add_member, never recursed
        }
    }

    /// Attach one member (object value or array element) under `parent_idx`.
    /// `head` is the key text or `[i]` index label.
    fn add_member(
        &mut self,
        parent_idx: usize,
        head: String,
        val: &DataValue,
        level: u8,
        path: Vec<PathSeg>,
    ) {
        match val {
            DataValue::Scalar(text, is_string) => {
                let full = format!("{head}: {}", scalar_label(text, *is_string));
                let idx = self.push(full, level + 1, path);
                self.nodes[parent_idx].children.push(idx);
            }
            DataValue::Object(_) | DataValue::Array(_) => {
                let idx = self.push(head, level + 1, path.clone());
                self.nodes[parent_idx].children.push(idx);
                if self.collapsed.contains(&self.nodes[idx].id.unwrap()) {
                    self.nodes[idx].has_hidden_children = true;
                } else {
                    self.walk_children(idx, val, level + 1, &path);
                }
            }
        }
    }
}

pub fn build_tree(
    root: &DataValue,
    doc_title: &str,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>) {
    let mut b = Builder {
        nodes: Vec::new(),
        paths: HashMap::new(),
        collapsed,
        next_id: 0,
    };
    let root_id = b.mint();
    let (label, truncated) = fit_label_for_node(doc_title);
    b.paths.insert(root_id, Vec::new());
    b.nodes.push(MNode {
        id: Some(root_id),
        label,
        full_label: doc_title.to_string(),
        truncated,
        level: 0,
        children: Vec::new(),
        has_hidden_children: false,
        x: 0.0,
        y: 0.0,
    });
    b.walk_children(0, root, 0, &[]);
    (b.nodes, b.paths)
}

fn title_for(file: Option<&Path>) -> String {
    file.and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

/// Root + a single warning child. Used for parse errors and unsupported langs.
fn fallback(title: &str, warning: &str) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>) {
    let mut paths = HashMap::new();
    let (rl, rt) = fit_label_for_node(title);
    paths.insert(BlockId(0), Vec::new());
    let (wl, wt) = fit_label_for_node(warning);
    paths.insert(BlockId(1), Vec::new());
    let nodes = vec![
        MNode {
            id: Some(BlockId(0)),
            label: rl,
            full_label: title.to_string(),
            truncated: rt,
            level: 0,
            children: vec![1],
            has_hidden_children: false,
            x: 0.0,
            y: 0.0,
        },
        MNode {
            id: Some(BlockId(1)),
            label: wl,
            full_label: warning.to_string(),
            truncated: wt,
            level: 1,
            children: Vec::new(),
            has_hidden_children: false,
            x: 0.0,
            y: 0.0,
        },
    ];
    (nodes, paths)
}

fn parse_to_value(source: &str, lang: &str) -> Option<DataValue> {
    match lang {
        "json" => serde_json::from_str::<serde_json::Value>(source)
            .ok()
            .map(|v| from_json(&v)),
        "yaml" => serde_yaml::from_str::<serde_yaml::Value>(source)
            .ok()
            .map(|v| from_yaml(&v)),
        _ => None,
    }
}

pub fn build_layout(
    source: &str,
    lang: &str,
    file: Option<&Path>,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, Size, HashMap<BlockId, Vec<PathSeg>>) {
    let title = title_for(file);
    let (mut nodes, paths) = match lang {
        "json" | "yaml" => match parse_to_value(source, lang) {
            Some(v) => build_tree(&v, &title, collapsed),
            None => fallback(&title, &format!("⚠ invalid {lang}")),
        },
        other => fallback(&title, &format!("⚠ {other} mindmap not supported")),
    };
    let mut y_cursor: f32 = crate::mindmap::PAD;
    crate::mindmap::layout(&mut nodes, 0, &mut y_cursor);
    let max_level = nodes.iter().map(|n| n.level).max().unwrap_or(0) as f32;
    let width = crate::mindmap::PAD * 2.0
        + crate::mindmap::NODE_W
        + max_level * (crate::mindmap::NODE_W + crate::mindmap::X_GAP);
    let height = y_cursor + crate::mindmap::PAD;
    (nodes, Size::new(width, height), paths)
}

/// Navigate `path` into the parsed source and pretty-print that subtree.
pub fn subtree_pretty(source: &str, lang: &str, path: &[PathSeg]) -> Option<String> {
    match lang {
        "json" => {
            let root: serde_json::Value = serde_json::from_str(source).ok()?;
            let mut cur = &root;
            for seg in path {
                cur = match seg {
                    PathSeg::Key(k) => cur.get(k)?,
                    PathSeg::Index(i) => cur.get(i)?,
                };
            }
            match cur {
                serde_json::Value::String(s) => Some(s.clone()),
                other => serde_json::to_string_pretty(other).ok(),
            }
        }
        "yaml" => {
            let root: serde_yaml::Value = serde_yaml::from_str(source).ok()?;
            let mut cur = &root;
            for seg in path {
                cur = match seg {
                    PathSeg::Key(k) => cur.get(serde_yaml::Value::String(k.clone()))?,
                    PathSeg::Index(i) => cur.get(*i)?,
                };
            }
            match cur {
                serde_yaml::Value::String(s) => Some(s.clone()),
                other => serde_yaml::to_string(other).ok(),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(src: &str) -> DataValue {
        from_json(&serde_json::from_str(src).unwrap())
    }

    #[test]
    fn json_and_yaml_normalize_alike() {
        let j: serde_json::Value =
            serde_json::from_str(r#"{"name":"rmdv","tags":["rust","gui"]}"#).unwrap();
        let y: serde_yaml::Value =
            serde_yaml::from_str("name: rmdv\ntags:\n  - rust\n  - gui\n").unwrap();
        assert_eq!(from_json(&j), from_yaml(&y));
    }

    #[test]
    fn scalars_stringify() {
        let j: serde_json::Value = serde_json::from_str(r#"{"n":42,"b":true,"z":null}"#).unwrap();
        match from_json(&j) {
            DataValue::Object(fields) => {
                assert_eq!(fields[0], ("n".into(), DataValue::Scalar("42".into(), false)));
                assert_eq!(fields[1], ("b".into(), DataValue::Scalar("true".into(), false)));
                assert_eq!(fields[2], ("z".into(), DataValue::Scalar("null".into(), false)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn nested_object_labels_and_children() {
        let v = obj(r#"{"name":"rmdv","deps":{"serde":"1"}}"#);
        let (nodes, _paths) = build_tree(&v, "rmdv.json", &HashSet::new());
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].full_label, "rmdv.json");
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[1].full_label, r#"name: "rmdv""#);
        assert_eq!(nodes[2].full_label, "deps");
        assert_eq!(nodes[2].children.len(), 1);
        assert_eq!(nodes[3].full_label, r#"serde: "1""#);
    }

    #[test]
    fn array_of_scalars_indexed() {
        let v = obj(r#"{"tags":["rust","gui"]}"#);
        let (nodes, _) = build_tree(&v, "f.json", &HashSet::new());
        let tags = &nodes[1];
        assert_eq!(tags.full_label, "tags");
        assert_eq!(tags.children.len(), 2);
        assert_eq!(nodes[tags.children[0]].full_label, r#"[0]: "rust""#);
        assert_eq!(nodes[tags.children[1]].full_label, r#"[1]: "gui""#);
    }

    #[test]
    fn array_of_objects_recurses() {
        let v = obj(r#"{"items":[{"a":1},{"b":2}]}"#);
        let (nodes, _) = build_tree(&v, "f.json", &HashSet::new());
        let items = &nodes[1];
        assert_eq!(items.children.len(), 2);
        let first = &nodes[items.children[0]];
        assert_eq!(first.full_label, "[0]");
        assert_eq!(first.children.len(), 1);
        assert_eq!(nodes[first.children[0]].full_label, "a: 1");
    }

    #[test]
    fn empty_object_root_only() {
        let (nodes, _) = build_tree(&obj("{}"), "f.json", &HashSet::new());
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].children.is_empty());
    }

    #[test]
    fn blockid_sequence_is_stable() {
        let v = obj(r#"{"a":1,"b":{"c":2}}"#);
        let (n1, _) = build_tree(&v, "f.json", &HashSet::new());
        let (n2, _) = build_tree(&v, "f.json", &HashSet::new());
        let ids1: Vec<_> = n1.iter().map(|n| n.id).collect();
        let ids2: Vec<_> = n2.iter().map(|n| n.id).collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn path_map_round_trips_scalar() {
        let v = obj(r#"{"deps":{"serde":"1"}}"#);
        let (nodes, paths) = build_tree(&v, "f.json", &HashSet::new());
        let leaf = nodes
            .iter()
            .find(|n| n.full_label.starts_with("serde"))
            .unwrap();
        let path = paths.get(&leaf.id.unwrap()).unwrap();
        assert_eq!(
            path,
            &vec![PathSeg::Key("deps".into()), PathSeg::Key("serde".into())]
        );
    }

    #[test]
    fn collapsed_node_hides_children() {
        let v = obj(r#"{"deps":{"serde":"1","iced":"0.14"}}"#);
        let (full, _) = build_tree(&v, "f.json", &HashSet::new());
        let deps_id = full
            .iter()
            .find(|n| n.full_label == "deps")
            .unwrap()
            .id
            .unwrap();
        let mut collapsed = HashSet::new();
        collapsed.insert(deps_id);
        let (nodes, _) = build_tree(&v, "f.json", &collapsed);
        let deps = nodes.iter().find(|n| n.full_label == "deps").unwrap();
        assert!(deps.children.is_empty());
        assert!(deps.has_hidden_children);
    }

    #[test]
    fn build_layout_json_ok() {
        let (nodes, size, paths) = build_layout(r#"{"a":1,"b":2}"#, "json", None, &HashSet::new());
        assert_eq!(nodes.len(), 3);
        assert!(size.width > 0.0 && size.height > 0.0);
        assert_eq!(paths.len(), 3);
        assert!(nodes[0].x >= 0.0);
    }

    #[test]
    fn build_layout_yaml_ok() {
        let (nodes, _, _) = build_layout("a: 1\nb: 2\n", "yaml", None, &HashSet::new());
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn build_layout_malformed_falls_back() {
        let (nodes, _, _) = build_layout("{not valid", "json", None, &HashSet::new());
        assert_eq!(nodes.len(), 2);
        assert!(nodes[1].full_label.contains("invalid"));
    }

    #[test]
    fn build_layout_unsupported_lang_falls_back() {
        let (nodes, _, _) = build_layout("a = 1", "toml", None, &HashSet::new());
        assert_eq!(nodes.len(), 2);
        assert!(nodes[1].full_label.contains("not supported"));
    }

    #[test]
    fn subtree_pretty_object_and_scalar() {
        let src = r#"{"deps":{"serde":"1"}}"#;
        let obj_path = vec![PathSeg::Key("deps".into())];
        let pretty = subtree_pretty(src, "json", &obj_path).unwrap();
        assert!(pretty.contains("serde"));
        let scalar_path = vec![PathSeg::Key("deps".into()), PathSeg::Key("serde".into())];
        let s = subtree_pretty(src, "json", &scalar_path).unwrap();
        assert!(s.contains('1'));
    }

    #[test]
    fn subtree_pretty_bad_path_is_none() {
        let src = r#"{"a":1}"#;
        assert!(subtree_pretty(src, "json", &[PathSeg::Key("nope".into())]).is_none());
    }
}

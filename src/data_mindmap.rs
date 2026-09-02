use crate::ast::BlockId;
use crate::mindmap::{fit_label_for_node, MNode};
use iced::Size;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Normalized view over JSON, YAML, and TOML so one walker serves all three.
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
        serde_json::Value::Object(o) => DataValue::Object(
            o.iter()
                .map(|(k, val)| (k.clone(), from_json(val)))
                .collect(),
        ),
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
    explicit_collapsed: Option<&'a HashSet<BlockId>>,
    collapse_depth: Option<u8>,
    depth_collapsed: HashSet<BlockId>,
    next_id: u64,
}

fn has_rendered_children(value: &DataValue, level: u8) -> bool {
    if level as usize >= MAX_DEPTH {
        return true;
    }
    match value {
        DataValue::Object(fields) => !fields.is_empty(),
        DataValue::Array(elements) => !elements.is_empty(),
        DataValue::Scalar(..) => false,
    }
}

impl Builder<'_> {
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

    fn should_collapse(&self, id: BlockId, level: u8) -> bool {
        self.explicit_collapsed
            .is_some_and(|collapsed| collapsed.contains(&id))
            || self
                .collapse_depth
                .is_some_and(|depth| depth > 0 && level >= depth)
    }

    /// Advance structural IDs through a hidden subtree without allocating
    /// `MNode`s or preview paths. Depth folding still records every branching
    /// descendant so expanding one frontier node preserves the requested depth.
    fn skip_children(&mut self, value: &DataValue, level: u8) {
        if level as usize >= MAX_DEPTH {
            let _ = self.mint();
            return;
        }
        match value {
            DataValue::Object(fields) => {
                for (_, value) in fields {
                    self.skip_member(value, level);
                }
            }
            DataValue::Array(elements) => {
                for value in elements {
                    self.skip_member(value, level);
                }
            }
            DataValue::Scalar(..) => {}
        }
    }

    fn skip_member(&mut self, value: &DataValue, level: u8) {
        let id = self.mint();
        if matches!(value, DataValue::Object(_) | DataValue::Array(_)) {
            let node_level = level.saturating_add(1);
            if self
                .collapse_depth
                .is_some_and(|depth| depth > 0 && node_level >= depth)
                && has_rendered_children(value, node_level)
            {
                self.depth_collapsed.insert(id);
            }
            self.skip_children(value, node_level);
        }
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
                let node_level = level.saturating_add(1);
                let idx = self.push(head, node_level, path.clone());
                self.nodes[parent_idx].children.push(idx);
                let id = self.nodes[idx].id.expect("data mindmap nodes have IDs");
                if self.should_collapse(id, node_level) && has_rendered_children(val, node_level) {
                    self.nodes[idx].has_hidden_children = true;
                    if self.collapse_depth.is_some() {
                        self.depth_collapsed.insert(id);
                    }
                    self.skip_children(val, node_level);
                } else {
                    self.walk_children(idx, val, node_level, &path);
                }
            }
        }
    }
}

fn build_tree_with_policy(
    root: &DataValue,
    doc_title: &str,
    explicit_collapsed: Option<&HashSet<BlockId>>,
    collapse_depth: Option<u8>,
) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>, HashSet<BlockId>) {
    let mut b = Builder {
        nodes: Vec::new(),
        paths: HashMap::new(),
        explicit_collapsed,
        collapse_depth,
        depth_collapsed: HashSet::new(),
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
    if b.should_collapse(root_id, 0) && has_rendered_children(root, 0) {
        b.nodes[0].has_hidden_children = true;
        b.skip_children(root, 0);
    } else {
        b.walk_children(0, root, 0, &[]);
    }
    (b.nodes, b.paths, b.depth_collapsed)
}

pub fn build_tree(
    root: &DataValue,
    doc_title: &str,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>) {
    let (nodes, paths, _) = build_tree_with_policy(root, doc_title, Some(collapsed), None);
    (nodes, paths)
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
        "toml" => toml::from_str::<toml::Value>(source)
            .ok()
            .map(|v| from_toml(&v)),
        _ => None,
    }
}

pub(crate) fn from_toml(value: &toml::Value) -> DataValue {
    match value {
        toml::Value::String(value) => DataValue::Scalar(value.clone(), true),
        toml::Value::Integer(value) => DataValue::Scalar(value.to_string(), false),
        toml::Value::Float(value) => DataValue::Scalar(value.to_string(), false),
        toml::Value::Boolean(value) => DataValue::Scalar(value.to_string(), false),
        toml::Value::Datetime(value) => DataValue::Scalar(value.to_string(), false),
        toml::Value::Array(values) => DataValue::Array(values.iter().map(from_toml).collect()),
        toml::Value::Table(values) => DataValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), from_toml(value)))
                .collect(),
        ),
    }
}

pub fn build_layout(
    source: &str,
    lang: &str,
    file: Option<&Path>,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, Size, HashMap<BlockId, Vec<PathSeg>>) {
    let title = title_for(file);
    let (nodes, paths) = match lang {
        "json" | "yaml" | "toml" => match parse_to_value(source, lang) {
            Some(v) => build_tree(&v, &title, collapsed),
            None => fallback(&title, &format!("⚠ invalid {lang}")),
        },
        other => fallback(&title, &format!("⚠ {other} mindmap not supported")),
    };
    let (nodes, size) = finish_layout(nodes);
    (nodes, size, paths)
}

fn finish_layout(mut nodes: Vec<MNode>) -> (Vec<MNode>, Size) {
    let mut y_cursor: f32 = crate::mindmap::PAD;
    crate::mindmap::layout(&mut nodes, 0, &mut y_cursor);
    let max_level = nodes.iter().map(|n| n.level).max().unwrap_or(0) as f32;
    let width = crate::mindmap::PAD * 2.0
        + crate::mindmap::NODE_W
        + max_level * (crate::mindmap::NODE_W + crate::mindmap::X_GAP);
    let height = y_cursor + crate::mindmap::PAD;
    (nodes, Size::new(width, height))
}

/// Build and lay out a depth-folded graph in one parse/walk. Hidden descendants
/// advance structural IDs but do not allocate `MNode`s or preview paths.
pub fn build_layout_for_depth(
    source: &str,
    lang: &str,
    file: Option<&Path>,
    depth: u8,
) -> (
    Vec<MNode>,
    Size,
    HashMap<BlockId, Vec<PathSeg>>,
    HashSet<BlockId>,
) {
    let title = title_for(file);
    let (nodes, paths, collapsed) = match lang {
        "json" | "yaml" | "toml" => match parse_to_value(source, lang) {
            Some(value) => build_tree_with_policy(&value, &title, None, Some(depth)),
            None => {
                let (nodes, paths) = fallback(&title, &format!("⚠ invalid {lang}"));
                (nodes, paths, HashSet::new())
            }
        },
        other => {
            let (nodes, paths) = fallback(&title, &format!("⚠ {other} mindmap not supported"));
            (nodes, paths, HashSet::new())
        }
    };
    let (nodes, size) = finish_layout(nodes);
    (nodes, size, paths, collapsed)
}

/// Collapse every branching node at or below `depth`, preserving the root at
/// level 0. Prefer `build_layout_for_depth` when the folded layout is also
/// needed so parsing and graph construction happen only once.
pub fn collapsed_for_depth(
    source: &str,
    lang: &str,
    file: Option<&Path>,
    depth: u8,
) -> HashSet<BlockId> {
    let (_, _, _, collapsed) = build_layout_for_depth(source, lang, file, depth);
    collapsed
}

/// Look up a YAML mapping value by the *stringified* form of its key.
///
/// `PathSeg::Key` stores keys as the same string `from_yaml`/`yaml_scalar`
/// produced for the node label, so a non-string YAML key (`42:`, `true:`) is
/// recorded as `"42"`/`"true"`. A direct `Value::String` lookup would miss it,
/// so first try the fast string-key path, then fall back to scanning the
/// mapping for a key whose scalar text matches.
fn yaml_get_by_str_key<'a>(cur: &'a serde_yaml::Value, k: &str) -> Option<&'a serde_yaml::Value> {
    if let Some(v) = cur.get(serde_yaml::Value::String(k.to_string())) {
        return Some(v);
    }
    let map = cur.as_mapping()?;
    map.iter().find_map(|(key, val)| {
        let matches = match key {
            serde_yaml::Value::String(s) => s == k,
            other => yaml_scalar(other).0 == k,
        };
        matches.then_some(val)
    })
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
                    PathSeg::Key(k) => yaml_get_by_str_key(cur, k)?,
                    PathSeg::Index(i) => cur.get(*i)?,
                };
            }
            match cur {
                serde_yaml::Value::String(s) => Some(s.clone()),
                other => serde_yaml::to_string(other).ok(),
            }
        }
        "toml" => {
            let root: toml::Value = toml::from_str(source).ok()?;
            let mut cur = &root;
            for seg in path {
                cur = match seg {
                    PathSeg::Key(key) => cur.get(key)?,
                    PathSeg::Index(index) => cur.get(*index)?,
                };
            }
            match cur {
                toml::Value::String(value) => Some(value.clone()),
                toml::Value::Array(_) => Some(cur.to_string()),
                toml::Value::Table(_) => toml::to_string_pretty(cur).ok(),
                scalar => Some(scalar.to_string()),
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
    fn json_yaml_and_toml_normalize_alike() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"name":"rmdv","nested":{"enabled":true}}"#).unwrap();
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("name: rmdv\nnested:\n  enabled: true\n").unwrap();
        let toml: toml::Value =
            toml::from_str("name = \"rmdv\"\n[nested]\nenabled = true\n").unwrap();
        assert_eq!(from_json(&json), from_yaml(&yaml));
        assert_eq!(from_json(&json), from_toml(&toml));
    }

    #[test]
    fn scalars_stringify() {
        let j: serde_json::Value = serde_json::from_str(r#"{"n":42,"b":true,"z":null}"#).unwrap();
        match from_json(&j) {
            DataValue::Object(fields) => {
                assert_eq!(
                    fields[0],
                    ("n".into(), DataValue::Scalar("42".into(), false))
                );
                assert_eq!(
                    fields[1],
                    ("b".into(), DataValue::Scalar("true".into(), false))
                );
                assert_eq!(
                    fields[2],
                    ("z".into(), DataValue::Scalar("null".into(), false))
                );
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
    fn collapsed_root_hides_children_without_retaining_hidden_paths() {
        let value = obj(r#"{"left":{"deep":1},"right":2}"#);
        let (full, _) = build_tree(&value, "f.json", &HashSet::new());
        let full_ids = full.iter().filter_map(|node| node.id).collect::<Vec<_>>();

        let (collapsed, paths) = build_tree(&value, "f.json", &HashSet::from([BlockId(0)]));

        assert_eq!(collapsed.len(), 1);
        assert!(collapsed[0].children.is_empty());
        assert!(collapsed[0].has_hidden_children);
        assert_eq!(paths.keys().copied().collect::<Vec<_>>(), vec![BlockId(0)]);

        let (expanded, _) = build_tree(&value, "f.json", &HashSet::new());
        assert_eq!(
            expanded
                .iter()
                .filter_map(|node| node.id)
                .collect::<Vec<_>>(),
            full_ids
        );
    }

    #[test]
    fn collapsing_an_earlier_branch_preserves_later_sibling_identity() {
        let value = obj(r#"{"left":{"deep":1},"right":{"deep":2}}"#);
        let (full, _) = build_tree(&value, "f.json", &HashSet::new());
        let left_id = full
            .iter()
            .find(|node| node.full_label == "left")
            .and_then(|node| node.id)
            .unwrap();
        let right_id = full
            .iter()
            .find(|node| node.full_label == "right")
            .and_then(|node| node.id)
            .unwrap();
        let hidden_left_id = full
            .iter()
            .find(|node| node.full_label == "deep: 1")
            .and_then(|node| node.id)
            .unwrap();

        let (collapsed, paths) = build_tree(&value, "f.json", &HashSet::from([left_id]));

        assert_eq!(
            collapsed
                .iter()
                .find(|node| node.full_label == "right")
                .and_then(|node| node.id),
            Some(right_id)
        );
        assert_eq!(paths.len(), collapsed.len());
        assert!(!paths.contains_key(&hidden_left_id));
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
    fn build_layout_toml_recurses() {
        let (nodes, _, _) = build_layout(
            "name = \"rmdv\"\n[deps]\niced = \"0.14\"\n",
            "toml",
            None,
            &HashSet::new(),
        );
        assert!(nodes.iter().any(|node| node.full_label == "deps"));
        assert!(nodes.iter().any(|node| node.full_label == "iced: \"0.14\""));
    }

    #[test]
    fn data_depth_collapse_keeps_root_and_requested_levels() {
        for (source, lang) in [
            (r#"{"a":{"b":{"c":1}}}"#, "json"),
            ("a:\n  b:\n    c: 1\n", "yaml"),
            ("[a.b]\nc = 1\n", "toml"),
        ] {
            let (nodes, _, paths, collapsed) = build_layout_for_depth(source, lang, None, 1);
            assert_eq!(nodes.iter().map(|node| node.level).max(), Some(1), "{lang}");
            assert_eq!(paths.len(), nodes.len(), "{lang} visible preview paths");
            assert_eq!(collapsed.len(), 2, "{lang} collapsed branching nodes");
            assert!(collapsed_for_depth(source, lang, None, 0).is_empty());
        }
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
    fn subtree_pretty_toml_object_and_scalar() {
        let source = "[deps]\niced = \"0.14\"\nmajor = 1\n";
        let object = subtree_pretty(source, "toml", &[PathSeg::Key("deps".into())]).unwrap();
        assert!(object.contains("iced = \"0.14\""));
        assert_eq!(
            subtree_pretty(
                source,
                "toml",
                &[PathSeg::Key("deps".into()), PathSeg::Key("iced".into())]
            )
            .as_deref(),
            Some("0.14")
        );
        assert_eq!(
            subtree_pretty(
                source,
                "toml",
                &[PathSeg::Key("deps".into()), PathSeg::Key("major".into())]
            )
            .as_deref(),
            Some("1")
        );
    }

    #[test]
    fn subtree_pretty_toml_array() {
        let source = "tags = [\"rust\", \"gui\"]\n";
        let preview = subtree_pretty(source, "toml", &[PathSeg::Key("tags".into())])
            .expect("TOML array nodes should produce preview content");
        assert!(preview.contains("rust"));
        assert!(preview.contains("gui"));
    }

    #[test]
    fn subtree_pretty_bad_path_is_none() {
        let src = r#"{"a":1}"#;
        assert!(subtree_pretty(src, "json", &[PathSeg::Key("nope".into())]).is_none());
    }

    #[test]
    fn subtree_pretty_yaml_non_string_keys() {
        // Integer and boolean keys must resolve via stringified-key fallback.
        let src = "42:\n  nested: ok\ntrue: yes\n";
        let int_path = vec![PathSeg::Key("42".into()), PathSeg::Key("nested".into())];
        let v = subtree_pretty(src, "yaml", &int_path).unwrap();
        assert!(
            v.contains("ok"),
            "int-keyed subtree should resolve, got {v:?}"
        );
        let bool_path = vec![PathSeg::Key("true".into())];
        let b = subtree_pretty(src, "yaml", &bool_path).unwrap();
        assert!(
            b.contains("yes"),
            "bool-keyed subtree should resolve, got {b:?}"
        );
    }
}

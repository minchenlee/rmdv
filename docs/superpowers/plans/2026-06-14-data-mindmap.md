# JSON/YAML Mind Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pressing ⌘M on a `.json`/`.yaml`/`.yml` file renders a structural mind map of the data, reusing the existing mind map canvas, navigation, zoom, collapse, and a leaf panel that shows each node's pretty-printed subtree.

**Architecture:** A new `src/data_mindmap.rs` module normalizes `serde_json::Value` and `serde_yaml::Value` into one `DataValue` enum, then walks it into the flat `Vec<MNode>` arena the existing mind map renderer already consumes (minting sequential `BlockId`s for stable nav/collapse), plus a `BlockId → path` map so the leaf panel can re-navigate the parsed value on demand. `App::mindmap_layout` dispatches on `is_data_doc`; `App::mindmap_panel_view` renders the subtree for data docs.

**Tech Stack:** Rust, Iced 0.14 canvas, serde_json 1, serde_yaml 0.9. Release builds only (`cargo build --release`, `cargo test --release`).

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `src/data_mindmap.rs` | JSON/YAML → `DataValue` → `Vec<MNode>` + path map; `subtree_pretty` | **Create** |
| `src/lib.rs` | module registration | Modify (add `pub mod data_mindmap;`) |
| `src/mindmap.rs` | expose `layout` + `fit_label_for_node` as `pub(crate)` for reuse | Modify |
| `src/app.rs` | dispatch in `mindmap_layout`; data branch in `mindmap_panel_view`; cache field + path map | Modify |

---

## Task 1: Expose mindmap helpers for reuse

The data converter reuses `mindmap::layout` (x/y assignment) and `fit_label_for_node` (label truncation). Both are currently private. Widen to `pub(crate)`. No behavior change.

**Files:**
- Modify: `src/mindmap.rs:126` (`fit_label_for_node`), `src/mindmap.rs:214` (`layout`)

- [ ] **Step 1: Widen `fit_label_for_node` visibility**

In `src/mindmap.rs`, change line 126 from:
```rust
fn fit_label_for_node(s: &str) -> (String, bool) {
```
to:
```rust
pub(crate) fn fit_label_for_node(s: &str) -> (String, bool) {
```

- [ ] **Step 2: Widen `layout` visibility**

In `src/mindmap.rs`, change line 214 from:
```rust
fn layout(nodes: &mut [MNode], idx: usize, y_cursor: &mut f32) -> f32 {
```
to:
```rust
pub(crate) fn layout(nodes: &mut [MNode], idx: usize, y_cursor: &mut f32) -> f32 {
```

- [ ] **Step 3: Verify it still builds**

Run: `cargo build --release 2>&1 | tail -3`
Expected: builds clean (warnings about unused `pub(crate)` are fine — Task 2 consumes them).

- [ ] **Step 4: Commit**

```bash
git add src/mindmap.rs
git commit -m "refactor(mindmap): expose layout + fit_label_for_node as pub(crate)"
```

---

## Task 2: Create `data_mindmap` module — DataValue + normalization

Introduce the module and the `DataValue` enum plus the two normalizers. Test that JSON and YAML normalize to the same shape.

**Files:**
- Create: `src/data_mindmap.rs`
- Modify: `src/lib.rs:4` (register module, alphabetical — after `cli`, before `diagram`)

- [ ] **Step 1: Register the module**

In `src/lib.rs`, after the `pub mod cli;` line (line 4), add:
```rust
pub mod data_mindmap;
```

- [ ] **Step 2: Write the module with `DataValue` + normalizers + failing test**

Create `src/data_mindmap.rs`:
```rust
use crate::ast::BlockId;
use crate::mindmap::MNode;
use iced::Size;
use std::collections::HashMap;
use std::path::Path;

/// Normalized view over both JSON and YAML so one walker serves both.
#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Scalar(String),
    Array(Vec<DataValue>),
    Object(Vec<(String, DataValue)>),
}

/// One step from the document root toward a node's value.
#[derive(Debug, Clone, PartialEq)]
pub enum PathSeg {
    Key(String),
    Index(usize),
}

fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn from_json(v: &serde_json::Value) -> DataValue {
    match v {
        serde_json::Value::Array(a) => DataValue::Array(a.iter().map(from_json).collect()),
        serde_json::Value::Object(o) => {
            DataValue::Object(o.iter().map(|(k, val)| (k.clone(), from_json(val))).collect())
        }
        scalar => DataValue::Scalar(json_scalar(scalar)),
    }
}

fn yaml_scalar(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        // Tagged / sequence / mapping handled before this is called.
        _ => String::new(),
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
                        other => yaml_scalar(other),
                    };
                    (key, from_yaml(val))
                })
                .collect(),
        ),
        scalar => DataValue::Scalar(yaml_scalar(scalar)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                assert_eq!(fields[0], ("n".into(), DataValue::Scalar("42".into())));
                assert_eq!(fields[1], ("b".into(), DataValue::Scalar("true".into())));
                assert_eq!(fields[2], ("z".into(), DataValue::Scalar("null".into())));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --release data_mindmap 2>&1 | tail -15`
Expected: `json_and_yaml_normalize_alike` and `scalars_stringify` PASS.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/data_mindmap.rs
git commit -m "feat(data_mindmap): DataValue enum + JSON/YAML normalizers"
```

---

## Task 3: Build the MNode tree + path map from DataValue

Walk `DataValue` into `Vec<MNode>` (arena, child-index refs) with minted `BlockId`s, applying the label rules and the collapse set, while recording each node's path.

**Files:**
- Modify: `src/data_mindmap.rs`

- [ ] **Step 1: Write failing tests for tree shape and labels**

Append to the `tests` module in `src/data_mindmap.rs`:
```rust
    use std::collections::HashSet;

    fn obj(src: &str) -> DataValue {
        from_json(&serde_json::from_str(src).unwrap())
    }

    #[test]
    fn nested_object_labels_and_children() {
        let v = obj(r#"{"name":"rmdv","deps":{"serde":"1"}}"#);
        let (nodes, _paths) = build_tree(&v, "rmdv.json", &HashSet::new());
        // root + name + deps + serde
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
        // root + tags + [0] + [1]
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
        // find the "serde: ..." leaf
        let leaf = nodes.iter().find(|n| n.full_label.starts_with("serde")).unwrap();
        let path = paths.get(&leaf.id.unwrap()).unwrap();
        assert_eq!(path, &vec![PathSeg::Key("deps".into()), PathSeg::Key("serde".into())]);
    }

    #[test]
    fn collapsed_node_hides_children() {
        let v = obj(r#"{"deps":{"serde":"1","iced":"0.14"}}"#);
        let (full, _) = build_tree(&v, "f.json", &HashSet::new());
        let deps_id = full.iter().find(|n| n.full_label == "deps").unwrap().id.unwrap();
        let mut collapsed = HashSet::new();
        collapsed.insert(deps_id);
        let (nodes, _) = build_tree(&v, "f.json", &collapsed);
        let deps = nodes.iter().find(|n| n.full_label == "deps").unwrap();
        assert!(deps.children.is_empty());
        assert!(deps.has_hidden_children);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --release data_mindmap 2>&1 | tail -15`
Expected: FAIL — `build_tree` not found.

- [ ] **Step 3: Implement `build_tree`**

Add to `src/data_mindmap.rs` (before the `tests` module):
```rust
use crate::mindmap::fit_label_for_node;
use std::collections::HashSet;

const MAX_DEPTH: usize = 64;

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

    /// Push a node, return its index. `label_text` is the full (untruncated) text.
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
            DataValue::Scalar(_) => {} // scalars are attached by add_member, never recursed
        }
    }

    /// Attach one member (object value or array element) under `parent_idx`.
    /// `head` is the key text or `[i]` index label.
    fn add_member(&mut self, parent_idx: usize, head: String, val: &DataValue, level: u8, path: Vec<PathSeg>) {
        match val {
            DataValue::Scalar(s) => {
                let full = format!("{head}: {}", quote_scalar(s, val));
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

/// Render a scalar as it should appear in a label: strings get quotes, other
/// scalars (numbers/bools/null) stay bare. We approximate "was it a string" by
/// checking whether the stored text is already a bare number/bool/null token.
fn quote_scalar(s: &str, _v: &DataValue) -> String {
    let bare = s == "true" || s == "false" || s == "null" || s.parse::<f64>().is_ok();
    if bare {
        s.to_string()
    } else {
        format!("\"{s}\"")
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
    // Root node (filename), level 0, empty path. Root gets no id minted as a
    // child — give it id 0 explicitly so it owns the path-root.
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
```

> Note on `quote_scalar`: a JSON string `"42"` will be rendered bare. This is a cosmetic edge case (a stringified number losing its quotes in the label); the panel subtree shows the true type. Acceptable for label display.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --release data_mindmap 2>&1 | tail -20`
Expected: all Task-2 and Task-3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/data_mindmap.rs
git commit -m "feat(data_mindmap): build_tree — DataValue to MNode arena + path map"
```

---

## Task 4: `build_layout` + `subtree_pretty` (parse, fallback, panel source)

Add the top-level entry points: `build_layout` (parse source → tree → layout → size, with graceful fallback) and `subtree_pretty` (re-parse + navigate path + pretty-print for the panel).

**Files:**
- Modify: `src/data_mindmap.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module:
```rust
    #[test]
    fn build_layout_json_ok() {
        let (nodes, size, paths) =
            build_layout(r#"{"a":1,"b":2}"#, "json", None, &HashSet::new());
        assert_eq!(nodes.len(), 3); // root + a + b
        assert!(size.width > 0.0 && size.height > 0.0);
        assert_eq!(paths.len(), 3);
        // root got x/y assigned by layout
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
        assert_eq!(nodes.len(), 2); // root + warning
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --release data_mindmap 2>&1 | tail -15`
Expected: FAIL — `build_layout` / `subtree_pretty` not found.

- [ ] **Step 3: Implement `build_layout` + `subtree_pretty` + fallback**

Add to `src/data_mindmap.rs` (before `tests`):
```rust
fn title_for(file: Option<&Path>) -> String {
    file.and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

/// Root + a single warning child. Used for parse errors and unsupported langs.
fn fallback(title: &str, warning: &str) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>) {
    let collapsed = HashSet::new();
    let v = DataValue::Object(vec![(warning.to_string(), DataValue::Scalar(String::new()))]);
    // Reuse build_tree so the warning shows as a node; but we want the raw
    // warning text, not "warning: ". Build manually instead:
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
    let _ = (collapsed, v);
    (nodes, paths)
}

fn parse_to_value(source: &str, lang: &str) -> Option<DataValue> {
    match lang {
        "json" => serde_json::from_str::<serde_json::Value>(source).ok().map(|v| from_json(&v)),
        "yaml" => serde_yaml::from_str::<serde_yaml::Value>(source).ok().map(|v| from_yaml(&v)),
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
            let mut cur: &serde_json::Value = &serde_json::from_str(source).ok()?;
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
            let mut cur: &serde_yaml::Value = &serde_yaml::from_str(source).ok()?;
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --release data_mindmap 2>&1 | tail -25`
Expected: all `data_mindmap` tests PASS (Tasks 2–4).

- [ ] **Step 5: Commit**

```bash
git add src/data_mindmap.rs
git commit -m "feat(data_mindmap): build_layout with fallback + subtree_pretty"
```

---

## Task 5: Wire dispatch into `App::mindmap_layout` + cache the path map

Make the layout cache carry the path map and dispatch on `is_data_doc`. Add the panel-string RefCell field.

**Files:**
- Modify: `src/app.rs` — cache field (`app.rs:499`), `mindmap_layout` (`app.rs:1059`), `invalidate_mindmap_layout` (`app.rs:1073`), `App` init (around `app.rs:557`), `mindmap_focus_first_child` (`app.rs:1084`), `view()` mindmap call (`app.rs:3472`).

- [ ] **Step 1: Read the exact current cache field + init + callers**

Run:
```bash
grep -nE "mindmap_layout: RefCell|mindmap_layout:|let \(nodes, _\) = self.mindmap_layout|self.mindmap_layout\(\)" src/app.rs
```
Confirm the field type at `app.rs:499` and the two `mindmap_layout()` call sites. **Read those exact lines before editing** (file may have shifted).

- [ ] **Step 2: Change the cache field type to include the path map**

In `src/app.rs` (field around line 499), change:
```rust
    mindmap_layout: RefCell<Option<(std::sync::Arc<Vec<crate::mindmap::MNode>>, iced::Size)>>,
```
to:
```rust
    mindmap_layout: RefCell<
        Option<(
            std::sync::Arc<Vec<crate::mindmap::MNode>>,
            iced::Size,
            std::sync::Arc<std::collections::HashMap<crate::ast::BlockId, Vec<crate::data_mindmap::PathSeg>>>,
        )>,
    >,
```
Add a new field next to it for the panel pretty-string cache:
```rust
    /// Pretty-printed subtree for the data-doc mindmap leaf panel, keyed by the
    /// shown node id. Recomputed when `mindmap_panel_shown` changes.
    mindmap_data_panel: RefCell<Option<(crate::ast::BlockId, String)>>,
```

- [ ] **Step 3: Initialize the new field**

In the `App` constructor (near `app.rs:557`, where `mindmap_layout: RefCell::new(None)` is initialized — grep `mindmap_layout: RefCell::new` to find it), add alongside:
```rust
            mindmap_data_panel: RefCell::new(None),
```

- [ ] **Step 4: Rewrite `mindmap_layout()` to dispatch + return the triple**

Replace the body of `fn mindmap_layout` (`app.rs:1059`) with:
```rust
    fn mindmap_layout(
        &self,
    ) -> (
        std::sync::Arc<Vec<crate::mindmap::MNode>>,
        iced::Size,
        std::sync::Arc<std::collections::HashMap<crate::ast::BlockId, Vec<crate::data_mindmap::PathSeg>>>,
    ) {
        let mut cache = self.mindmap_layout.borrow_mut();
        if cache.is_none() {
            let (nodes, size, paths) = if self.is_data_doc {
                let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                crate::data_mindmap::build_layout(
                    &self.source,
                    lang,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                )
            } else {
                let (n, s) = crate::mindmap::build_layout(
                    &self.ast,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                );
                (n, s, std::collections::HashMap::new())
            };
            *cache = Some((
                std::sync::Arc::new(nodes),
                size,
                std::sync::Arc::new(paths),
            ));
        }
        let (nodes, size, paths) = cache.as_ref().unwrap();
        (
            std::sync::Arc::clone(nodes),
            *size,
            std::sync::Arc::clone(paths),
        )
    }
```

- [ ] **Step 5: Update the two `mindmap_layout()` callers to the 3-tuple**

In `mindmap_focus_first_child` (`app.rs:1084`), change:
```rust
        let (nodes, _) = self.mindmap_layout();
```
to:
```rust
        let (nodes, _, _) = self.mindmap_layout();
```
In `view()` at the mindmap branch (grep `let (nodes, ` near `app.rs:3472`; it currently destructures `(nodes, canvas_size)` or similar from `self.mindmap_layout()`), add the third ignored element, e.g.:
```rust
        let (nodes, mind_size, _) = self.mindmap_layout();
```
(Match the existing variable names; only add the trailing `, _`.)

- [ ] **Step 6: Invalidate the panel cache alongside the layout cache**

In `invalidate_mindmap_layout` (`app.rs:1073`), change:
```rust
    fn invalidate_mindmap_layout(&self) {
        *self.mindmap_layout.borrow_mut() = None;
    }
```
to:
```rust
    fn invalidate_mindmap_layout(&self) {
        *self.mindmap_layout.borrow_mut() = None;
        *self.mindmap_data_panel.borrow_mut() = None;
    }
```

- [ ] **Step 7: Build to verify dispatch compiles**

Run: `cargo build --release 2>&1 | tail -8`
Expected: builds clean. If `view()` destructure names mismatch, fix them per the actual lines you read in Step 1.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): dispatch mindmap_layout on is_data_doc, carry path map"
```

---

## Task 6: Data-doc leaf panel — pretty-printed subtree

Branch `mindmap_panel_view` so data docs render the selected node's subtree via `render::data_view`, using the RefCell pretty-string cache.

**Files:**
- Modify: `src/app.rs` — `mindmap_panel_view` (`app.rs:1317`).

- [ ] **Step 1: Read the current `mindmap_panel_view` head**

Run: `sed -n '1315,1335p' src/app.rs` — confirm the signature and the `match self.mindmap_panel_shown` opening. **Read it before editing.**

- [ ] **Step 2: Add the data-doc branch at the top of `mindmap_panel_view`**

In `mindmap_panel_view` (`app.rs:1317`), immediately after the signature line `) -> Element<'_, Message> {` and before `let pal_c = *pal;`, insert:
```rust
        if self.is_data_doc {
            return self.mindmap_data_panel_view(pal, recently_scrolled, panel_width);
        }
```

- [ ] **Step 3: Add the `mindmap_data_panel_view` helper method**

Immediately before `fn mindmap_panel_view` (`app.rs:1317`), add:
```rust
    /// Leaf panel for data-doc mindmaps: pretty-print the selected node's
    /// subtree and render it through the shared data code-block view. The
    /// pretty string is cached in `mindmap_data_panel` so it is computed at most
    /// once per selection change (mirrors the markdown panel's settle behavior).
    fn mindmap_data_panel_view(
        &self,
        pal: &Palette,
        recently_scrolled: bool,
        panel_width: f32,
    ) -> Element<'_, Message> {
        let pal_c = *pal;
        // Refresh the cached pretty string if the shown node changed.
        if let Some(target) = self.mindmap_panel_shown {
            let needs = self
                .mindmap_data_panel
                .borrow()
                .as_ref()
                .map(|(id, _)| *id != target)
                .unwrap_or(true);
            if needs {
                let (_, _, paths) = self.mindmap_layout();
                let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                let pretty = paths
                    .get(&target)
                    .and_then(|p| crate::data_mindmap::subtree_pretty(&self.source, lang, p))
                    .unwrap_or_default();
                *self.mindmap_data_panel.borrow_mut() = Some((target, pretty));
            }
        }

        let guard = self.mindmap_data_panel.borrow();
        let content: Element<'_, Message> = match (self.mindmap_panel_shown, guard.as_ref()) {
            (Some(_), Some((_, pretty))) if !pretty.is_empty() => {
                crate::render::data_view(pretty, &[], pal, &self.typography)
            }
            _ => container(
                text("Select a node to see its value")
                    .color(pal.muted)
                    .size(13),
            )
            .padding(24)
            .into(),
        };

        let scrolled = scrollable(container(content).padding(Padding::from([24, 24])))
            .height(Length::Shrink)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal_c, recently_scrolled));
        container(scrolled)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal_c.surface.into()),
                ..Default::default()
            })
            .into()
    }
```

> Borrow note: `guard` (the `Ref` from `mindmap_data_panel.borrow()`) is held for the rest of the function, so `data_view`'s returned `Element` can borrow `pretty: &str` from it. The `Ref` lives as long as the returned `Element<'_>` because both are tied to `&self`. If the borrow checker rejects this (Ref dropped at end of fn vs Element outliving it), fall back to: clone `pretty` into a local `String` owned by the function and build the element from the owned string — but try the borrow form first; `data_view` borrows `&'a str` and the `Ref` borrow is from `&self`, which the returned `Element<'_, Message>` is already bound to.

- [ ] **Step 4: Build**

Run: `cargo build --release 2>&1 | tail -10`
Expected: builds clean. If the borrow note's lifetime issue appears (error mentions `guard` / `borrow` does not live long enough), apply the owned-string fallback: replace the `guard`-based block with:
```rust
        let pretty_owned: Option<String> = self
            .mindmap_data_panel
            .borrow()
            .as_ref()
            .filter(|(_, p)| !p.is_empty())
            .map(|(_, p)| p.clone());
        let content: Element<'_, Message> = match pretty_owned {
            Some(pretty) => {
                // data_view needs &'a str; leak-free: build an owned mono text block.
                container(
                    text(pretty)
                        .font(iced::Font::with_name("JetBrains Mono"))
                        .size(self.typography.code_size),
                )
                .padding(Padding::from([28, 32]))
                .width(Length::Fill)
                .into()
            }
            None => container(text("Select a node to see its value").color(pal.muted).size(13))
                .padding(24)
                .into(),
        };
```
(This owned-string form loses per-token color but is lifetime-safe. Prefer the `data_view` form if it compiles.)

- [ ] **Step 5: Confirm the data-doc test suite + full build still green**

Run: `cargo test --release 2>&1 | tail -15`
Expected: all tests pass, including `data_mindmap` module.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): data-doc mindmap leaf panel shows pretty-printed subtree"
```

---

## Task 7: Visual verification (project hard rule)

Per `.claude/rules/ui-verification.md`: drive the running release binary over IPC and LOOK at the rendered mindmap before declaring done.

**Files:** none (verification only).

- [ ] **Step 1: Build the release binary**

Run: `cargo build --release 2>&1 | tail -3`
Expected: `Finished release`.

- [ ] **Step 2: Create test fixtures**

```bash
cat > /tmp/mm.json <<'EOF'
{
  "name": "rmdv",
  "version": "0.2.2",
  "deps": { "serde": "1", "iced": "0.14" },
  "tags": ["rust", "gui", "viewer"],
  "items": [ { "a": 1 }, { "b": 2 } ]
}
EOF
printf 'name: rmdv\nversion: 0.2.2\ndeps:\n  serde: "1"\n  iced: "0.14"\ntags:\n  - rust\n  - gui\n' > /tmp/mm.yaml
```

- [ ] **Step 3: Open JSON, toggle mindmap, screenshot (use the rmdv-cli skill)**

Drive via IPC: `rmdv open /tmp/mm.json` → switch to mindmap (⌘M / the IPC view-mode command) → screenshot. Then click/select a parent node (e.g. `deps`) and screenshot the panel.
Inspect for: `key: value` leaf labels, `[0]`/`[1]` array children, no label cropping at node edges, arrow-nav moves focus and the panel follows, the `deps` panel shows the pretty-printed `{ "serde": "1", "iced": "0.14" }`.

- [ ] **Step 4: Repeat for YAML**

`rmdv open /tmp/mm.yaml` → mindmap → screenshot. Same checks.

- [ ] **Step 5: Malformed + unsupported fallback check**

```bash
echo '{not valid json' > /tmp/bad.json
```
`rmdv open /tmp/bad.json` → mindmap → screenshot. Expect root + `⚠ invalid json` child, no crash.

- [ ] **Step 6: Fix any visual defect found, re-shoot, then stop**

If cropping/overflow/blank-panel/nav-not-following appears, fix in source, rebuild, re-screenshot. Only declare done once a screenshot of each of JSON + YAML mindmaps looks correct.

---

## Self-Review Notes

- **Spec coverage:** key:value labels (T3), indexed arrays (T3), ⌘M auto-dispatch (T5), parse-fail fallback (T4), TOML unsupported fallback (T4), parse-fresh/Option A (T4/T5 no persisted DataValue), serde_yaml kept (T2), pretty-subtree panel (T6), depth guard (T3 `MAX_DEPTH`), BlockId stability (T3 test). All covered.
- **Lifetime risk** in T6 is the one real unknown; both the borrow form and an owned-string fallback are specified so the task can't dead-end.
- **`view()` destructure names** (T5 Step 5) must be matched to the actual source line — the plan flags this and tells the worker to read first.

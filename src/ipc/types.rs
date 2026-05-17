use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    View,
    Edit,
    Mindmap,
}

/// Per-call window focus behavior for navigation commands.
///
/// `Default` means: follow the user's `auto_focus_on_nav` preference.
/// `Force` always raises; `Suppress` never raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusBehavior {
    Default,
    Force,
    Suppress,
}

impl Default for FocusBehavior {
    fn default() -> Self {
        FocusBehavior::Default
    }
}

impl FocusBehavior {
    pub fn is_default(&self) -> bool {
        matches!(self, FocusBehavior::Default)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "kebab-case")]
pub enum Cmd {
    Open {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
        #[serde(default, skip_serializing_if = "FocusBehavior::is_default")]
        focus: FocusBehavior,
    },
    OpenFolder {
        dir: String,
    },
    Goto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
        #[serde(default, skip_serializing_if = "FocusBehavior::is_default")]
        focus: FocusBehavior,
    },
    Mode {
        mode: Mode,
        #[serde(default, skip_serializing_if = "FocusBehavior::is_default")]
        focus: FocusBehavior,
    },
    Reveal {
        file: String,
        #[serde(default, skip_serializing_if = "FocusBehavior::is_default")]
        focus: FocusBehavior,
    },
    Focus,
    Close,
    Current,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub cmd: Cmd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64) -> Self {
        Self { id, ok: true, result: None, error: None }
    }
    pub fn ok_with(id: u64, result: serde_json::Value) -> Self {
        Self { id, ok: true, result: Some(result), error: None }
    }
    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self { id, ok: false, result: None, error: Some(msg.into()) }
    }
}

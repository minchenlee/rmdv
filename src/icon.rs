use iced::widget::{text, Text};
use iced::{Color, Font};

pub fn font() -> Font {
    Font::with_name("lucide")
}

pub fn glyph<'a>(code: char, size: f32, color: Color) -> Text<'a> {
    text(code.to_string()).font(font()).size(size).color(color)
}

pub mod ic {
    pub const CHEVRON_RIGHT: char = '\u{e06f}';
    pub const CHEVRON_DOWN: char = '\u{e06d}';
    pub const CHEVRON_LEFT: char = '\u{e06e}';
    pub const FOLDER: char = '\u{e0d7}';
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const FILE: char = '\u{e0c0}';
    pub const FILE_TEXT: char = '\u{e0cc}';
    pub const HOME: char = '\u{e0f5}';
    pub const ARROW_UP: char = '\u{e04a}';
    pub const ARROW_UP_FROM_LINE: char = '\u{e45a}';
    pub const ARROW_LEFT: char = '\u{e048}';
    pub const ARROW_RIGHT: char = '\u{e049}';
    pub const X: char = '\u{e1b2}';
    pub const SEARCH: char = '\u{e151}';
    pub const CIRCLE_DOT: char = '\u{e345}';
    pub const CHECK: char = '\u{e06c}';
    pub const MOON: char = '\u{e11e}';
    pub const SUN: char = '\u{e178}';
    pub const COMMAND: char = '\u{e09a}';
    pub const PANEL_LEFT: char = '\u{e12a}';
    pub const COPY: char = '\u{e09e}';
    /// Lucide "maximize" — used for diagram hover-zoom affordance.
    pub const ZOOM: char = '\u{e1a1}';
    /// Lucide "keyboard" — floating shortcuts-cheatsheet button.
    pub const KEYBOARD: char = '\u{e284}';
}

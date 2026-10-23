//! Free view functions: `fn(..., Palette) -> Element` builders that read app
//! state but never mutate it.

use super::*;

mod chrome;
mod edge_peek;
mod overlays;
mod press_watch;
mod settings;
mod sidebar;
mod title_row;
mod toolbar;
mod vault_search;
mod widgets;

pub(super) use chrome::*;
pub(super) use edge_peek::EdgePeek;
#[cfg(test)]
pub(super) use edge_peek::TEST_BUTTONS_DOWN;
pub(super) use overlays::*;
pub(super) use press_watch::PressWatch;
pub(super) use settings::*;
pub(super) use sidebar::*;
pub(super) use title_row::TitleRow;
pub(super) use toolbar::*;
pub(super) use vault_search::*;
pub(super) use widgets::*;
pub(crate) use widgets::{reader_tip, sleek_scrollable_style};

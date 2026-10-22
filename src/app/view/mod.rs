//! Free view functions: `fn(..., Palette) -> Element` builders that read app
//! state but never mutate it.

use super::*;

mod chrome;
mod overlays;
mod settings;
mod sidebar;
mod toolbar;
mod vault_search;
mod widgets;

pub(super) use chrome::*;
pub(super) use overlays::*;
pub(super) use settings::*;
pub(super) use sidebar::*;
pub(super) use toolbar::*;
pub(super) use vault_search::*;
pub(super) use widgets::*;
pub(crate) use widgets::{reader_tip, sleek_scrollable_style};

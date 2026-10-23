//! Free view functions: `fn(..., Palette) -> Element` builders that read app
//! state but never mutate it.

use super::*;

mod chrome;
mod context_menu;
mod edge_peek;
mod menu_sink;
mod overlays;
mod press_outside;
mod press_watch;
mod settings;
mod sidebar;
mod title_row;
mod toolbar;
mod vault_search;
mod wheel_gate;
mod widgets;

pub(super) use chrome::*;
pub(super) use context_menu::*;
pub(super) use edge_peek::EdgePeek;
#[cfg(test)]
pub(super) use edge_peek::TEST_BUTTONS_DOWN;
pub(super) use overlays::*;
pub(super) use press_outside::PressOutside;
pub(super) use press_watch::{double_click_interval, PressWatch};
pub(super) use settings::*;
pub(super) use sidebar::*;
pub(super) use title_row::TitleRow;
pub(super) use toolbar::*;
pub(super) use vault_search::*;
#[cfg(test)]
pub(super) use wheel_gate::HScrollProbe;
#[cfg(test)]
pub(super) use wheel_gate::PROBE_WALKS;
pub(super) use wheel_gate::{Gate, WheelGate};
pub(super) use widgets::*;
pub(crate) use widgets::{reader_tip, sleek_scrollable_style};

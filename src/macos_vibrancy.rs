//! Window glass (macOS): an AppKit `NSVisualEffectView` behind Iced's Metal
//! layer, so the blurred desktop shows through translucent panels.
//!
//! The user picks which panels are translucent ([`Glass`]) and how strongly
//! the theme tints them (the opacity). The window must be created transparent
//! for any of this to show, so turning glass on from [`Glass::Off`] takes
//! effect on the next launch.

use serde::{Deserialize, Serialize};

/// Which panels let the window glass show through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Glass {
    /// Opaque window.
    Off,
    /// Translucent sidebar; the reader stays opaque.
    Sidebar,
    /// Translucent sidebar and reader (the default).
    #[default]
    Window,
}

impl Glass {
    pub fn next(self) -> Self {
        match self {
            Glass::Off => Glass::Sidebar,
            Glass::Sidebar => Glass::Window,
            Glass::Window => Glass::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Glass::Off => "off",
            Glass::Sidebar => "sidebar",
            Glass::Window => "whole window",
        }
    }
}

/// Tint opacity steps over the glass. Below 0.6 body text loses contrast on
/// busy wallpapers.
pub const OPACITY_LEVELS: [f32; 4] = [0.6, 0.7, 0.8, 0.9];
pub const DEFAULT_OPACITY: f32 = 0.6;

/// A stored opacity, kept inside the supported range.
pub fn clamp_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(OPACITY_LEVELS[0], OPACITY_LEVELS[OPACITY_LEVELS.len() - 1])
    } else {
        DEFAULT_OPACITY
    }
}

/// The next opacity step, wrapping from the most opaque to the least.
pub fn next_opacity(opacity: f32) -> f32 {
    OPACITY_LEVELS
        .into_iter()
        .find(|level| *level > opacity + 0.001)
        .unwrap_or(OPACITY_LEVELS[0])
}

/// True when the window must be created transparent for the saved glass mode.
pub fn launch_transparent(glass: Glass) -> bool {
    cfg!(target_os = "macos") && glass != Glass::Off
}

/// Installs the blur view on the first call, then shows or hides it and
/// matches its material to the theme. Runs on the main thread (Iced's
/// `window::run`).
#[cfg(target_os = "macos")]
pub fn show(window: &dyn iced::window::Window, visible: bool, dark: bool) {
    if let Err(e) = imp::show(window, visible, dark) {
        eprintln!("rmdv: window glass unavailable: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn show(_window: &dyn iced::window::Window, _visible: bool, _dark: bool) {}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;

    use iced::window::raw_window_handle::RawWindowHandle;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };

    thread_local! {
        /// The installed blur view. AppKit objects live on the main thread.
        static BLUR: RefCell<Option<Retained<NSVisualEffectView>>> = const { RefCell::new(None) };
    }

    pub fn show(
        window: &dyn iced::window::Window,
        visible: bool,
        dark: bool,
    ) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
        BLUR.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Some(install(window, mtm)?);
            }
            let blur = slot.as_ref().expect("installed above");
            blur.setHidden(!visible);
            // The material follows its own appearance, not the system's, so a
            // light theme gets the light material in dark mode and vice versa.
            let name = unsafe {
                if dark {
                    NSAppearanceNameDarkAqua
                } else {
                    NSAppearanceNameAqua
                }
            };
            blur.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
            Ok(())
        })
    }

    fn install(
        window: &dyn iced::window::Window,
        mtm: MainThreadMarker,
    ) -> Result<Retained<NSVisualEffectView>, String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("not an AppKit window".into());
        };
        // SAFETY: winit's AppKit handle points at the window's live content
        // view, and we are on the main thread.
        let content: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        let frame_view = unsafe { content.superview() }.ok_or("content view has no superview")?;
        let ns_window = content.window().ok_or("content view has no window")?;

        // The blur sits beside the content view inside the window frame view,
        // ordered below it, so the Metal layer composites on top of it.
        let blur = NSVisualEffectView::initWithFrame(mtm.alloc(), frame_view.bounds());
        blur.setMaterial(NSVisualEffectMaterial::Sidebar);
        blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        blur.setState(NSVisualEffectState::FollowsWindowActiveState);
        blur.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        frame_view.addSubview_positioned_relativeTo(
            &blur,
            NSWindowOrderingMode::Below,
            Some(content),
        );
        ns_window.setOpaque(false);
        ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
        Ok(blur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_steps_wrap_and_stay_in_range() {
        assert_eq!(next_opacity(0.6), 0.7);
        assert_eq!(next_opacity(0.8), 0.9);
        assert_eq!(next_opacity(0.9), 0.6);
        assert_eq!(next_opacity(0.75), 0.8);
        assert_eq!(clamp_opacity(0.1), 0.6);
        assert_eq!(clamp_opacity(1.5), 0.9);
        assert_eq!(clamp_opacity(f32::NAN), DEFAULT_OPACITY);
    }

    #[test]
    fn glass_modes_cycle_through_every_mode() {
        assert_eq!(Glass::Off.next(), Glass::Sidebar);
        assert_eq!(Glass::Sidebar.next(), Glass::Window);
        assert_eq!(Glass::Window.next(), Glass::Off);
        assert!(!launch_transparent(Glass::Off));
        assert_eq!(launch_transparent(Glass::Window), cfg!(target_os = "macos"));
    }
}

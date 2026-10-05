//! Experimental macOS window vibrancy (spike, off by default).
//!
//! Set `RMDV_VIBRANCY=1` to make the window transparent and place an AppKit
//! `NSVisualEffectView` (sidebar material) behind Iced's Metal layer. The
//! sidebar then paints its color at [`GLASS_ALPHA`] so the blurred desktop
//! shows through; the reader panel stays opaque.

/// Opacity of the sidebar tint over the blur (zeron uses 0.80).
pub const GLASS_ALPHA: f32 = 0.80;

/// True when this run asked for vibrancy and the platform supports it.
pub fn requested() -> bool {
    cfg!(target_os = "macos") && std::env::var_os("RMDV_VIBRANCY").is_some_and(|v| v == "1")
}

/// Inserts the blur view behind the window content. Must run on the main
/// thread, which is where Iced runs `window::run` callbacks.
#[cfg(target_os = "macos")]
pub fn install(window: &dyn iced::window::Window) {
    if let Err(e) = imp::install(window) {
        eprintln!("rmdv: window vibrancy unavailable: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install(_window: &dyn iced::window::Window) {}

#[cfg(target_os = "macos")]
mod imp {
    use iced::window::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };

    pub fn install(window: &dyn iced::window::Window) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
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
        unsafe {
            frame_view.addSubview_positioned_relativeTo(
                &blur,
                NSWindowOrderingMode::Below,
                Some(content),
            );
        }
        ns_window.setOpaque(false);
        ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
        Ok(())
    }
}

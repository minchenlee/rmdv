//! A sharper raster for a diagram zoomed in the viewer (MDV-048 P5).
//!
//! A diagram's inline raster is 2x its intrinsic size and at most 1600 px
//! wide, so it blurs once the viewer zooms past it. The viewer reports the
//! raster detail it wants (`Message::ImageZoomDetail`); this state asks for
//! one larger raster after a short debounce, identifies each request, and
//! keeps only the result of the latest one. Closing the viewer drops it all.

use iced::widget::image;

/// How long the wanted detail must hold before a raster is made, so a
/// zoom gesture does not start one raster per tier it passes.
pub(super) const ZOOM_RASTER_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

/// Raster pixels per logical pixel the viewer assumes when it asks for
/// detail. Retina is the common case; on a 1x display the raster is larger
/// than needed but stays within the byte cap.
pub(super) const ZOOM_RASTER_DENSITY: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingZoomRaster {
    pub id: u64,
    pub size: (u32, u32),
    /// The debounce ended and the raster is being made.
    pub dispatched: bool,
}

/// The sharper raster state of the diagram open in the viewer.
#[derive(Debug, Clone)]
pub(super) struct ZoomRaster {
    /// The diagram: its content hash and theme id in the diagram cache.
    pub key: (u64, u32),
    /// The inline raster's pixel size; the viewer's 100% size.
    pub base: (u32, u32),
    pub pending: Option<PendingZoomRaster>,
    /// The sharper raster on screen, and its pixel width.
    pub sharp: Option<(u32, image::Handle)>,
}

impl ZoomRaster {
    pub fn new(key: (u64, u32), base: (u32, u32)) -> Self {
        Self {
            key,
            base,
            pending: None,
            sharp: None,
        }
    }

    /// The pixel width of the raster on screen.
    pub fn shown_width(&self) -> u32 {
        self.sharp.as_ref().map_or(self.base.0, |(w, _)| *w)
    }

    /// The viewer wants `tier` raster pixels per base pixel. Returns the
    /// new request's id when a debounce should start (`id` is the next
    /// free request id); `None` when the raster on screen or the one
    /// already requested will do.
    pub fn want(&mut self, tier: u32, budget_bytes: usize, id: u64) -> Option<u64> {
        let size = crate::diagram::zoom_raster_size(self.base, tier, budget_bytes)
            .filter(|(w, _)| *w > self.shown_width());
        let Some(size) = size else {
            // Zoomed back out before the debounce ended: make nothing. A
            // raster already being made still lands; it is sharper.
            if self.pending.is_some_and(|p| !p.dispatched) {
                self.pending = None;
            }
            return None;
        };
        if self.pending.is_some_and(|p| p.size == size) {
            return None;
        }
        self.pending = Some(PendingZoomRaster {
            id,
            size,
            dispatched: false,
        });
        Some(id)
    }

    /// The debounce for `id` ended. Returns the size to make, once, when
    /// `id` is still the latest request.
    pub fn due(&mut self, id: u64) -> Option<(u32, u32)> {
        let pending = self
            .pending
            .as_mut()
            .filter(|p| p.id == id && !p.dispatched)?;
        pending.dispatched = true;
        Some(pending.size)
    }

    /// A raster arrived (or failed). Keeps it only when `id` is the latest
    /// request; returns whether it was kept.
    pub fn ready(&mut self, id: u64, result: Result<image::Handle, String>) -> bool {
        if !self.pending.is_some_and(|p| p.id == id) {
            return false;
        }
        self.pending = None;
        match result {
            Ok(handle) => {
                let width = rgba_size(&handle).map_or(0, |(w, _)| w);
                self.sharp = Some((width, handle));
                true
            }
            Err(_) => false,
        }
    }
}

/// The pixel size of an RGBA handle.
pub(super) fn rgba_size(handle: &image::Handle) -> Option<(u32, u32)> {
    match handle {
        image::Handle::Rgba { width, height, .. } => Some((*width, *height)),
        _ => None,
    }
}

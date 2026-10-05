//! Remote and zoomed image cache with a soft byte budget.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ImageState {
    Loading,
    Loaded(iced::widget::image::Handle),
    LoadedSvg {
        svg: iced::widget::svg::Handle,
        bytes: std::sync::Arc<Vec<u8>>,
        /// Rasterized variant for zoom modal (filled lazily on first zoom open).
        raster: Option<iced::widget::image::Handle>,
    },
    Failed,
}

/// Retained heap cost of one image entry: encoded/decoded payload bytes.
/// `svg::Handle::from_memory` keeps its own copy of the SVG payload alongside
/// the `bytes` Arc, hence the ×2.
pub(super) fn image_state_cost(s: &ImageState) -> usize {
    fn handle_cost(h: &iced::widget::image::Handle) -> usize {
        match h {
            iced::widget::image::Handle::Rgba { pixels, .. } => pixels.len(),
            iced::widget::image::Handle::Bytes(_, bytes) => bytes.len(),
            iced::widget::image::Handle::Path(..) => 0,
        }
    }
    match s {
        ImageState::Loading | ImageState::Failed => 0,
        ImageState::Loaded(h) => handle_cost(h),
        ImageState::LoadedSvg { bytes, raster, .. } => {
            bytes.len() * 2 + raster.as_ref().map(handle_cost).unwrap_or(0)
        }
    }
}

/// Soft byte budget for `ImageCache`. Bounds session-long accumulation of
/// fetched remote images and SVG zoom rasters; generous enough that a single
/// document's images never get evicted in realistic use.
pub(super) const IMAGE_CACHE_BYTE_BUDGET: usize = 256 * 1024 * 1024;

/// Insertion-ordered image cache with a soft byte budget. Entries were
/// previously kept in a bare `HashMap` for the whole session; `trim` (called
/// after each image load) evicts the oldest entries NOT referenced by the
/// current document, so what's on screen never changes.
#[derive(Debug, Default)]
pub struct ImageCache {
    map: HashMap<String, ImageState>,
    /// Insertion order, oldest first. Only ever holds keys present in `map`.
    order: Vec<String>,
    /// Running total of `image_state_cost` over all entries, so the budget
    /// check on each image load is O(1) instead of a full map walk.
    bytes: usize,
}

impl ImageCache {
    pub fn get(&self, key: &str) -> Option<&ImageState> {
        self.map.get(key)
    }

    /// Mutable access for in-place updates (SVG raster fill). Callers that
    /// grow an entry's payload must re-sync the running cost via `resync_cost`.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut ImageState> {
        self.map.get_mut(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<ImageState> {
        let removed = self.map.remove(key);
        if let Some(state) = removed.as_ref() {
            self.bytes = self.bytes.saturating_sub(image_state_cost(state));
        }
        self.order.retain(|entry| entry != key);
        removed
    }

    pub fn insert(&mut self, key: String, value: ImageState) {
        self.bytes += image_state_cost(&value);
        if let Some(replaced) = self.map.insert(key.clone(), value) {
            self.bytes = self.bytes.saturating_sub(image_state_cost(&replaced));
        } else {
            self.order.push(key);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Total retained payload bytes (tracked incrementally; O(1)).
    pub fn cost_bytes(&self) -> usize {
        self.bytes
    }

    /// Recompute the running cost from scratch. Call after mutating an entry
    /// in place through `get_mut`.
    pub(super) fn resync_cost(&mut self) {
        self.bytes = self.map.values().map(image_state_cost).sum();
    }

    /// Evict oldest-inserted entries until under `budget`, skipping any key
    /// `keep` returns true for (the current document's images). Zero-cost
    /// entries (`Loading`/`Failed`) are never evicted: dropping them can't
    /// reach the budget, and evicting a `Failed` sentinel would silently
    /// re-enable fetch retries that the old unbounded cache never made.
    pub fn trim(&mut self, budget: usize, keep: impl Fn(&str) -> bool) {
        if self.bytes <= budget {
            return;
        }
        let map = &mut self.map;
        let bytes = &mut self.bytes;
        self.order.retain(|key| {
            if *bytes <= budget || keep(key) {
                return true;
            }
            let cost = map.get(key).map(image_state_cost).unwrap_or(0);
            if cost == 0 {
                return true;
            }
            map.remove(key);
            *bytes = bytes.saturating_sub(cost);
            false
        });
    }
}

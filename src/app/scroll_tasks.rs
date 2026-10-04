//! Scroll, height-measurement, and window-mode tasks issued from update.

use super::*;

pub(super) fn edge_scroll(
    id: iced::widget::Id,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    cursor: usize,
    total: usize,
    row_h: f32,
) -> Task<Message> {
    // List inside scrollable has small top/bottom padding (~6-8px each). Pad cur_bot
    // so the bottom edge of the *last* row is fully revealed instead of clipped.
    const PAD: f32 = 8.0;
    let Some(v) = viewport else {
        if total <= 1 {
            return Task::none();
        }
        let y = (cursor as f32 / (total - 1) as f32).clamp(0.0, 1.0);
        return iced::widget::operation::snap_to(
            id,
            iced::widget::scrollable::RelativeOffset { x: 0.0, y },
        );
    };
    let cur_top = cursor as f32 * row_h;
    let cur_bot = cur_top + row_h + PAD;
    let off = v.absolute_offset();
    let view_top = off.y;
    let view_h = v.bounds().height;
    let view_bot = view_top + view_h;
    let new_y = if cur_top < view_top {
        cur_top
    } else if cur_bot > view_bot {
        cur_bot - view_h
    } else {
        return Task::none();
    };
    iced::widget::operation::scroll_to(
        id,
        iced::widget::scrollable::AbsoluteOffset {
            x: 0.0,
            y: new_y.max(0.0),
        },
    )
}

pub(super) fn scroll_block_to_top(id: crate::ast::BlockId) -> Task<Message> {
    struct ScrollBlockToTop {
        body_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        target_top: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollBlockToTop {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            _bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            _translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.body_id) {
                self.content_top = Some(content_bounds.y);
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    self.target_top = Some((bounds.y - content_top).max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_top
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::RestoreBodyScroll(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollBlockToTop {
        body_id: App::scroll_id(),
        target_id: crate::render::block_anchor_id(id),
        content_top: None,
        target_top: None,
    })
}

/// Scroll the body so the given block lands slightly above center, using real
/// laid-out widget bounds (not height estimates). Used by find/highlight nav so
/// the matched word is always actually visible.
pub(super) fn scroll_block_to_center(id: crate::ast::BlockId) -> Task<Message> {
    struct ScrollBlockToCenter {
        body_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        view_h: f32,
        target_y: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollBlockToCenter {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            _translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.body_id) {
                self.content_top = Some(content_bounds.y);
                self.view_h = bounds.height;
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    let block_top = bounds.y - content_top;
                    // Place block slightly above center so following context shows.
                    let y = block_top + bounds.height * 0.5 - self.view_h * 0.38;
                    self.target_y = Some(y.max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_y
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::RestoreBodyScroll(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollBlockToCenter {
        body_id: App::scroll_id(),
        target_id: crate::render::block_anchor_id(id),
        content_top: None,
        view_h: 0.0,
        target_y: None,
    })
}

/// Harvest real laid-out heights for the given anchored block containers.
/// Feeds the virt-window `HeightCache` so prefix estimates converge.
pub(super) fn measure_block_heights(
    targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
    at_offset: f32,
) -> Task<Message> {
    struct MeasureHeights {
        targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
        at_offset: f32,
        out: Vec<(crate::ast::BlockId, f32)>,
    }

    impl iced::advanced::widget::Operation<Message> for MeasureHeights {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if let Some(bid) = id.and_then(|i| self.targets.get(i)) {
                self.out.push((*bid, bounds.height));
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            if self.out.is_empty() {
                iced::advanced::widget::operation::Outcome::None
            } else {
                iced::advanced::widget::operation::Outcome::Some(Message::BlockHeightsMeasured(
                    self.out.clone(),
                    self.at_offset,
                ))
            }
        }
    }

    if targets.is_empty() {
        return Task::none();
    }
    iced::advanced::widget::operate(MeasureHeights {
        targets,
        at_offset,
        out: Vec::new(),
    })
}

/// Preview-owned counterpart to `measure_block_heights`. The widget operation
/// is identical, but carries the selected path and preview generation so the
/// update handler can reject a result after navigation changed the panel.
pub(super) fn measure_full_mindmap_preview_block_heights(
    path: PathBuf,
    namespace: u64,
    identity: u64,
    generation: u64,
    targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
    at_offset: f32,
) -> Task<Message> {
    struct MeasurePreviewHeights {
        path: PathBuf,
        namespace: u64,
        identity: u64,
        generation: u64,
        targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
        at_offset: f32,
        out: Vec<(crate::ast::BlockId, f32)>,
    }

    impl iced::advanced::widget::Operation<Message> for MeasurePreviewHeights {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if let Some(block_id) = id.and_then(|widget_id| self.targets.get(widget_id)) {
                self.out.push((*block_id, bounds.height));
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            iced::advanced::widget::operation::Outcome::Some(
                Message::FullMindmapPreviewBlockHeightsMeasured {
                    path: self.path.clone(),
                    namespace: self.namespace,
                    identity: self.identity,
                    generation: self.generation,
                    measured: self.out.clone(),
                    at_offset: self.at_offset,
                },
            )
        }
    }

    if targets.is_empty() {
        return Task::none();
    }
    iced::advanced::widget::operate(MeasurePreviewHeights {
        path,
        namespace,
        identity,
        generation,
        targets,
        at_offset,
        out: Vec::new(),
    })
}

/// Scroll the vault results page just enough to bring the cursor's match block
/// fully into view, measuring its real bounds (blocks have variable height).
/// Only moves when the block is off-screen, like a code editor's cursor follow.
pub(super) fn scroll_vault_to_match(vis_idx: usize) -> Task<Message> {
    struct ScrollVaultToMatch {
        scroll_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        view_top: f32,
        view_h: f32,
        target_y: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollVaultToMatch {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.scroll_id) {
                self.content_top = Some(content_bounds.y);
                self.view_top = translation.y;
                self.view_h = bounds.height;
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    const PAD: f32 = 12.0;
                    let block_top = bounds.y - content_top;
                    let block_bot = block_top + bounds.height;
                    let view_top = self.view_top;
                    let view_bot = view_top + self.view_h;
                    let y = if block_top < view_top {
                        block_top - PAD
                    } else if block_bot > view_bot {
                        // Reveal the block's bottom; if taller than the viewport,
                        // pin its top so the match line stays visible.
                        let candidate = block_bot - self.view_h + PAD;
                        candidate.min(block_top - PAD)
                    } else {
                        return; // already fully visible — don't move
                    };
                    self.target_y = Some(y.max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_y
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::VaultScrollTo(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollVaultToMatch {
        scroll_id: App::vault_scroll_id(),
        target_id: App::vault_match_anchor_id(vis_idx),
        content_top: None,
        view_top: 0.0,
        view_h: 0.0,
        target_y: None,
    })
}

pub(super) fn refresh_window_mode(id: iced::window::Id) -> Task<Message> {
    iced::window::mode(id).map(Message::WindowModeChanged)
}

/// Sample the window mode now and again after the native transition settles.
///
/// macOS native fullscreen enter/exit animates; the resize/focus event that
/// triggers a refresh can fire *before* the mode flips, so a single immediate
/// query can read the stale (pre-transition) mode on exit. The delayed second
/// query lands after the animation completes and corrects the flag, restoring
/// the windowed header reserve. See the fullscreen-exit relayout bug.
pub(super) fn refresh_window_mode_after_native_transition(id: iced::window::Id) -> Task<Message> {
    // Sample immediately, then again after the native animation could plausibly
    // have settled. Two delayed samples (250ms + 600ms) because a single fixed
    // delay can still land before a slow fullscreen-exit animation finishes.
    let delayed = |ms: u64| {
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                id
            },
            Message::RefreshWindowModeSettled,
        )
    };
    Task::batch([refresh_window_mode(id), delayed(250), delayed(600)])
}

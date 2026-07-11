//! Native magnification input that Iced 0.14 does not yet expose.
//!
//! Winit receives macOS `PinchGesture` events, but Iced 0.14 drops them while
//! converting window events. This small AppKit monitor forwards only the
//! magnification delta through an async subscription; graph-specific zoom math
//! remains in `mindmap::MindmapProgram`.

#[cfg(target_os = "macos")]
mod imp {
    use block2::RcBlock;
    use futures::stream::{self, BoxStream};
    use futures::StreamExt;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc::{self, Receiver, Sender};

    struct Channel {
        wake: Sender<()>,
        receiver: Mutex<Option<Receiver<()>>>,
        pending_delta_bits: AtomicU64,
    }

    impl Channel {
        fn record(&self, delta: f32) {
            let delta = f64::from(delta);
            let mut current = self.pending_delta_bits.load(Ordering::Acquire);
            loop {
                let next_value = f64::from_bits(current) + delta;
                if !next_value.is_finite() {
                    return;
                }
                match self.pending_delta_bits.compare_exchange_weak(
                    current,
                    next_value.to_bits(),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return,
                    Err(actual) => current = actual,
                }
            }
        }

        fn take_pending_delta(&self) -> f32 {
            f64::from_bits(
                self.pending_delta_bits
                    .swap(0.0_f64.to_bits(), Ordering::AcqRel),
            ) as f32
        }
    }

    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    thread_local! {
        // AppKit's monitor token retains the supplied block. Keep that token
        // on the main thread for the process lifetime, then let normal thread
        // teardown release it rather than intentionally leaking native state.
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    /// Register a process-lifetime local AppKit monitor while on the main
    /// thread. Unit tests can construct `App` off-main-thread, so they simply
    /// skip this platform integration.
    pub fn install() {
        if MainThreadMarker::new().is_none() || MONITOR.with(|slot| slot.borrow().is_some()) {
            return;
        }
        let channel = CHANNEL.get_or_init(|| {
            let (wake, receiver) = mpsc::channel(1);
            Channel {
                wake,
                receiver: Mutex::new(Some(receiver)),
                pending_delta_bits: AtomicU64::new(0.0_f64.to_bits()),
            }
        });
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a non-null event for a local monitor, and
            // the callback returns this same still-valid event to AppKit.
            let delta = unsafe { event.as_ref() }.magnification() as f32;
            if delta.is_finite() && delta != 0.0 {
                channel.record(delta);
                // Capacity one is intentional: pending deltas are accumulated
                // atomically, and a single wake-up is enough to drain them.
                let _ = channel.wake.try_send(());
            }
            event.as_ptr()
        });
        // SAFETY: The callback returns the exact event pointer AppKit gave us,
        // satisfying the method's valid-pointer contract.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::Magnify, &block)
        };
        if let Some(monitor) = monitor {
            MONITOR.with(|slot| *slot.borrow_mut() = Some(monitor));
        } else {
            eprintln!("rmdv: could not install macOS pinch monitor");
        }
    }

    pub fn subscription() -> iced::Subscription<f32> {
        if CHANNEL.get().is_some() {
            iced::Subscription::run(events)
        } else {
            iced::Subscription::none()
        }
    }

    fn events() -> BoxStream<'static, f32> {
        let receiver = CHANNEL
            .get()
            .and_then(|channel| channel.receiver.lock().ok()?.take());
        match receiver {
            Some(receiver) => stream::unfold(receiver, |mut receiver| async {
                receiver.recv().await.map(|()| {
                    let delta = CHANNEL
                        .get()
                        .map(Channel::take_pending_delta)
                        .unwrap_or_default();
                    (delta, receiver)
                })
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn install() {}

    pub fn subscription() -> iced::Subscription<f32> {
        iced::Subscription::none()
    }
}

pub use imp::{install, subscription};

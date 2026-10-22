//! Route macOS Quit (⌘Q, the app menu's Quit) through the app's unsaved-edits
//! confirmation.
//!
//! Winit's app menu sends `terminate:` to `NSApplication`, which asks the
//! application delegate `applicationShouldTerminate:` and quits at once when
//! the delegate does not answer. Winit's delegate does not implement that
//! selector, so this module adds it to the same already-registered class, as
//! `macos_open` does for document opens. While the document has no unsaved
//! edits the answer is "terminate now", so a clean quit behaves exactly as
//! before; with unsaved edits the quit is cancelled and the app shows its
//! Save / Don't Save / Cancel confirmation, whose close ends the app.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the document has unsaved edits; the app updates it after every
/// message, and the AppKit callback reads it on the main thread.
static UNSAVED: AtomicBool = AtomicBool::new(false);

/// Record whether the document has unsaved edits.
pub fn set_unsaved(unsaved: bool) {
    UNSAVED.store(unsaved, Ordering::Relaxed);
}

/// What a quit does now: go ahead, or hand the decision to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitDecision {
    TerminateNow,
    AskTheApp,
}

/// The quit decision: with unsaved edits the app asks first, unless it
/// cannot be told (`delivered` is false), when a quit must not get stuck.
pub fn quit_decision(unsaved: bool, delivered: impl FnOnce() -> bool) -> QuitDecision {
    if unsaved && delivered() {
        QuitDecision::AskTheApp
    } else {
        QuitDecision::TerminateNow
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{quit_decision, QuitDecision, UNSAVED};
    use futures::stream::{self, BoxStream, StreamExt};
    use objc2::ffi;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc::{self, Receiver, Sender};

    /// `NSApplicationTerminateReply applicationShouldTerminate:(NSApplication *)`.
    const SHOULD_TERMINATE_ENCODING: &[u8] = b"Q@:@\0";

    struct Channel {
        sender: Sender<()>,
        receiver: Mutex<Option<Receiver<()>>>,
    }

    static CHANNEL: OnceLock<Channel> = OnceLock::new();

    /// Register the quit handler after winit has created its event loop.
    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            // App construction in unit tests is not on the AppKit main thread.
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = app.delegate() else {
            eprintln!("rmdv: macOS application delegate is unavailable");
            return;
        };
        let delegate: objc2::rc::Retained<AnyObject> = delegate.into();
        let class: &AnyClass = delegate.class();
        let selector = sel!(applicationShouldTerminate:);
        if class.instance_method(selector).is_some() {
            // Someone already answers quits; leave them in charge.
            return;
        }
        CHANNEL.get_or_init(|| {
            let (sender, receiver) = mpsc::channel(4);
            Channel {
                sender,
                receiver: Mutex::new(Some(receiver)),
            }
        });
        let implementation = unsafe {
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    &AnyObject,
                    Sel,
                    &NSApplication,
                ) -> NSApplicationTerminateReply,
                Imp,
            >(should_terminate)
        };
        let added = unsafe {
            ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                selector,
                implementation,
                SHOULD_TERMINATE_ENCODING.as_ptr().cast(),
            )
        };
        if !added.as_bool() {
            eprintln!("rmdv: could not register macOS quit handler");
        }
    }

    unsafe extern "C-unwind" fn should_terminate(
        _delegate: &AnyObject,
        _selector: Sel,
        _application: &NSApplication,
    ) -> NSApplicationTerminateReply {
        let unsaved = UNSAVED.load(Ordering::Relaxed);
        let delivered = || {
            CHANNEL
                .get()
                .is_some_and(|channel| channel.sender.try_send(()).is_ok())
        };
        match quit_decision(unsaved, delivered) {
            QuitDecision::TerminateNow => NSApplicationTerminateReply::TerminateNow,
            QuitDecision::AskTheApp => NSApplicationTerminateReply::TerminateCancel,
        }
    }

    /// One item per quit the app must decide on.
    pub fn subscription() -> iced::Subscription<()> {
        if CHANNEL.get().is_none() {
            return iced::Subscription::none();
        }
        iced::Subscription::run(events)
    }

    fn events() -> BoxStream<'static, ()> {
        let receiver = CHANNEL
            .get()
            .and_then(|channel| channel.receiver.lock().ok()?.take());
        match receiver {
            Some(receiver) => stream::unfold(receiver, |mut receiver| async {
                receiver.recv().await.map(|()| ((), receiver))
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    /// Elsewhere a quit closes the window, which sends a close request the
    /// app already guards.
    pub fn install() {}

    pub fn subscription() -> iced::Subscription<()> {
        iced::Subscription::none()
    }
}

pub use imp::{install, subscription};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_quit_terminates_without_asking() {
        let mut asked = false;
        let decision = quit_decision(false, || {
            asked = true;
            true
        });
        assert_eq!(decision, QuitDecision::TerminateNow);
        assert!(!asked, "a clean quit does not involve the app");
    }

    #[test]
    fn unsaved_quit_asks_the_app() {
        assert_eq!(quit_decision(true, || true), QuitDecision::AskTheApp);
    }

    #[test]
    fn unsaved_quit_terminates_when_the_app_cannot_be_told() {
        assert_eq!(quit_decision(true, || false), QuitDecision::TerminateNow);
    }
}

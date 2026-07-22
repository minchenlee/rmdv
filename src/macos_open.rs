//! Bridge macOS Finder open-document events into the Iced application.
//!
//! Finder sends document opens through AppKit rather than the process argv.
//! Winit owns the application delegate, so this module adds only the document-
//! open selectors to that already-registered delegate and leaves its
//! lifecycle handling untouched.

#[cfg(target_os = "macos")]
mod imp {
    use futures::stream::{self, BoxStream, StreamExt};
    use objc2::ffi;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationDelegateReply};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc::{self, Receiver, Sender};

    const OPEN_URLS_ENCODING: &[u8] = b"v@:@@\0";
    const OPEN_FILES_ENCODING: &[u8] = b"v@:@@\0";
    const OPEN_FILE_ENCODING: &[u8] = b"B@:@\0";

    struct Channel {
        sender: Sender<PathBuf>,
        receiver: Mutex<Option<Receiver<PathBuf>>>,
    }

    static CHANNEL: OnceLock<Channel> = OnceLock::new();

    /// Register the document handlers after winit has created its event loop.
    ///
    /// AppKit keeps the delegate as a weak reference, and winit retains its
    /// delegate in the event-loop value. Adding methods to that same class is
    /// therefore both smaller and safer than replacing the delegate and
    /// having to proxy winit's lifecycle callbacks.
    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            // App construction in unit tests is not guaranteed to be on the
            // AppKit main thread.
            return;
        };

        CHANNEL.get_or_init(|| {
            let (sender, receiver) = mpsc::channel(64);
            Channel {
                sender,
                receiver: Mutex::new(Some(receiver)),
            }
        });

        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = app.delegate() else {
            eprintln!("rmdv: macOS application delegate is unavailable");
            return;
        };
        let delegate: objc2::rc::Retained<AnyObject> = delegate.into();
        let class = delegate.class();
        add_method(
            class,
            sel!(application:openURLs:),
            unsafe {
                std::mem::transmute::<
                    unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication, &NSArray<NSURL>),
                    Imp,
                >(handle_open_urls)
            },
            OPEN_URLS_ENCODING,
        );
        add_method(
            class,
            sel!(application:openFiles:),
            unsafe {
                std::mem::transmute::<
                    unsafe extern "C-unwind" fn(
                        &AnyObject,
                        Sel,
                        &NSApplication,
                        &NSArray<NSString>,
                    ),
                    Imp,
                >(handle_open_files)
            },
            OPEN_FILES_ENCODING,
        );
        add_method(
            class,
            sel!(application:openFile:),
            unsafe {
                std::mem::transmute::<
                    unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication, &NSString) -> bool,
                    Imp,
                >(handle_open_file)
            },
            OPEN_FILE_ENCODING,
        );
    }

    /// Add a method to winit's already-registered delegate without replacing
    /// its `applicationDidFinishLaunching:` and termination handlers.
    fn add_method(class: &AnyClass, selector: Sel, implementation: Imp, encoding: &[u8]) {
        if class.instance_method(selector).is_some() {
            return;
        }

        let added = unsafe {
            ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                selector,
                implementation,
                encoding.as_ptr().cast(),
            )
        };
        if !added.as_bool() {
            eprintln!("rmdv: could not register macOS document-open handler");
        }
    }

    unsafe extern "C-unwind" fn handle_open_urls(
        _delegate: &AnyObject,
        _selector: Sel,
        _application: &NSApplication,
        urls: &NSArray<NSURL>,
    ) {
        for index in 0..urls.count() {
            let url = urls.objectAtIndex(index);
            if let Some(path) = url.path() {
                enqueue(PathBuf::from(path.to_string()));
            }
        }
    }

    unsafe extern "C-unwind" fn handle_open_files(
        _delegate: &AnyObject,
        _selector: Sel,
        _application: &NSApplication,
        filenames: &NSArray<NSString>,
    ) {
        let mut accepted = true;
        for index in 0..filenames.count() {
            if !enqueue(PathBuf::from(filenames.objectAtIndex(index).to_string())) {
                accepted = false;
            }
        }
        let reply = if accepted {
            NSApplicationDelegateReply::Success
        } else {
            NSApplicationDelegateReply::Failure
        };
        _application.replyToOpenOrPrint(reply);
    }

    unsafe extern "C-unwind" fn handle_open_file(
        _delegate: &AnyObject,
        _selector: Sel,
        _application: &NSApplication,
        filename: &NSString,
    ) -> bool {
        enqueue(PathBuf::from(filename.to_string()))
    }

    fn enqueue(path: PathBuf) -> bool {
        if let Some(channel) = CHANNEL.get() {
            if channel.sender.try_send(path).is_ok() {
                return true;
            }
            eprintln!("rmdv: macOS document-open queue is full");
        }
        false
    }

    pub fn subscription() -> iced::Subscription<PathBuf> {
        if CHANNEL.get().is_none() {
            return iced::Subscription::none();
        }
        iced::Subscription::run(events)
    }

    fn events() -> BoxStream<'static, PathBuf> {
        let receiver = CHANNEL
            .get()
            .and_then(|channel| channel.receiver.lock().ok()?.take());
        match receiver {
            Some(receiver) => stream::unfold(receiver, |mut receiver| async {
                receiver.recv().await.map(|path| (path, receiver))
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::PathBuf;

    pub fn install() {}

    pub fn subscription() -> iced::Subscription<PathBuf> {
        iced::Subscription::none()
    }
}

pub use imp::{install, subscription};

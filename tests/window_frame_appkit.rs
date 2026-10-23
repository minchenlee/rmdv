//! The window frame's screen list against real AppKit (MDV-048 P8). The
//! list is read on the main thread before the window opens, so this target
//! has no test harness: `main` runs each check on the main thread, before
//! any `NSApplication` exists. Nothing here opens a window.

#[cfg(target_os = "macos")]
mod appkit {
    use rmdv::window_frame::visible_screens;

    fn the_screen_list_is_there_before_the_app_exists() {
        let screens = visible_screens().expect("a screen list on the main thread");
        for screen in &screens {
            assert!(
                screen.width > 0.0 && screen.height > 0.0,
                "a usable visible area: {screen:?}"
            );
        }
    }

    fn off_the_main_thread_there_is_no_list() {
        let list = std::thread::spawn(visible_screens)
            .join()
            .expect("the thread ends");
        assert_eq!(list, None);
    }

    pub fn run() {
        let checks: [(&str, fn()); 2] = [
            (
                "the_screen_list_is_there_before_the_app_exists",
                the_screen_list_is_there_before_the_app_exists,
            ),
            (
                "off_the_main_thread_there_is_no_list",
                off_the_main_thread_there_is_no_list,
            ),
        ];
        for (name, check) in checks {
            check();
            println!("test {name} ... ok");
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    appkit::run();
}

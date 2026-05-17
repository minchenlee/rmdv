//! Poll-watch the themes directory for `*.toml` changes. Emits a single unit
//! event after each batch of file mtime changes so the app reloads at most
//! once per debounce interval.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub fn watch_subscription() -> iced::Subscription<()> {
    use iced::futures::stream;
    let Some(dir) = crate::theme_load::themes_dir() else {
        return iced::Subscription::none();
    };
    iced::Subscription::run_with(dir.clone(), move |d| {
        stream::unfold(WatchState::new(d.clone()), |mut s| async move {
            s.next_change().await.map(|()| ((), s))
        })
    })
}

struct WatchState {
    dir: PathBuf,
    snapshot: HashMap<PathBuf, SystemTime>,
    interval: tokio::time::Interval,
}

impl WatchState {
    fn new(dir: PathBuf) -> Self {
        let snapshot = scan(&dir);
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            dir,
            snapshot,
            interval,
        }
    }

    async fn next_change(&mut self) -> Option<()> {
        loop {
            self.interval.tick().await;
            let now = scan(&self.dir);
            if now != self.snapshot {
                self.snapshot = now;
                return Some(());
            }
        }
    }
}

fn scan(dir: &std::path::Path) -> HashMap<PathBuf, SystemTime> {
    let mut map = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return map;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        if let Ok(m) = entry.metadata().and_then(|m| m.modified()) {
            map.insert(p, m);
        }
    }
    map
}

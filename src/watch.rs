use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn watch_subscription(path: Option<PathBuf>) -> iced::Subscription<PathBuf> {
    use iced::futures::stream;
    let Some(p) = path else {
        return iced::Subscription::none();
    };
    iced::Subscription::run_with(p, |p| {
        stream::unfold(WatchState::new(Some(p.clone())), |mut s| async move {
            s.next_change().await.map(|p| (p, s))
        })
    })
}

struct WatchState {
    path: Option<PathBuf>,
    last: Option<SystemTime>,
    interval: tokio::time::Interval,
}

impl WatchState {
    fn new(path: Option<PathBuf>) -> Self {
        let last = path.as_deref().and_then(mtime);
        let mut interval = tokio::time::interval(Duration::from_millis(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            path,
            last,
            interval,
        }
    }

    async fn next_change(&mut self) -> Option<PathBuf> {
        let p = self.path.clone()?;
        loop {
            self.interval.tick().await;
            let now = mtime(&p);
            if now != self.last {
                self.last = now;
                return Some(p);
            }
        }
    }
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

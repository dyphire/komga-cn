use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryScanInterval {
    Disabled,
    Hourly,
    Every6h,
    Every12h,
    Daily,
    Weekly,
}

impl LibraryScanInterval {
    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Disabled => None,
            Self::Hourly => Some(Duration::from_secs(60 * 60)),
            Self::Every6h => Some(Duration::from_secs(6 * 60 * 60)),
            Self::Every12h => Some(Duration::from_secs(12 * 60 * 60)),
            Self::Daily => Some(Duration::from_secs(24 * 60 * 60)),
            Self::Weekly => Some(Duration::from_secs(7 * 24 * 60 * 60)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledLibraryScan {
    pub library_id: String,
    pub interval: LibraryScanInterval,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryScanScheduler {
    registry: HashMap<String, ScheduledLibraryScan>,
}

impl LibraryScanScheduler {
    pub fn schedule_scan(&mut self, library_id: impl Into<String>, interval: LibraryScanInterval) {
        let library_id = library_id.into();
        if interval == LibraryScanInterval::Disabled {
            self.registry.remove(&library_id);
            return;
        }

        self.registry.insert(
            library_id.clone(),
            ScheduledLibraryScan {
                library_id,
                interval,
            },
        );
    }

    pub fn scheduled_tasks(&self) -> Vec<ScheduledLibraryScan> {
        let mut tasks = self.registry.values().cloned().collect::<Vec<_>>();
        tasks.sort_by(|left, right| left.library_id.cmp(&right.library_id));
        tasks
    }
}

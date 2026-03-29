use std::collections::HashMap;
use std::time::Duration;

use super::{LibraryScanInterval, ScheduledLibraryScan, TaskQueueRecord};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryScanProfile {
    pub library_id: String,
    pub scan_startup: bool,
    pub scan_interval: String,
}

pub fn library_scan_interval_from_db(value: &str) -> LibraryScanInterval {
    match value.trim().to_ascii_uppercase().as_str() {
        "DISABLED" => LibraryScanInterval::Disabled,
        "HOURLY" => LibraryScanInterval::Hourly,
        "EVERY_6H" => LibraryScanInterval::Every6h,
        "EVERY_12H" => LibraryScanInterval::Every12h,
        "DAILY" => LibraryScanInterval::Daily,
        "WEEKLY" => LibraryScanInterval::Weekly,
        _ => LibraryScanInterval::Every6h,
    }
}

pub fn build_scheduled_library_scans(profiles: &[LibraryScanProfile]) -> Vec<ScheduledLibraryScan> {
    let mut scans = profiles
        .iter()
        .filter_map(|profile| {
            let interval = library_scan_interval_from_db(profile.scan_interval.as_str());
            if interval == LibraryScanInterval::Disabled {
                None
            } else {
                Some(ScheduledLibraryScan {
                    library_id: profile.library_id.clone(),
                    interval,
                })
            }
        })
        .collect::<Vec<_>>();

    scans.sort_by(|left, right| left.library_id.cmp(&right.library_id));
    scans
}

pub fn build_startup_library_scan_tasks(profiles: &[LibraryScanProfile]) -> Vec<TaskQueueRecord> {
    let library_ids = profiles
        .iter()
        .filter(|profile| profile.scan_startup)
        .map(|profile| profile.library_id.clone())
        .collect::<Vec<_>>();

    build_library_scan_tasks(&library_ids)
}

pub fn build_library_scan_tasks(library_ids: &[String]) -> Vec<TaskQueueRecord> {
    library_ids
        .iter()
        .map(|library_id| {
            TaskQueueRecord::new(
                format!("SCAN_LIBRARY:{library_id}"),
                100,
                Some(library_id.clone()),
            )
        })
        .collect()
}

pub fn library_scan_due_periods(profiles: &[LibraryScanProfile]) -> HashMap<String, Duration> {
    let mut periods = HashMap::new();
    for profile in profiles {
        let interval = library_scan_interval_from_db(profile.scan_interval.as_str());
        let Some(seconds) = interval.duration_seconds() else {
            continue;
        };
        periods.insert(profile.library_id.clone(), Duration::from_secs(seconds));
    }
    periods
}

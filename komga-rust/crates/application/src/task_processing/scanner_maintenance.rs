use std::collections::HashMap;
use std::time::Duration;

use super::{LibraryScanInterval, ScheduledLibraryScan, TaskQueueRecord};
use serde_json::json;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryScanProfile {
    pub library_id: String,
    pub scan_startup: bool,
    pub scan_interval: String,
}

pub fn library_scan_interval_from_db(value: &str) -> Result<LibraryScanInterval, String> {
    match value.trim().to_ascii_uppercase().as_str() {
        "DISABLED" => Ok(LibraryScanInterval::Disabled),
        "HOURLY" => Ok(LibraryScanInterval::Hourly),
        "EVERY_6H" => Ok(LibraryScanInterval::Every6h),
        "EVERY_12H" => Ok(LibraryScanInterval::Every12h),
        "DAILY" => Ok(LibraryScanInterval::Daily),
        "WEEKLY" => Ok(LibraryScanInterval::Weekly),
        _ => Err(format!("unsupported library scan interval: {value}")),
    }
}

pub fn build_scheduled_library_scans(
    profiles: &[LibraryScanProfile],
) -> Result<Vec<ScheduledLibraryScan>, String> {
    let mut scans = profiles
        .iter()
        .map(|profile| {
            library_scan_interval_from_db(profile.scan_interval.as_str())
                .map(|interval| (profile.library_id.clone(), interval))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(library_id, interval)| {
            (interval != LibraryScanInterval::Disabled).then_some(ScheduledLibraryScan {
                library_id,
                interval,
            })
        })
        .collect::<Vec<_>>();

    scans.sort_by(|left, right| left.library_id.cmp(&right.library_id));
    Ok(scans)
}

pub fn build_startup_library_scan_tasks(profiles: &[LibraryScanProfile]) -> Vec<TaskQueueRecord> {
    let library_ids = profiles
        .iter()
        .filter(|profile| profile.scan_startup)
        .map(|profile| profile.library_id.clone())
        .collect::<Vec<_>>();

    build_library_scan_tasks(&library_ids)
}

fn background_scan_task_id(library_id: &str) -> String {
    format!("SCAN_LIBRARY:{library_id}:DEEP:false")
}

fn background_scan_task_payload(library_id: &str, task_id: &str) -> String {
    json!({
        "libraryId": library_id,
        "scanDeep": false,
        "priority": 4,
        "groupId": serde_json::Value::Null,
        "uniqueId": task_id,
    })
    .to_string()
}

fn background_scan_task_record(library_id: &str) -> TaskQueueRecord {
    let task_id = background_scan_task_id(library_id);
    TaskQueueRecord::new(task_id.clone(), 4, None)
        .with_payload(background_scan_task_payload(library_id, &task_id))
}

pub fn build_library_scan_tasks(library_ids: &[String]) -> Vec<TaskQueueRecord> {
    library_ids
        .iter()
        .map(|library_id| background_scan_task_record(library_id))
        .collect()
}

pub fn library_scan_due_periods(
    profiles: &[LibraryScanProfile],
) -> Result<HashMap<String, Duration>, String> {
    let mut periods = HashMap::new();
    for profile in profiles {
        let interval = library_scan_interval_from_db(profile.scan_interval.as_str())?;
        let Some(seconds) = interval.duration_seconds() else {
            continue;
        };
        periods.insert(profile.library_id.clone(), Duration::from_secs(seconds));
    }
    Ok(periods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_scan_interval_returns_error_instead_of_defaulting() {
        let error = library_scan_interval_from_db("future-value")
            .expect_err("unknown scan interval should not silently default");
        assert!(error.contains("unsupported library scan interval"));
    }

    #[test]
    fn scheduled_scans_propagate_invalid_intervals() {
        let profiles = vec![LibraryScanProfile {
            library_id: "library-1".to_string(),
            scan_startup: false,
            scan_interval: "future-value".to_string(),
        }];

        let error = build_scheduled_library_scans(&profiles)
            .expect_err("invalid scan interval should fail scheduled scan building");
        assert!(error.contains("unsupported library scan interval"));
    }

    #[test]
    fn startup_library_scan_tasks_only_include_enabled_startup_profiles() {
        let profiles = vec![
            LibraryScanProfile {
                library_id: "library-2".to_string(),
                scan_startup: false,
                scan_interval: "DAILY".to_string(),
            },
            LibraryScanProfile {
                library_id: "library-1".to_string(),
                scan_startup: true,
                scan_interval: "DISABLED".to_string(),
            },
        ];

        let tasks = build_startup_library_scan_tasks(&profiles);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "SCAN_LIBRARY:library-1:DEEP:false");
        assert_eq!(tasks[0].priority, 4);
        assert_eq!(tasks[0].group, None);
    }

    #[test]
    fn due_periods_skip_disabled_and_map_supported_intervals() {
        let profiles = vec![
            LibraryScanProfile {
                library_id: "library-disabled".to_string(),
                scan_startup: false,
                scan_interval: "DISABLED".to_string(),
            },
            LibraryScanProfile {
                library_id: "library-hourly".to_string(),
                scan_startup: false,
                scan_interval: "HOURLY".to_string(),
            },
            LibraryScanProfile {
                library_id: "library-weekly".to_string(),
                scan_startup: false,
                scan_interval: "WEEKLY".to_string(),
            },
        ];

        let periods = library_scan_due_periods(&profiles)
            .expect("supported intervals should map to due periods");
        assert!(!periods.contains_key("library-disabled"));
        assert_eq!(
            periods.get("library-hourly"),
            Some(&Duration::from_secs(60 * 60))
        );
        assert_eq!(
            periods.get("library-weekly"),
            Some(&Duration::from_secs(7 * 24 * 60 * 60))
        );
    }
}

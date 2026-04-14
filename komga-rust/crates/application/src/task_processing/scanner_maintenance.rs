use super::LibraryScanInterval;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryScanProfile {
    pub library_id: String,
    pub scan_startup: bool,
    pub scan_interval: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedLibraryScanProfile {
    pub library_id: String,
    pub scan_startup: bool,
    pub scan_interval: LibraryScanInterval,
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

pub fn normalize_library_scan_profiles(
    profiles: &[LibraryScanProfile],
) -> Result<Vec<NormalizedLibraryScanProfile>, String> {
    let mut normalized = profiles
        .iter()
        .map(|profile| {
            library_scan_interval_from_db(profile.scan_interval.as_str()).map(|scan_interval| {
                NormalizedLibraryScanProfile {
                    library_id: profile.library_id.clone(),
                    scan_startup: profile.scan_startup,
                    scan_interval,
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    normalized.sort_by(|left, right| left.library_id.cmp(&right.library_id));
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_scans_propagate_invalid_intervals() {
        let profiles = vec![LibraryScanProfile {
            library_id: "library-1".to_string(),
            scan_startup: false,
            scan_interval: "future-value".to_string(),
        }];

        let error = normalize_library_scan_profiles(&profiles)
            .expect_err("invalid scan interval should fail normalization");
        assert!(error.contains("unsupported library scan interval"));
    }

    #[test]
    fn normalized_profiles_sort_and_propagate_supported_intervals() {
        let profiles = vec![
            LibraryScanProfile {
                library_id: "library-2".to_string(),
                scan_startup: false,
                scan_interval: "WEEKLY".to_string(),
            },
            LibraryScanProfile {
                library_id: "library-1".to_string(),
                scan_startup: true,
                scan_interval: "DAILY".to_string(),
            },
        ];

        let normalized = normalize_library_scan_profiles(&profiles)
            .expect("supported intervals should normalize");
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].library_id, "library-1");
        assert!(normalized[0].scan_startup);
        assert_eq!(normalized[0].scan_interval, LibraryScanInterval::Daily);
        assert_eq!(normalized[1].library_id, "library-2");
        assert_eq!(normalized[1].scan_interval, LibraryScanInterval::Weekly);
    }
}

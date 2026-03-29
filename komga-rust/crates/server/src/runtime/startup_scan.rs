use komga_infrastructure::task_queue::process_startup_library_scans;

use crate::config::RuntimeConfig;

pub fn bootstrap_library_scan(config: &RuntimeConfig) {
    process_startup_library_scans(config.clone());
}

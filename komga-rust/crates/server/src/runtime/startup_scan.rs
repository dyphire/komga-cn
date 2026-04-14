use komga_infrastructure::task_queue::process_startup_library_scans;

use crate::config::RuntimeConfig;

/// Keep server startup on the same schedule(Startup) drain path used by runtime bootstrap so
/// startup scan semantics have a single live source of truth.
pub fn bootstrap_library_scan(config: &RuntimeConfig) {
    process_startup_library_scans(config.clone());
}

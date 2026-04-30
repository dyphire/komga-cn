use komga_infrastructure::task_queue::worker_runtime::process_startup_library_scans;

use komga_config::env_config::RuntimeConfig;

/// Keep server startup on the same schedule(Startup) drain path used by runtime bootstrap so
/// startup scan semantics have a single live source of truth.
pub async fn bootstrap_library_scan(config: &RuntimeConfig) {
    process_startup_library_scans(crate::config::task_runtime_context(config).await).await;
}

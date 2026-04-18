use axum::Router;
use std::time::Instant;
use tokio::net::TcpListener;

use crate::composition::start_server;
use komga_config::env_config::RuntimeConfig;
use komga_interfaces::http::state::{RuntimeProfile, StartupTimingState};

pub fn build_router() -> Router {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    build_router_with_config(&config)
}

pub fn build_router_with_profile(profile: RuntimeProfile) -> Router {
    let config = RuntimeConfig::for_runtime_profile(match profile {
        RuntimeProfile::SnapshotAligned => komga_config::profile::RuntimeProfile::SnapshotAligned,
        RuntimeProfile::LiveLocaldb => komga_config::profile::RuntimeProfile::LiveLocaldb,
    });
    build_router_with_config(&config)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    let startup_started_at = Instant::now();
    let startup_timing = StartupTimingState::default();
    if matches!(
        config.runtime_profile,
        komga_config::profile::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(config);
    }
    start_server::build_router_with_config(config, startup_timing, startup_started_at)
}

pub fn build_router_without_runtime_workers_for_contract(config: &RuntimeConfig) -> Router {
    let startup_started_at = Instant::now();
    let startup_timing = StartupTimingState::default();
    if matches!(
        config.runtime_profile,
        komga_config::profile::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(config);
    }
    start_server::build_router_without_runtime_workers(config, startup_timing, startup_started_at)
}

pub fn prepare_startup_search_task_for_contract(
    config: &RuntimeConfig,
) -> std::io::Result<Option<&'static str>> {
    start_server::prepare_startup_search_task(config)
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    serve_with_config(listener, config).await
}

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    let startup_started_at = Instant::now();
    let startup_timing = StartupTimingState::default();
    if matches!(
        config.runtime_profile,
        komga_config::profile::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(&config);
    }
    start_server::serve_with_config(listener, config, startup_timing, startup_started_at).await
}

pub async fn validate_startup_schema_gate_for_contract(
    config: &RuntimeConfig,
) -> std::io::Result<()> {
    crate::bootstrap::validate_startup_schema_gate(config).await
}

pub async fn shutdown_runtime_for_contract() {
    start_server::shutdown_runtime_for_contract().await;
}

pub fn invalidate_sessions_for_user(user_id: &str) {
    komga_interfaces::http::identity_access::auth::invalidate_user_sessions(user_id)
}

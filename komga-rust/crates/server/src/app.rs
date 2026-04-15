use axum::Router;
use tokio::net::TcpListener;

use crate::composition::start_server;
use crate::config::RuntimeConfig;
use komga_interfaces::http::state::RuntimeProfile;

pub fn build_router() -> Router {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    build_router_with_config(&config)
}

pub fn build_router_with_profile(profile: RuntimeProfile) -> Router {
    let config = RuntimeConfig::for_runtime_profile(match profile {
        RuntimeProfile::SnapshotAligned => crate::config::RuntimeProfile::SnapshotAligned,
        RuntimeProfile::LiveLocaldb => crate::config::RuntimeProfile::LiveLocaldb,
    });
    build_router_with_config(&config)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    if matches!(
        config.runtime_profile,
        crate::config::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(config);
    }
    start_server::build_router_with_config(config)
}

pub fn build_router_without_runtime_workers_for_contract(config: &RuntimeConfig) -> Router {
    if matches!(
        config.runtime_profile,
        crate::config::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(config);
    }
    start_server::build_router_without_runtime_workers(config)
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
    if matches!(
        config.runtime_profile,
        crate::config::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(&config);
    }
    start_server::serve_with_config(listener, config).await
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

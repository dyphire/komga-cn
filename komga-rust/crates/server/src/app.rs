use axum::Router;
use std::time::Instant;
use tokio::net::TcpListener;

use crate::composition::start_server;
use komga_config::env_config::RuntimeConfig;
use komga_interfaces::state::{RuntimeProfile, StartupTimingState};

pub async fn build_router(config: &RuntimeConfig) -> std::io::Result<Router> {
    let startup_timing = StartupTimingState::default();
    start_server::build_router_with_config(config, startup_timing).await
}

pub async fn build_router_with_profile(profile: RuntimeProfile) -> Router {
    let config = RuntimeConfig::for_runtime_profile(match profile {
        RuntimeProfile::SnapshotAligned => komga_config::profile::RuntimeProfile::SnapshotAligned,
        RuntimeProfile::LiveLocaldb => komga_config::profile::RuntimeProfile::LiveLocaldb,
    });
    build_router_with_config(&config).await
}

pub async fn build_router_with_config(config: &RuntimeConfig) -> Router {
    build_router(config).await.expect("router should build")
}

pub async fn build_router_without_runtime_workers_for_contract(config: &RuntimeConfig) -> Router {
    let startup_timing = StartupTimingState::default();
    start_server::build_router_without_runtime_workers(config, startup_timing)
        .await
        .expect("router should build without runtime workers")
}

pub async fn serve(listener: TcpListener, config: RuntimeConfig) -> std::io::Result<()> {
    let startup_started_at = Instant::now();
    let startup_timing = StartupTimingState::default();
    serve_with_startup_timing(listener, config, startup_timing, startup_started_at).await
}

pub(crate) async fn serve_with_startup_timing(
    listener: TcpListener,
    config: RuntimeConfig,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> std::io::Result<()> {
    start_server::serve(listener, config, startup_timing, startup_started_at).await
}

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    serve(listener, config).await
}

pub async fn validate_startup_schema_gate_for_contract(
    config: &RuntimeConfig,
) -> std::io::Result<()> {
    crate::bootstrap::validate_startup_schema_gate(config).await
}

pub async fn shutdown_runtime_for_contract() {
    start_server::shutdown_runtime_for_contract().await;
}

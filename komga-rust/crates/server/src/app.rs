use axum::Router;
use komga_application::operational::StartupTimingState;
use komga_interfaces::state::RuntimeSseEventHub;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

use komga_config::env_config::RuntimeConfig;
use komga_interfaces::state::RuntimeProfile;

pub async fn build_router(config: &RuntimeConfig) -> std::io::Result<Router> {
    let startup_timing = StartupTimingState::default();
    crate::composition::build_router_with_config(config, startup_timing).await
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
    crate::composition::build_router_without_runtime_workers(config, startup_timing)
        .await
        .expect("router should build without runtime workers")
}

pub async fn build_router_with_runtime_events_for_contract(
    config: &RuntimeConfig,
    runtime_events: Arc<RuntimeSseEventHub>,
) -> Router {
    let startup_timing = StartupTimingState::default();
    crate::composition::build_router_with_runtime_events(
        config,
        crate::runtime::TaskRuntimeMode::WorkersEnabled { shutdown_rx: None },
        None,
        startup_timing,
        runtime_events,
    )
    .await
    .expect("router should build")
}

pub async fn build_router_without_runtime_workers_with_runtime_events_for_contract(
    config: &RuntimeConfig,
    runtime_events: Arc<RuntimeSseEventHub>,
) -> Router {
    let startup_timing = StartupTimingState::default();
    crate::composition::build_router_without_runtime_workers_with_runtime_events(
        config,
        startup_timing,
        runtime_events,
    )
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
    crate::composition::serve(listener, config, startup_timing, startup_started_at).await
}

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    serve(listener, config).await
}

pub async fn serve_with_startup_timing_for_contract(
    listener: TcpListener,
    config: RuntimeConfig,
    startup_timing: StartupTimingState,
) -> std::io::Result<()> {
    serve_with_startup_timing(listener, config, startup_timing, Instant::now()).await
}

pub async fn validate_startup_schema_gate_for_contract(
    config: &RuntimeConfig,
) -> std::io::Result<()> {
    crate::bootstrap::validate_startup_schema_gate(config).await
}

pub async fn shutdown_runtime_for_contract() {
    crate::composition::shutdown_runtime_for_contract().await;
}

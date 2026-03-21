use axum::Router;
use tokio::net::TcpListener;

use crate::config::RuntimeConfig;

mod compat_runtime;
mod placeholder_auth;
mod snapshots;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatProfile {
    SnapshotAligned,
    JavaLiveLocaldb,
}

pub fn build_router() -> Router {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    build_router_with_config(&config)
}

pub fn build_router_with_profile(profile: CompatProfile) -> Router {
    let config = match profile {
        CompatProfile::SnapshotAligned => RuntimeConfig::for_compat_profile(crate::config::CompatProfile::SnapshotAligned),
        CompatProfile::JavaLiveLocaldb => RuntimeConfig::for_compat_profile(crate::config::CompatProfile::JavaLiveLocaldb),
    };
    compat_runtime::build_router(&config)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    compat_runtime::build_router(config)
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    serve_with_config(listener, config).await
}

pub async fn serve_with_config(listener: TcpListener, config: RuntimeConfig) -> std::io::Result<()> {
    axum::serve(listener, build_router_with_config(&config)).await
}

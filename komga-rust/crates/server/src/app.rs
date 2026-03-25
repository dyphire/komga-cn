use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;

use crate::config::RuntimeConfig;
use crate::search;

mod compat_runtime;
pub mod discovery_auth;
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
        CompatProfile::SnapshotAligned => {
            RuntimeConfig::for_compat_profile(crate::config::CompatProfile::SnapshotAligned)
        }
        CompatProfile::JavaLiveLocaldb => {
            RuntimeConfig::for_compat_profile(crate::config::CompatProfile::JavaLiveLocaldb)
        }
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

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    config.resolve_webui_assets_layout().map_err(|error| {
        std::io::Error::other(format!("webui startup layout check failed: {error}"))
    })?;
    search::startup_recover(config.lucene_data_directory.as_path()).map_err(|error| {
        std::io::Error::other(format!("search startup recovery failed: {error}"))
    })?;
    axum::serve(listener, build_router_with_config(&config))
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        if let Ok(mut stream) = signal(SignalKind::terminate()) {
            let _ = stream.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

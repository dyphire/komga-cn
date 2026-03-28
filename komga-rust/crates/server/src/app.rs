use axum::Router;
use komga_persistence::sqlite::close_all_shared_pools;
use std::path::Path;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;

use crate::config::RuntimeConfig;
use crate::search::{self, SearchError};

mod auth;
pub(crate) mod compat_runtime;
pub mod discovery_auth;
mod runtime_auth;
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
    compat_runtime::build_router(&config, None, None)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    compat_runtime::build_router(config, None, None)
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
    let has_existing_search_index = config.lucene_data_directory.join("meta.json").exists();
    let mut startup_search_task = if has_existing_search_index {
        Some("UPGRADE_INDEX")
    } else {
        Some("REBUILD_INDEX")
    };

    match search::startup_recover(config.lucene_data_directory.as_path()) {
        Ok(()) => {}
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            search::reset_for_rebuild(config.lucene_data_directory.as_path()).map_err(|error| {
                std::io::Error::other(format!("search startup recovery failed: {error}"))
            })?;
            startup_search_task = Some("REBUILD_INDEX");
        }
        Err(error) => {
            return Err(std::io::Error::other(format!(
                "search startup recovery failed: {error}"
            )));
        }
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let router = compat_runtime::build_router(&config, Some(shutdown_tx), startup_search_task);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(shutdown_rx))
        .await
}

async fn shutdown_signal(mut shutdown_rx: watch::Receiver<bool>) {
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

    let shutdown_request = async move {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }
            if shutdown_rx.changed().await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = shutdown_request => {},
    }

    close_all_shared_pools().await;
}

pub fn invalidate_sessions_for_user(user_id: &str) {
    runtime_auth::invalidate_user_sessions(user_id)
}

pub fn configure_remember_me_store_root(store_root: &Path) -> String {
    runtime_auth::configure_remember_me_store(store_root)
}

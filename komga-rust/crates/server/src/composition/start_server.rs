use axum::Router;
use komga_infrastructure::sqlite::close_all_shared_pools;
use komga_infrastructure::{SearchError, reset_for_rebuild, startup_recover};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;

use crate::composition::http_state::compose_http_runtime;
use crate::config::RuntimeConfig;
use crate::runtime::background_workers::{prepare_task_queue, spawn_runtime_workers};

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    let background = prepare_task_queue(config, None);
    spawn_runtime_workers(
        background.task_queue.clone(),
        config.clone(),
        background.scheduled_scans.clone(),
    );
    let runtime = compose_http_runtime(config, background, None);
    komga_interfaces::http::router::build_router(
        runtime.profile,
        runtime.read_progress,
        runtime.discovery_auth,
        runtime.auth_db,
        runtime.operational,
    )
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

    match startup_recover(config.lucene_data_directory.as_path()) {
        Ok(()) => {}
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            reset_for_rebuild(config.lucene_data_directory.as_path()).map_err(|error| {
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
    let background = prepare_task_queue(&config, startup_search_task);
    spawn_runtime_workers(
        background.task_queue.clone(),
        config.clone(),
        background.scheduled_scans.clone(),
    );
    let runtime = compose_http_runtime(&config, background, Some(shutdown_tx));
    let router = komga_interfaces::http::router::build_router(
        runtime.profile,
        runtime.read_progress,
        runtime.discovery_auth,
        runtime.auth_db,
        runtime.operational,
    );

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

use axum::Router;
use komga_infrastructure::sqlite::close_all_shared_pools;
use komga_interfaces::state::StartupTimingState;
use std::net::SocketAddr;
use std::time::Duration;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::oneshot;
use tokio::sync::watch;

use crate::composition::http_state::compose_http_runtime;
use crate::runtime::{TaskRuntime, TaskRuntimeMode};
use komga_config::env_config::RuntimeConfig;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(crate) async fn build_router(
    config: &RuntimeConfig,
    mode: TaskRuntimeMode,
    shutdown_trigger: Option<watch::Sender<bool>>,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> std::io::Result<Router> {
    let task_runtime = TaskRuntime::start(config, mode).await?;
    let router_parts = task_runtime.into_router_parts();
    let router = finalize_router_startup(
        compose_http_runtime(
            config,
            router_parts.http,
            shutdown_trigger,
            startup_timing.clone(),
        ),
        startup_timing,
        startup_started_at,
    );
    Ok(router_parts.lifecycle.attach(router))
}

pub(crate) async fn build_router_with_config(
    config: &RuntimeConfig,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> std::io::Result<Router> {
    build_router(
        config,
        TaskRuntimeMode::WorkersEnabled { shutdown_rx: None },
        None,
        startup_timing,
        startup_started_at,
    )
    .await
}

pub(crate) async fn build_router_without_runtime_workers(
    config: &RuntimeConfig,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> std::io::Result<Router> {
    build_router(
        config,
        TaskRuntimeMode::WorkersDisabled,
        None,
        startup_timing,
        startup_started_at,
    )
    .await
}

pub(crate) async fn serve(
    listener: TcpListener,
    config: RuntimeConfig,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> std::io::Result<()> {
    crate::bootstrap::emit_startup_banner_and_runtime_event(&config).await;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let router = build_router(
        &config,
        TaskRuntimeMode::WorkersEnabled {
            shutdown_rx: Some(shutdown_rx.clone()),
        },
        Some(shutdown_tx.clone()),
        startup_timing,
        startup_started_at,
    )
    .await?;
    emit_server_bind_event(&listener);

    serve_router_with_shutdown_timeout(
        listener,
        router,
        shutdown_tx,
        shutdown_rx,
        SHUTDOWN_GRACE_PERIOD,
    )
    .await
}

async fn serve_router_with_shutdown_timeout(
    listener: TcpListener,
    router: Router,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    shutdown_grace_period: Duration,
) -> std::io::Result<()> {
    let (shutdown_lifecycle_tx, mut shutdown_lifecycle_rx) = oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal(
            shutdown_tx,
            shutdown_rx,
            shutdown_lifecycle_tx,
        ))
        .await
    });

    tokio::select! {
        result = &mut server => flatten_server_task_result(result),
        _ = &mut shutdown_lifecycle_rx => wait_for_server_shutdown_completion(
            &mut server,
            shutdown_grace_period,
        ).await,
    }
}

fn build_http_router(app: komga_interfaces::state::HttpAppState) -> Router {
    komga_interfaces::router::build_router(app)
}

fn finalize_router_startup(
    app: komga_interfaces::state::HttpAppState,
    startup_timing: StartupTimingState,
    startup_started_at: Instant,
) -> Router {
    startup_timing.record_application_started(startup_started_at.elapsed());
    let router = build_http_router(app);
    startup_timing.record_application_ready(startup_started_at.elapsed());
    router
}

async fn shutdown_signal(
    shutdown_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
    shutdown_lifecycle_tx: oneshot::Sender<()>,
) {
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

    let _ = shutdown_tx.send(true);
    complete_shutdown_lifecycle().await;
    let _ = shutdown_lifecycle_tx.send(());
}

async fn wait_for_server_shutdown_completion(
    server: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    shutdown_grace_period: Duration,
) -> std::io::Result<()> {
    match tokio::time::timeout(shutdown_grace_period, &mut *server).await {
        Ok(result) => flatten_server_task_result(result),
        Err(_) => {
            tracing::warn!(
                event = "server_shutdown_timeout",
                outcome = "forced",
                shutdown_grace_period_ms = shutdown_grace_period.as_millis() as u64,
                "Server graceful shutdown exceeded deadline; aborting lingering connections",
            );
            server.abort();
            match server.await {
                Ok(result) => result,
                Err(error) if error.is_cancelled() => Ok(()),
                Err(error) => Err(std::io::Error::other(format!(
                    "server shutdown task failed after abort: {error}"
                ))),
            }
        }
    }
}

fn flatten_server_task_result(
    result: Result<std::io::Result<()>, tokio::task::JoinError>,
) -> std::io::Result<()> {
    match result {
        Ok(result) => result,
        Err(error) => Err(std::io::Error::other(format!(
            "server task failed to join: {error}"
        ))),
    }
}

pub(crate) async fn shutdown_runtime_for_contract() {
    complete_shutdown_lifecycle().await;
}

fn emit_server_bind_event(listener: &TcpListener) {
    let bind_address = listener
        .local_addr()
        .map(|address| address.to_string())
        .unwrap_or_default();

    tracing::info!(
        event = "server_bind",
        outcome = "ready",
        bind_address = bind_address.as_str(),
        "Server listener ready",
    );
}

async fn complete_shutdown_lifecycle() {
    tracing::info!(
        event = "server_shutdown",
        outcome = "graceful",
        "Server shutdown requested",
    );
    close_all_shared_pools().await;
    tracing::info!(
        event = "shared_pool_close",
        outcome = "closed",
        "Closed shared sqlite pools",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::{Duration, timeout};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn graceful_shutdown_exits_even_when_keep_alive_connection_lingers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose local addr");
        let router = Router::new().route("/hold", get(|| async { "ok" }));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let server = tokio::spawn(serve_router_with_shutdown_timeout(
            listener,
            router,
            shutdown_tx.clone(),
            shutdown_rx,
            Duration::from_millis(100),
        ));

        let mut stream = TcpStream::connect(address)
            .await
            .expect("test client should connect");
        stream
            .write_all(b"GET /hold HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n")
            .await
            .expect("test client should write request");

        let mut response = Vec::new();
        let mut buffer = [0_u8; 256];
        while !response.ends_with(b"ok") {
            let read = timeout(Duration::from_secs(1), stream.read(&mut buffer))
                .await
                .expect("response read should not time out")
                .expect("response read should succeed");
            assert!(read > 0, "response should include the keep-alive payload");
            response.extend_from_slice(&buffer[..read]);
        }

        shutdown_tx
            .send(true)
            .expect("shutdown signal should be sent");

        timeout(Duration::from_secs(1), server)
            .await
            .expect("server should stop within the shutdown deadline")
            .expect("server task should join")
            .expect("server shutdown should succeed");
    }
}

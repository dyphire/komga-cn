use axum::Router;
use komga_infrastructure::sqlite::close_all_shared_pools;
use komga_infrastructure::{SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::oneshot;
use tokio::sync::watch;

use crate::composition::http_state::{HttpRuntimeState, compose_http_runtime};
use crate::config::{RuntimeConfig, WriterKind};
use crate::runtime::background_workers::{prepare_task_queue, spawn_runtime_workers};

#[derive(Clone, Copy)]
struct StartupSearchPlan {
    writer_decision: crate::config::WriterDecision,
    lifecycle: &'static str,
    startup_task: Option<&'static str>,
}

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(crate) fn prepare_startup_search_task(
    config: &RuntimeConfig,
) -> std::io::Result<Option<&'static str>> {
    Ok(plan_startup_search_task(config)?.startup_task)
}

fn plan_startup_search_task(config: &RuntimeConfig) -> std::io::Result<StartupSearchPlan> {
    let writer_decision = config.writer_decision(WriterKind::SearchIndex);
    if !writer_decision.allows_write() {
        return Ok(StartupSearchPlan {
            writer_decision,
            lifecycle: "skipped_writer_blocked",
            startup_task: None,
        });
    }

    match decide_startup_lifecycle(config.lucene_data_directory.as_path()) {
        Ok(SearchStartupLifecycle::Ready) => Ok(StartupSearchPlan {
            writer_decision,
            lifecycle: "ready",
            startup_task: None,
        }),
        Ok(SearchStartupLifecycle::RebuildRequired) => {
            prepare_for_rebuild(config.lucene_data_directory.as_path()).map_err(|error| {
                std::io::Error::other(format!(
                    "search startup rebuild preparation failed: {error}"
                ))
            })?;
            Ok(StartupSearchPlan {
                writer_decision,
                lifecycle: "rebuild_required",
                startup_task: Some("REBUILD_INDEX"),
            })
        }
        Err(error) => Err(std::io::Error::other(format!(
            "search startup lifecycle decision failed: {error}"
        ))),
    }
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    let background = prepare_task_queue(config, None);
    spawn_runtime_workers(
        background.task_queue.clone(),
        config.clone(),
        background.task_wakeup.clone(),
        None,
    );
    build_http_router(compose_http_runtime(config, background, None))
}

pub fn build_router_without_runtime_workers(config: &RuntimeConfig) -> Router {
    let background = prepare_task_queue(config, None);
    build_http_router(compose_http_runtime(config, background, None))
}

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    crate::bootstrap::emit_startup_banner_and_runtime_event(&config);
    let startup_search_plan = plan_startup_search_task_with_logging(&config)?;
    emit_server_bind_event(&listener);
    let startup_search_task = startup_search_plan.startup_task;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let background = prepare_task_queue(&config, startup_search_task);
    let worker_shutdown_rx = shutdown_rx.clone();
    spawn_runtime_workers(
        background.task_queue.clone(),
        config.clone(),
        background.task_wakeup.clone(),
        Some(worker_shutdown_rx),
    );
    let router = build_http_router(compose_http_runtime(
        &config,
        background,
        Some(shutdown_tx.clone()),
    ));

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

fn build_http_router(runtime: HttpRuntimeState) -> Router {
    komga_interfaces::http::router::build_router(
        runtime.profile,
        runtime.read_progress,
        runtime.discovery_auth,
        runtime.auth_db,
        runtime.operational,
    )
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

fn plan_startup_search_task_with_logging(
    config: &RuntimeConfig,
) -> std::io::Result<StartupSearchPlan> {
    match plan_startup_search_task(config) {
        Ok(startup_search_plan) => {
            emit_search_startup_event(config, startup_search_plan, None);
            Ok(startup_search_plan)
        }
        Err(error) => {
            emit_search_startup_event(config, failed_search_startup_plan(config), Some(&error));
            Err(error)
        }
    }
}

fn emit_search_startup_event(
    config: &RuntimeConfig,
    startup_search_plan: StartupSearchPlan,
    error: Option<&std::io::Error>,
) {
    let error_message = error.map_or_else(String::new, std::string::ToString::to_string);

    if error.is_some() {
        tracing::error!(
            event = "search_startup_decision",
            outcome = search_startup_outcome(startup_search_plan, error),
            search_writer_decision = search_writer_decision_label(startup_search_plan.writer_decision),
            search_writer_reason = search_writer_reason(startup_search_plan.writer_decision),
            search_startup_lifecycle = startup_search_plan.lifecycle,
            startup_task = startup_search_plan.startup_task.unwrap_or(""),
            lucene_data_directory = %config.lucene_data_directory.display(),
            error = error_message.as_str(),
            "Resolved startup search decision",
        );
    } else {
        tracing::info!(
            event = "search_startup_decision",
            outcome = search_startup_outcome(startup_search_plan, error),
            search_writer_decision = search_writer_decision_label(startup_search_plan.writer_decision),
            search_writer_reason = search_writer_reason(startup_search_plan.writer_decision),
            search_startup_lifecycle = startup_search_plan.lifecycle,
            startup_task = startup_search_plan.startup_task.unwrap_or(""),
            lucene_data_directory = %config.lucene_data_directory.display(),
            error = error_message.as_str(),
            "Resolved startup search decision",
        );
    }
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

fn search_writer_decision_label(decision: crate::config::WriterDecision) -> &'static str {
    match decision {
        crate::config::WriterDecision::Allowed => "allowed",
        crate::config::WriterDecision::Isolated => "isolated",
        crate::config::WriterDecision::Blocked { .. } => "blocked",
    }
}

fn search_writer_reason(decision: crate::config::WriterDecision) -> &'static str {
    match decision {
        crate::config::WriterDecision::Allowed | crate::config::WriterDecision::Isolated => "",
        crate::config::WriterDecision::Blocked { reason } => reason,
    }
}

fn failed_search_startup_plan(config: &RuntimeConfig) -> StartupSearchPlan {
    StartupSearchPlan {
        writer_decision: config.writer_decision(WriterKind::SearchIndex),
        lifecycle: "failed",
        startup_task: None,
    }
}

fn search_startup_outcome(
    startup_search_plan: StartupSearchPlan,
    error: Option<&std::io::Error>,
) -> &'static str {
    if error.is_some() {
        return "failed";
    }

    match startup_search_plan.lifecycle {
        "ready" => "ready",
        "rebuild_required" => "rebuild_required",
        "skipped_writer_blocked" => "skipped",
        _ => "ready",
    }
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

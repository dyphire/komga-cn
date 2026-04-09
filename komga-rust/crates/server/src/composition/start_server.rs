use axum::Router;
use komga_infrastructure::sqlite::close_all_shared_pools;
use komga_infrastructure::{SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;

use crate::composition::http_state::compose_http_runtime;
use crate::config::{RuntimeConfig, WriterKind};
use crate::runtime::background_workers::{prepare_task_queue, spawn_runtime_workers};

#[derive(Clone, Copy)]
struct StartupSearchPlan {
    writer_decision: crate::config::WriterDecision,
    lifecycle: &'static str,
    startup_task: Option<&'static str>,
}

pub(crate) fn prepare_startup_search_task(
    config: &RuntimeConfig,
) -> std::io::Result<Option<&'static str>> {
    Ok(plan_startup_search_task(config)?.startup_task)
}

fn plan_startup_search_task(config: &RuntimeConfig) -> std::io::Result<StartupSearchPlan> {
    let writer_decision = config.writer_decision(WriterKind::SearchIndex);
    if !config
        .writer_decision(WriterKind::SearchIndex)
        .allows_write()
    {
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
    crate::bootstrap::emit_startup_banner_and_runtime_event(&config);
    let startup_search_plan = plan_startup_search_task_with_logging(&config)?;
    emit_server_bind_event(&listener);
    let startup_search_task = startup_search_plan.startup_task;

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

    complete_shutdown_lifecycle().await;
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

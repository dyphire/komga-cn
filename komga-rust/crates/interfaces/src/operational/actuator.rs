use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{ActuatorService, actuator_metric_query_tags};
use serde_json::{Value, json};
use std::fs;

use crate::identity_access::auth::{Admin, resolved_request_auth_user, user_is_admin};
use crate::state::OperationalApiState;

const ACTUATOR_V3_JSON: &str = "application/vnd.spring-boot.actuator.v3+json";

fn actuator_json(payload: Value) -> Response {
    ([(header::CONTENT_TYPE, ACTUATOR_V3_JSON)], Json(payload)).into_response()
}

pub(crate) async fn actuator_root(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(ActuatorService::root_payload())
}

pub(crate) async fn actuator_health(
    headers: HeaderMap,
    State(app): State<OperationalApiState>,
) -> Response {
    let request_auth_user = resolved_request_auth_user(&app.identity, &headers).await;
    let include_details = request_auth_user.as_ref().is_some_and(user_is_admin);

    Json(
        ActuatorService::new(
            app.actuator_snapshots.as_ref(),
            app.operational_runtime.as_ref(),
        )
        .health_payload(include_details),
    )
    .into_response()
}

pub(crate) async fn actuator_info(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(
        ActuatorService::new(
            app.actuator_snapshots.as_ref(),
            app.operational_runtime.as_ref(),
        )
        .info_payload(),
    )
}

pub(crate) async fn actuator_logfile(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    let logfile = match fs::read_to_string(app.operational.runtime.log_file.as_path()) {
        Ok(logfile) => logfile,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "log file not found",
                    "path": app.operational.runtime.log_file.to_string_lossy().to_string(),
                })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        logfile,
    )
        .into_response()
}

pub(crate) async fn actuator_shutdown(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    app.operational
        .sse
        .lock()
        .expect("sse state lock should not be poisoned")
        .accepting_connections = false;

    if let Some(trigger) = app.operational.shutdown_trigger.as_ref() {
        let _ = trigger.send(true);
    }

    Json(json!({ "message": "Shutting down, bye..." })).into_response()
}

pub(crate) async fn actuator_metrics_index(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(ActuatorService::metrics_index_payload())
}

pub(crate) async fn actuator_metric_detail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
    AxumPath(metric_name): AxumPath<String>,
) -> Response {
    let tag_filters = actuator_metric_query_tags(uri.query());
    let service = ActuatorService::new(
        app.actuator_snapshots.as_ref(),
        app.operational_runtime.as_ref(),
    );

    match service
        .metric_detail_payload(&metric_name, &tag_filters)
        .await
    {
        Ok(Some(metric)) => actuator_json(metric),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response(),
    }
}

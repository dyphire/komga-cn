use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    ActuatorBuildInfo, ActuatorDatabaseHealthReport, ActuatorDatasourceHealthReport,
    ActuatorDiskSpaceHealthReport, ActuatorHealthReport, ActuatorHealthStatus,
    ActuatorInfoSnapshot, ActuatorMetricDetail, ActuatorOsInfo, ActuatorPingHealthReport,
    ActuatorProcessInfo, ActuatorService,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;

use crate::identity_access::auth::{Admin, resolved_request_auth_user};
use crate::state::OperationalApiState;
use komga_application::identity_access::user_is_admin;

const ACTUATOR_V3_JSON: &str = "application/vnd.spring-boot.actuator.v3+json";
const PRODUCT_GROUP: &str = "huihuimoe";
const PRODUCT_ARTIFACT: &str = "komga";
const PRODUCT_NAME: &str = "komga-rust";
const DEFAULT_PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn actuator_json(payload: Value) -> Response {
    ([(header::CONTENT_TYPE, ACTUATOR_V3_JSON)], Json(payload)).into_response()
}

pub(crate) async fn actuator_root(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_root_payload())
}

pub(crate) async fn actuator_health(
    headers: HeaderMap,
    State(app): State<OperationalApiState>,
) -> Response {
    let include_details = match resolved_request_auth_user(&app.identity, &headers).await {
        Ok(Some(user)) => user_is_admin(&user),
        Ok(None) => false,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let health = ActuatorService::new(
        app.actuator_snapshots.as_ref(),
        app.operational_runtime.as_ref(),
    )
    .health_report();

    Json(actuator_health_payload(health, include_details)).into_response()
}

pub(crate) async fn actuator_info(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_info_payload(
        ActuatorService::new(
            app.actuator_snapshots.as_ref(),
            app.operational_runtime.as_ref(),
        )
        .info_snapshot(),
    ))
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
    app.operational.sse.stop_accepting();

    if let Some(trigger) = app.operational.shutdown_trigger.as_ref() {
        trigger.request_shutdown();
    }

    Json(json!({ "message": "Shutting down, bye..." })).into_response()
}

pub(crate) async fn actuator_metrics_index(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_metrics_index_payload())
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

    match service.metric_detail(&metric_name, &tag_filters).await {
        Ok(Some(metric)) => actuator_json(actuator_metric_detail_payload(metric)),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(?error, "actuator metric detail failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("{error:#}") })),
            )
                .into_response()
        }
    }
}

fn actuator_root_payload() -> Value {
    json!({
        "_links": actuator_root_links(),
    })
}

fn actuator_root_links() -> Value {
    let links = [
        ("self", "/actuator", false),
        ("beans", "/actuator/beans", false),
        ("caches", "/actuator/caches", false),
        ("conditions", "/actuator/conditions", false),
        ("configprops", "/actuator/configprops", false),
        ("env", "/actuator/env", false),
        ("env-toMatch", "/actuator/env/{toMatch}", true),
        ("flyway", "/actuator/flyway", false),
        ("health", "/actuator/health", false),
        ("health-path", "/actuator/health/{*path}", true),
        ("heapdump", "/actuator/heapdump", false),
        ("httpexchanges", "/actuator/httpexchanges", false),
        ("info", "/actuator/info", false),
        ("logfile", "/actuator/logfile", false),
        ("loggers", "/actuator/loggers", false),
        ("loggers-name", "/actuator/loggers/{name}", true),
        ("mappings", "/actuator/mappings", false),
        ("metrics", "/actuator/metrics", false),
        (
            "metrics-requiredMetricName",
            "/actuator/metrics/{requiredMetricName}",
            true,
        ),
        ("scheduledtasks", "/actuator/scheduledtasks", false),
        ("shutdown", "/actuator/shutdown", false),
        ("threaddump", "/actuator/threaddump", false),
    ];

    Value::Object(serde_json::Map::from_iter(links.into_iter().map(
        |(name, href, templated)| {
            (
                name.to_string(),
                json!({
                    "href": href,
                    "templated": templated,
                }),
            )
        },
    )))
}

fn actuator_health_payload(report: ActuatorHealthReport, include_details: bool) -> Value {
    if !include_details {
        return json!({ "status": actuator_health_status(report.status) });
    }

    json!({
        "status": actuator_health_status(report.status),
        "components": {
            "db": actuator_database_health_payload(report.db),
            "diskSpace": actuator_disk_space_payload(report.disk_space),
            "ping": actuator_ping_payload(report.ping),
        }
    })
}

fn actuator_database_health_payload(report: ActuatorDatabaseHealthReport) -> Value {
    json!({
        "status": actuator_health_status(report.status),
        "components": {
            "sqliteDataSourceRW": actuator_datasource_health_payload(report.sqlite_rw),
            "sqliteDataSourceRO": actuator_datasource_health_payload(report.sqlite_ro),
            "tasksDataSourceRW": actuator_datasource_health_payload(report.tasks_rw),
            "tasksDataSourceRO": actuator_datasource_health_payload(report.tasks_ro),
        }
    })
}

fn actuator_datasource_health_payload(report: ActuatorDatasourceHealthReport) -> Value {
    json!({
        "status": actuator_health_status(report.status),
        "details": {
            "database": "SQLite",
            "validationQuery": "isValid()",
        }
    })
}

fn actuator_disk_space_payload(report: ActuatorDiskSpaceHealthReport) -> Value {
    match (report.total, report.free) {
        (Some(total), Some(free)) => json!({
            "status": actuator_health_status(report.status),
            "details": {
                "total": total,
                "free": free,
                "threshold": report.threshold,
                "path": report.path,
            }
        }),
        _ => json!({
            "status": actuator_health_status(report.status),
            "details": {
                "threshold": report.threshold,
                "path": report.path,
            }
        }),
    }
}

fn actuator_ping_payload(report: ActuatorPingHealthReport) -> Value {
    json!({ "status": actuator_health_status(report.status) })
}

fn actuator_health_status(status: ActuatorHealthStatus) -> &'static str {
    match status {
        ActuatorHealthStatus::Up => "UP",
        ActuatorHealthStatus::Down => "DOWN",
    }
}

fn actuator_info_payload(snapshot: ActuatorInfoSnapshot) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("build".to_string(), build_info_json(&snapshot.build));
    payload.insert("os".to_string(), os_info_json(snapshot.os));
    payload.insert("process".to_string(), process_info_json(snapshot.process));

    if snapshot.build.git_branch.is_some()
        || snapshot.build.git_commit_id.is_some()
        || snapshot.build.git_commit_time.is_some()
    {
        payload.insert(
            "git".to_string(),
            json!({
                "branch": snapshot.build.git_branch,
                "commit": {
                    "id": snapshot.build.git_commit_id,
                    "time": snapshot.build.git_commit_time,
                }
            }),
        );
    }
    Value::Object(payload)
}

fn process_info_json(process: ActuatorProcessInfo) -> Value {
    json!({
        "pid": process.pid,
        "parentPid": process.parent_pid,
        "cpus": process.cpus,
        "virtualThreads": process.virtual_threads,
        "memory": {
            "heap": {
                "used": process.memory.heap_used,
                "committed": process.memory.heap_committed,
                "max": process.memory.heap_max,
            },
            "nonHeap": {
                "used": process.memory.non_heap_used,
                "committed": process.memory.non_heap_committed,
                "max": process.memory.non_heap_max,
            }
        }
    })
}

fn build_info_json(build: &ActuatorBuildInfo) -> Value {
    let version = build
        .version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_PRODUCT_VERSION.to_string());

    Value::Object(serde_json::Map::from_iter([
        (
            "artifact".to_string(),
            Value::String(PRODUCT_ARTIFACT.to_string()),
        ),
        ("name".to_string(), Value::String(PRODUCT_NAME.to_string())),
        ("version".to_string(), Value::String(version)),
        (
            "group".to_string(),
            Value::String(PRODUCT_GROUP.to_string()),
        ),
    ]))
}

fn os_info_json(os: ActuatorOsInfo) -> Value {
    let mut payload = serde_json::Map::from_iter([
        ("name".to_string(), Value::String(os.name)),
        ("arch".to_string(), Value::String(os.arch)),
    ]);

    if let Some(version) = os.version {
        payload.insert("version".to_string(), Value::String(version));
    }

    Value::Object(payload)
}

fn actuator_metrics_index_payload() -> Value {
    json!({
        "names": ActuatorService::metric_names(),
    })
}

fn actuator_metric_query_tags(query: Option<&str>) -> HashMap<String, String> {
    query
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.strip_prefix("tag="))
        .filter_map(|pair| pair.split_once(':'))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn actuator_metric_detail_payload(metric: ActuatorMetricDetail) -> Value {
    let mut payload = serde_json::Map::from_iter([
        ("name".to_string(), Value::String(metric.name)),
        ("description".to_string(), Value::String(metric.description)),
        (
            "measurements".to_string(),
            Value::Array(
                metric
                    .measurements
                    .into_iter()
                    .map(|measurement| {
                        json!({
                            "statistic": measurement.statistic,
                            "value": measurement.value,
                        })
                    })
                    .collect(),
            ),
        ),
        (
            "availableTags".to_string(),
            Value::Array(
                metric
                    .available_tags
                    .into_iter()
                    .map(|tag| {
                        json!({
                            "tag": tag.tag,
                            "values": tag.values,
                        })
                    })
                    .collect(),
            ),
        ),
    ]);
    if let Some(base_unit) = metric.base_unit {
        payload.insert("baseUnit".to_string(), Value::String(base_unit));
    }

    Value::Object(payload)
}

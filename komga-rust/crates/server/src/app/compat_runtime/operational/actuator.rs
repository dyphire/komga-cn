use axum::Json;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::app::runtime_auth::require_admin;

use super::super::OperationalState;

pub(in crate::app::compat_runtime) async fn actuator_root(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "_links": {
            "self": {"href": "/actuator", "templated": false},
            "health": {"href": "/actuator/health", "templated": false},
            "info": {"href": "/actuator/info", "templated": false},
            "metrics": {"href": "/actuator/metrics", "templated": false},
            "logfile": {"href": "/actuator/logfile", "templated": false},
            "shutdown": {"href": "/actuator/shutdown", "templated": false}
        }
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_health(
    Extension(state): Extension<OperationalState>,
) -> Response {
    let db_ready = state.runtime.database_file.exists();
    let tasks_ready = state.runtime.tasks_db_file.exists();
    let status = if db_ready { "UP" } else { "DOWN" };

    Json(json!({
        "status": status,
        "components": {
            "db": {
                "status": if db_ready { "UP" } else { "DOWN" },
                "details": {
                    "database": "sqlite",
                    "path": state.runtime.database_file.to_string_lossy().to_string(),
                }
            },
            "tasksDb": {
                "status": if tasks_ready { "UP" } else { "DOWN" },
                "details": {
                    "path": state.runtime.tasks_db_file.to_string_lossy().to_string(),
                }
            }
        }
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_info(
    Extension(_state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let build_time = std::env::var("BUILD_TIME").ok();
    let commit_time = std::env::var("GIT_COMMIT_TIME").ok();
    let commit_id = std::env::var("GIT_COMMIT_ID").ok();
    let branch = std::env::var("GIT_BRANCH").ok();
    let version = std::env::var("KOMGA_VERSION").ok();

    let mut payload = serde_json::Map::new();
    if branch.is_some() || commit_id.is_some() || commit_time.is_some() {
        payload.insert(
            "git".to_string(),
            json!({
                "branch": branch,
                "commit": {
                    "id": commit_id,
                    "time": commit_time,
                }
            }),
        );
    }
    if version.is_some() || build_time.is_some() {
        payload.insert(
            "build".to_string(),
            json!({
                "version": version,
                "time": build_time,
            }),
        );
    }

    Json(Value::Object(payload)).into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_logfile(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let logfile = match fs::read_to_string(state.runtime.log_file.as_path()) {
        Ok(logfile) => logfile,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "log file not found",
                    "path": state.runtime.log_file.to_string_lossy().to_string(),
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

pub(in crate::app::compat_runtime) async fn actuator_shutdown(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned")
        .accepting_connections = false;

    if let Some(trigger) = state.shutdown_trigger.as_ref() {
        let _ = trigger.send(true);
    }

    Json(json!({ "message": "Shutting down, bye..." })).into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_metrics_index(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "names": [
            "komga.tasks.execution",
            "komga.tasks.failure",
            "komga.libraries",
            "komga.series",
            "komga.books",
            "komga.books.filesize",
            "komga.sidecars",
            "komga.collections",
            "komga.readlists",
        ]
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_metric_detail(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(metric_name): AxumPath<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match metric_detail_json(
        state.runtime.database_file.as_path(),
        state.runtime.tasks_db_file.as_path(),
        &metric_name,
        &uri,
    )
    .await
    {
        Ok(Some(metric)) => Json(metric).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response(),
    }
}

async fn metric_detail_json(
    database_file: &Path,
    tasks_db_file: &Path,
    metric_name: &str,
    uri: &Uri,
) -> Result<Option<Value>, String> {
    let tag_filters = metric_query_tags(uri);

    match metric_name {
        "komga.tasks.execution" => Ok(Some(
            metric_tasks_execution(tasks_db_file, tag_filters.get("type").map(String::as_str))
                .await?,
        )),
        "komga.tasks.failure" => Ok(Some(metric_tasks_failure(database_file).await?)),
        "komga.libraries" => Ok(Some(
            simple_metric(
                metric_name,
                "Libraries count",
                scalar_metric(
                    database_file,
                    "SELECT CAST(COUNT(*) AS REAL) AS VALUE \
                     FROM LIBRARY",
                )
                .await?,
            )
            .await,
        )),
        "komga.series" => Ok(Some(
            metric_library_value(
                database_file,
                metric_name,
                "Series count grouped by library",
                "SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE \
                 FROM SERIES \
                 GROUP BY LIBRARY_ID \
                 ORDER BY LIBRARY_ID",
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books" => Ok(Some(
            metric_library_value(
                database_file,
                metric_name,
                "Books count grouped by library",
                "SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE \
                 FROM BOOK \
                 GROUP BY LIBRARY_ID \
                 ORDER BY LIBRARY_ID",
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books.filesize" => Ok(Some(
            metric_library_value(
                database_file,
                metric_name,
                "Books file size grouped by library",
                "SELECT LIBRARY_ID, CAST(COALESCE(SUM(FILE_SIZE), 0) AS REAL) AS VALUE \
                 FROM BOOK \
                 GROUP BY LIBRARY_ID \
                 ORDER BY LIBRARY_ID",
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.sidecars" => Ok(Some(
            metric_library_value(
                database_file,
                metric_name,
                "Sidecars count grouped by library",
                "SELECT LIBRARY_ID, CAST(COUNT(*) AS REAL) AS VALUE \
                 FROM SIDECAR \
                 GROUP BY LIBRARY_ID \
                 ORDER BY LIBRARY_ID",
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.collections" => Ok(Some(
            simple_metric(
                metric_name,
                "Collections count",
                scalar_metric(
                    database_file,
                    "SELECT CAST(COUNT(*) AS REAL) AS VALUE \
                                              FROM COLLECTION",
                )
                .await?,
            )
            .await,
        )),
        "komga.readlists" => Ok(Some(
            simple_metric(
                metric_name,
                "Read lists count",
                scalar_metric(
                    database_file,
                    "SELECT CAST(COUNT(*) AS REAL) AS VALUE \
                                              FROM READLIST",
                )
                .await?,
            )
            .await,
        )),
        _ => Ok(None),
    }
}

fn metric_query_tags(uri: &Uri) -> HashMap<String, String> {
    uri.query()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.strip_prefix("tag="))
        .filter_map(|pair| pair.split_once(':'))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

async fn metric_tasks_execution(
    tasks_db_file: &Path,
    task_type: Option<&str>,
) -> Result<Value, String> {
    let pool = connect_pool(tasks_db_file, 1)
        .await
        .map_err(|error| format!("open tasks db for metrics: {error}"))?;
    let rows = sqlx::query(
        "SELECT SIMPLE_TYPE, CAST(COUNT(*) AS REAL) AS VALUE \
         FROM TASK \
         GROUP BY SIMPLE_TYPE \
         ORDER BY SIMPLE_TYPE",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query task execution metrics: {error}"))?;

    let values = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("SIMPLE_TYPE"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>();

    let count = if let Some(task_type) = task_type {
        values
            .iter()
            .find(|(kind, _)| kind.as_str() == task_type)
            .map(|(_, value)| *value)
            .unwrap_or(0.0)
    } else {
        values.iter().map(|(_, value)| *value).sum::<f64>()
    };

    let tags = values
        .iter()
        .map(|(kind, _)| kind.clone())
        .collect::<Vec<_>>();

    Ok(json!({
        "name": "komga.tasks.execution",
        "description": "Task execution statistics",
        "measurements": [{ "statistic": "COUNT", "value": count }],
        "availableTags": [
            {
                "tag": "type",
                "values": tags,
            }
        ]
    }))
}

async fn metric_tasks_failure(database_file: &Path) -> Result<Value, String> {
    let failures = scalar_metric(
        database_file,
        "SELECT CAST(COUNT(*) AS REAL) AS VALUE \
         FROM HISTORICAL_EVENT \
         WHERE TYPE LIKE '%TASK%' \
         AND TYPE LIKE '%FAIL%'",
    )
    .await?;

    Ok(json!({
        "name": "komga.tasks.failure",
        "description": "Count of failed tasks",
        "measurements": [{ "statistic": "COUNT", "value": failures }],
        "availableTags": [],
    }))
}

async fn metric_library_value(
    database_file: &Path,
    name: &str,
    description: &str,
    sql: &str,
    requested_library: Option<&str>,
) -> Result<Value, String> {
    let values = grouped_library_metric(database_file, sql).await?;

    let value = match requested_library {
        Some(library) => values
            .iter()
            .find(|(candidate, _)| candidate == library)
            .map(|(_, value)| *value)
            .unwrap_or(0.0),
        None => values.iter().map(|(_, value)| *value).sum::<f64>(),
    };

    Ok(json!({
        "name": name,
        "description": description,
        "measurements": [{ "statistic": "VALUE", "value": value }],
        "availableTags": [
            {
                "tag": "library",
                "values": values
                    .iter()
                    .map(|(library, _)| library.clone())
                    .collect::<Vec<_>>(),
            }
        ]
    }))
}

async fn grouped_library_metric(
    database_file: &Path,
    sql: &str,
) -> Result<Vec<(String, f64)>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open main db for grouped metrics: {error}"))?;

    let rows = sqlx::query(sql)
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query grouped library metrics: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("LIBRARY_ID"),
                row.get::<f64, _>("VALUE"),
            )
        })
        .collect::<Vec<_>>())
}

async fn scalar_metric(database_file: &Path, sql: &str) -> Result<f64, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open main db for scalar metric: {error}"))?;

    let row = sqlx::query(sql)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query scalar metric: {error}"))?;

    Ok(row.map(|value| value.get::<f64, _>("VALUE")).unwrap_or(0.0))
}

async fn simple_metric(name: &str, description: &str, value: f64) -> Value {
    json!({
        "name": name,
        "description": description,
        "measurements": [{ "statistic": "VALUE", "value": value }],
        "availableTags": [],
    })
}

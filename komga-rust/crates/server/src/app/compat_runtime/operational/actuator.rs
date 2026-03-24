use axum::Json;
use axum::extract::Extension;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::app::placeholder_auth::require_admin;

use super::super::{
    DEFAULT_BUILD_TIME, DEFAULT_GIT_BRANCH, DEFAULT_GIT_COMMIT_ID, DEFAULT_GIT_COMMIT_TIME,
    DEFAULT_LOGFILE, OperationalState,
};
use super::helpers::env_or_default;

pub(in crate::app::compat_runtime) async fn health_live() -> Response {
    Json(json!({ "status": "UP" })).into_response()
}

pub(in crate::app::compat_runtime) async fn health_ready() -> Response {
    Json(json!({ "status": "UP" })).into_response()
}

pub(in crate::app::compat_runtime) async fn metrics_text() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        "# HELP komga_runtime_up Rust runtime health state\n# TYPE komga_runtime_up gauge\nkomga_runtime_up 1\n",
    )
        .into_response()
}

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
            "shutdown": {"href": "/actuator/shutdown", "templated": false},
            "beans": {"href": "/actuator/beans", "templated": false},
        }
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_beans(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "contexts": {
            "application": {
                "beans": {
                    "compatRuntimeRouter": {
                        "aliases": [],
                        "scope": "singleton",
                        "type": "komga_rust::app::compat_runtime::Router",
                        "dependencies": [],
                    }
                }
            }
        }
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_health() -> Response {
    Json(json!({ "status": "UP" })).into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_info(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "git": {
            "branch": env_or_default("GIT_BRANCH", DEFAULT_GIT_BRANCH),
            "commit": {
                "id": env_or_default("GIT_COMMIT_ID", DEFAULT_GIT_COMMIT_ID),
                "time": env_or_default("GIT_COMMIT_TIME", DEFAULT_GIT_COMMIT_TIME),
            }
        },
        "build": {
            "name": "Komga",
            "artifact": "komga",
            "group": "org.gotson",
            "version": env_or_default("KOMGA_VERSION", env!("CARGO_PKG_VERSION")),
            "time": env_or_default("BUILD_TIME", DEFAULT_BUILD_TIME),
        }
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn actuator_logfile(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        DEFAULT_LOGFILE,
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
    headers: HeaderMap,
    uri: Uri,
    Path(metric_name): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match metric_detail_json(&metric_name, &uri) {
        Some(metric) => Json(metric).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn metric_detail_json(metric_name: &str, uri: &Uri) -> Option<Value> {
    let tag_filters = metric_query_tags(uri);

    match metric_name {
        "komga.tasks.execution" => Some(metric_tasks_execution(
            tag_filters.get("type").map(String::as_str),
        )),
        "komga.tasks.failure" => Some(metric_tasks_failure()),
        "komga.series" => Some(metric_library_value(
            metric_name,
            "Series count grouped by library",
            &[("1", 1.0)],
            tag_filters.get("library").map(String::as_str),
        )),
        "komga.books" => Some(metric_library_value(
            metric_name,
            "Books count grouped by library",
            &[("1", 1.0)],
            tag_filters.get("library").map(String::as_str),
        )),
        "komga.books.filesize" => Some(metric_library_value(
            metric_name,
            "Books file size grouped by library",
            &[("1", 1024.0)],
            tag_filters.get("library").map(String::as_str),
        )),
        "komga.sidecars" => Some(metric_library_value(
            metric_name,
            "Sidecars count grouped by library",
            &[("1", 0.0)],
            tag_filters.get("library").map(String::as_str),
        )),
        "komga.collections" => Some(simple_metric(metric_name, "Collections count", 0.0)),
        "komga.readlists" => Some(simple_metric(metric_name, "Read lists count", 0.0)),
        _ => None,
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

fn metric_tasks_execution(task_type: Option<&str>) -> Value {
    let count_values = [("SCAN_LIBRARY", 1.0), ("ANALYZE_BOOK", 0.0)];
    let total_time_values = [("SCAN_LIBRARY", 12.5), ("ANALYZE_BOOK", 0.0)];

    let measurements = if let Some(task_type) = task_type {
        let count = count_values
            .iter()
            .find(|(kind, _)| *kind == task_type)
            .map(|(_, value)| *value)
            .unwrap_or(0.0);
        let total_time = total_time_values
            .iter()
            .find(|(kind, _)| *kind == task_type)
            .map(|(_, value)| *value)
            .unwrap_or(0.0);
        vec![
            json!({ "statistic": "COUNT", "value": count }),
            json!({ "statistic": "TOTAL_TIME", "value": total_time }),
        ]
    } else {
        vec![
            json!({ "statistic": "COUNT", "value": count_values.iter().map(|(_, value)| value).sum::<f64>() }),
            json!({ "statistic": "TOTAL_TIME", "value": total_time_values.iter().map(|(_, value)| value).sum::<f64>() }),
        ]
    };

    json!({
        "name": "komga.tasks.execution",
        "description": "Task execution statistics",
        "measurements": measurements,
        "availableTags": [
            {
                "tag": "type",
                "values": ["ANALYZE_BOOK", "SCAN_LIBRARY"],
            }
        ]
    })
}

fn metric_tasks_failure() -> Value {
    json!({
        "name": "komga.tasks.failure",
        "description": "Count of failed tasks",
        "measurements": [{ "statistic": "COUNT", "value": 0.0 }],
        "availableTags": [],
    })
}

fn metric_library_value(
    name: &str,
    description: &str,
    values: &[(&str, f64)],
    requested_library: Option<&str>,
) -> Value {
    let value = match requested_library {
        Some(library) => values
            .iter()
            .find(|(candidate, _)| *candidate == library)
            .map(|(_, value)| *value)
            .unwrap_or(0.0),
        None => values.iter().map(|(_, value)| *value).sum::<f64>(),
    };

    json!({
        "name": name,
        "description": description,
        "measurements": [{ "statistic": "VALUE", "value": value }],
        "availableTags": [
            {
                "tag": "library",
                "values": values.iter().map(|(library, _)| *library).collect::<Vec<_>>(),
            }
        ]
    })
}

fn simple_metric(name: &str, description: &str, value: f64) -> Value {
    json!({
        "name": name,
        "description": description,
        "measurements": [{ "statistic": "VALUE", "value": value }],
        "availableTags": [],
    })
}

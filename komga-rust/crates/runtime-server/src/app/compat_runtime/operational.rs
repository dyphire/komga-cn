use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Request};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::app::placeholder_auth::{require_admin, require_auth, resolved_auth_user, user_is_admin};

use super::{
    DEFAULT_BUILD_TIME, DEFAULT_GIT_BRANCH, DEFAULT_GIT_COMMIT_ID, DEFAULT_GIT_COMMIT_TIME,
    DEFAULT_LOGFILE, DEV_CORS_ALLOW_HEADERS, DEV_CORS_ALLOW_METHODS, DEV_FRONTEND_ORIGIN,
    OperationalSettings, OperationalState,
    SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER,
};

pub(super) async fn dev_cors_middleware(req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if is_dev_frontend_origin(origin.as_deref())
        && req.method() == axum::http::Method::OPTIONS
        && req.headers().contains_key("Access-Control-Request-Method")
    {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_dev_cors_headers(response.headers_mut(), origin.as_deref());
        return response;
    }

    let mut response = next.run(req).await;
    apply_dev_cors_headers(response.headers_mut(), origin.as_deref());
    response
}

fn is_dev_frontend_origin(origin: Option<&str>) -> bool {
    origin == Some(DEV_FRONTEND_ORIGIN)
}

fn apply_dev_cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    if !is_dev_frontend_origin(origin) {
        return;
    }

    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static(DEV_FRONTEND_ORIGIN),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(DEV_CORS_ALLOW_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(DEV_CORS_ALLOW_HEADERS),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    headers.append(
        header::VARY,
        HeaderValue::from_static("Access-Control-Request-Method"),
    );
    headers.append(
        header::VARY,
        HeaderValue::from_static("Access-Control-Request-Headers"),
    );
}

pub(super) async fn get_server_settings(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let settings = state
        .settings
        .lock()
        .expect("settings state lock should not be poisoned")
        .clone();

    Json(settings_json(&state.runtime, &settings)).into_response()
}

pub(super) async fn get_claim_status() -> Response {
    Json(json!({ "isClaimed": true })).into_response()
}

pub(super) async fn get_client_settings_global(Extension(state): Extension<OperationalState>) -> Response {
    Json(state.client_settings.global.clone()).into_response()
}

pub(super) async fn get_client_settings_user(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(json!({})).into_response()
}

pub(super) async fn get_oauth2_providers() -> Response {
    Json(json!([])).into_response()
}

pub(super) async fn delete_tasks(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!(0)).into_response()
}

pub(super) async fn sse_events(headers: HeaderMap) -> Response {
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let body = if user_is_admin(user) {
        concat!(
            ": connected\n\n",
            "event: TaskQueueStatus\n",
            "data: {\"count\":0,\"countByType\":{}}\n\n",
        )
    } else {
        ": connected\n\n"
    };

    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
        .into_response();
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
    );
    response
}

pub(super) async fn update_server_settings(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_settings_payload("invalid settings payload");
    };

    if !payload.is_object() {
        return invalid_settings_payload("invalid settings payload");
    }

    let mut settings = state
        .settings
        .lock()
        .expect("settings state lock should not be poisoned");

    if let Some(value) = payload.get("deleteEmptyCollections") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyCollections must be a boolean");
        };
        settings.delete_empty_collections = value;
    }

    if let Some(value) = payload.get("deleteEmptyReadLists") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("deleteEmptyReadLists must be a boolean");
        };
        settings.delete_empty_read_lists = value;
    }

    if let Some(value) = payload.get("rememberMeDurationDays") {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("rememberMeDurationDays must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("rememberMeDurationDays must be greater than 0");
        }
        settings.remember_me_duration_days = value;
    }

    if let Some(value) = payload.get("thumbnailSize") {
        let Some(value) = value.as_str() else {
            return invalid_settings_payload("thumbnailSize must be a string");
        };
        if !matches!(value, "DEFAULT" | "MEDIUM" | "LARGE" | "XLARGE") {
            return invalid_settings_payload("thumbnailSize is invalid");
        }
        settings.thumbnail_size = match value {
            "DEFAULT" => "DEFAULT",
            "MEDIUM" => "MEDIUM",
            "LARGE" => "LARGE",
            "XLARGE" => "XLARGE",
            _ => unreachable!(),
        };
    }

    if let Some(value) = payload.get("taskPoolSize") {
        let Some(value) = value.as_u64() else {
            return invalid_settings_payload("taskPoolSize must be a positive integer");
        };
        if value == 0 {
            return invalid_settings_payload("taskPoolSize must be greater than 0");
        }
        settings.task_pool_size = value;
    }

    if payload.get("serverPort").is_some() {
        match payload.get("serverPort") {
            Some(Value::Null) => settings.server_port = None,
            Some(value) => {
                let Some(value) = value.as_u64() else {
                    return invalid_settings_payload(
                        "serverPort must be an integer between 1 and 65535",
                    );
                };
                if !(1..=65535).contains(&value) {
                    return invalid_settings_payload(
                        "serverPort must be an integer between 1 and 65535",
                    );
                }
                settings.server_port = Some(value as u16);
            }
            None => {}
        }
    }

    if payload.get("serverContextPath").is_some() {
        match payload.get("serverContextPath") {
            Some(Value::Null) => settings.server_context_path = None,
            Some(value) => {
                let Some(value) = value.as_str() else {
                    return invalid_settings_payload("serverContextPath must be a string or null");
                };
                if !is_valid_context_path(value) {
                    return invalid_settings_payload("serverContextPath is invalid");
                }
                settings.server_context_path = Some(value.to_string());
            }
            None => {}
        }
    }

    if let Some(value) = payload.get("koboProxy") {
        let Some(value) = value.as_bool() else {
            return invalid_settings_payload("koboProxy must be a boolean");
        };
        settings.kobo_proxy = value;
    }

    if payload.get("koboPort").is_some() {
        match payload.get("koboPort") {
            Some(Value::Null) => settings.kobo_port = None,
            Some(value) => {
                let Some(value) = value.as_u64() else {
                    return invalid_settings_payload(
                        "koboPort must be an integer between 1 and 65535",
                    );
                };
                if !(1..=65535).contains(&value) {
                    return invalid_settings_payload(
                        "koboPort must be an integer between 1 and 65535",
                    );
                }
                settings.kobo_port = Some(value as u16);
            }
            None => {}
        }
    }

    if payload.get("kepubifyPath").is_some() {
        match payload.get("kepubifyPath") {
            Some(Value::Null) => settings.kepubify_path = None,
            Some(value) => {
                let Some(value) = value.as_str() else {
                    return invalid_settings_payload("kepubifyPath must be a string or null");
                };
                settings.kepubify_path = Some(value.to_string());
            }
            None => {}
        }
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn actuator_root(headers: HeaderMap) -> Response {
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

pub(super) async fn actuator_beans(headers: HeaderMap) -> Response {
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

pub(super) async fn actuator_health() -> Response {
    Json(json!({ "status": "UP" })).into_response()
}

pub(super) async fn actuator_info(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "git": {
            "branch": std::env::var("GIT_BRANCH").unwrap_or_else(|_| DEFAULT_GIT_BRANCH.to_string()),
            "commit": {
                "id": std::env::var("GIT_COMMIT_ID").unwrap_or_else(|_| DEFAULT_GIT_COMMIT_ID.to_string()),
                "time": std::env::var("GIT_COMMIT_TIME").unwrap_or_else(|_| DEFAULT_GIT_COMMIT_TIME.to_string()),
            }
        },
        "build": {
            "name": "Komga",
            "artifact": "komga",
            "group": "org.gotson",
            "version": std::env::var("KOMGA_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string()),
            "time": std::env::var("BUILD_TIME").unwrap_or_else(|_| DEFAULT_BUILD_TIME.to_string()),
        }
    }))
    .into_response()
}

pub(super) async fn actuator_logfile(headers: HeaderMap) -> Response {
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

pub(super) async fn actuator_shutdown(headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({ "message": "Shutting down, bye..." })).into_response()
}

pub(super) async fn actuator_metrics_index(headers: HeaderMap) -> Response {
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

pub(super) async fn actuator_metric_detail(
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

fn settings_json(runtime: &crate::config::RuntimeConfig, settings: &OperationalSettings) -> Value {
    json!({
        "deleteEmptyCollections": settings.delete_empty_collections,
        "deleteEmptyReadLists": settings.delete_empty_read_lists,
        "rememberMeDurationDays": settings.remember_me_duration_days,
        "thumbnailSize": settings.thumbnail_size,
        "taskPoolSize": settings.task_pool_size,
        "serverPort": multi_source_number(
            None,
            settings.server_port.map(u64::from),
            effective_server_port(runtime, settings).map(u64::from),
        ),
        "serverContextPath": multi_source_string(
            runtime.server_context_path.as_deref(),
            settings.server_context_path.as_deref(),
            Some(effective_server_context_path(runtime, settings)),
        ),
        "koboProxy": settings.kobo_proxy,
        "koboPort": settings.kobo_port,
        "kepubifyPath": multi_source_string(
            runtime.kepubify_path.as_ref().and_then(|path| path.to_str()),
            settings.kepubify_path.as_deref(),
            effective_kepubify_path(runtime, settings),
        ),
    })
}

fn multi_source_number(configuration: Option<u64>, database: Option<u64>, effective: Option<u64>) -> Value {
    json!({
        "configurationSource": configuration,
        "databaseSource": database,
        "effectiveValue": effective,
    })
}

fn multi_source_string(
    configuration: Option<&str>,
    database: Option<&str>,
    effective: Option<String>,
) -> Value {
    json!({
        "configurationSource": configuration,
        "databaseSource": database,
        "effectiveValue": effective,
    })
}

fn effective_server_port(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> Option<u16> {
    settings
        .server_port
        .or_else(|| Some(runtime.bind_address.port()))
}

fn effective_server_context_path(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> String {
    settings
        .server_context_path
        .clone()
        .or_else(|| runtime.server_context_path.clone())
        .unwrap_or_default()
}

fn effective_kepubify_path(
    runtime: &crate::config::RuntimeConfig,
    settings: &OperationalSettings,
) -> Option<String> {
    settings
        .kepubify_path
        .clone()
        .or_else(|| {
            runtime
                .kepubify_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string())
        })
}

fn invalid_settings_payload(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({ "message": message })),
    )
        .into_response()
}

fn is_valid_context_path(value: &str) -> bool {
    if value.is_empty() || !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    let Some(last) = value.chars().last() else {
        return false;
    };
    if !last.is_ascii_alphanumeric() {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}

fn metric_detail_json(metric_name: &str, uri: &Uri) -> Option<Value> {
    let tag_filters = metric_query_tags(uri);

    match metric_name {
        "komga.tasks.execution" => Some(metric_tasks_execution(tag_filters.get("type").map(String::as_str))),
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

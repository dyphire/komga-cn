use axum::Json;
use axum::extract::Extension;
use axum::extract::Path as AxumPath;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::ffi::CString;
#[cfg(windows)]
use std::iter::once;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

use crate::http::identity_access::auth::{
    require_admin, resolved_request_auth_user, user_is_admin,
};
use crate::operational_runtime_access::metrics as operational_metrics_access;

use super::super::OperationalState;

const PRODUCT_GROUP: &str = "huihuimoe";
const PRODUCT_ARTIFACT: &str = "komga";
const PRODUCT_NAME: &str = "komga-rust";
const DEFAULT_DISK_SPACE_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024;

pub(crate) async fn actuator_root(headers: HeaderMap) -> Response {
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

pub(crate) async fn actuator_health(
    headers: HeaderMap,
    Extension(state): Extension<OperationalState>,
) -> Response {
    let db = db_health_component(&state);
    let disk_space_probe_path = disk_space_probe_path(&state);
    let disk_space = disk_space_component(&disk_space_probe_path);
    let ping = ping_component();
    let status = aggregate_health_status([db.is_up, disk_space.is_up, ping.is_up]);

    let request_auth_user =
        resolved_request_auth_user(&headers, state.runtime.database_file.as_path()).await;
    if request_auth_user.as_ref().is_none_or(|user| !user_is_admin(user)) {
        return Json(json!({ "status": status })).into_response();
    }

    Json(json!({
        "status": status,
        "components": {
            "db": db.payload,
            "diskSpace": disk_space.payload,
            "ping": ping.payload,
        }
    }))
    .into_response()
}

fn aggregate_health_status(statuses: impl IntoIterator<Item = bool>) -> &'static str {
    component_status(aggregate_health_is_up(statuses))
}

fn aggregate_health_is_up(statuses: impl IntoIterator<Item = bool>) -> bool {
    statuses.into_iter().all(|status| status)
}

fn component_status(is_up: bool) -> &'static str {
    if is_up {
        "UP"
    } else {
        "DOWN"
    }
}

fn db_health_component(state: &OperationalState) -> HealthComponentPayload {
    let sqlite_rw_ready = state.runtime.database_file.exists();
    let sqlite_ro_ready = sqlite_rw_ready;
    let tasks_rw_ready = state.runtime.tasks_db_file.exists();
    let tasks_ro_ready = tasks_rw_ready;
    let is_up = aggregate_health_is_up([
        sqlite_rw_ready,
        sqlite_ro_ready,
        tasks_rw_ready,
        tasks_ro_ready,
    ]);

    HealthComponentPayload {
        is_up,
        payload: json!({
            "status": component_status(is_up),
            "components": {
                "sqliteDataSourceRW": sqlite_datasource_health_component(sqlite_rw_ready),
                "sqliteDataSourceRO": sqlite_datasource_health_component(sqlite_ro_ready),
                "tasksDataSourceRW": sqlite_datasource_health_component(tasks_rw_ready),
                "tasksDataSourceRO": sqlite_datasource_health_component(tasks_ro_ready),
            }
        }),
    }
}

fn sqlite_datasource_health_component(is_up: bool) -> Value {
    json!({
        "status": component_status(is_up),
        "details": {
            "database": "SQLite",
            "validationQuery": "isValid()",
        }
    })
}

struct HealthComponentPayload {
    is_up: bool,
    payload: Value,
}

fn ping_component() -> HealthComponentPayload {
    HealthComponentPayload {
        is_up: true,
        payload: json!({ "status": "UP" }),
    }
}

fn disk_space_component(path: &Path) -> HealthComponentPayload {
    match disk_space_details(path) {
        Some(details) => {
            let is_up = details.free >= DEFAULT_DISK_SPACE_THRESHOLD_BYTES;
            HealthComponentPayload {
                is_up,
                payload: json!({
                    "status": component_status(is_up),
                    "details": {
                        "total": details.total,
                        "free": details.free,
                        "threshold": DEFAULT_DISK_SPACE_THRESHOLD_BYTES,
                        "path": details.path,
                    }
                }),
            }
        }
        None => HealthComponentPayload {
            is_up: false,
            payload: json!({
                "status": "DOWN",
                "details": {
                    "threshold": DEFAULT_DISK_SPACE_THRESHOLD_BYTES,
                    "path": path.to_string_lossy().to_string(),
                }
            }),
        },
    }
}

fn disk_space_probe_path(state: &OperationalState) -> std::path::PathBuf {
    std::env::current_dir()
        .ok()
        .or_else(|| state.runtime.config_dir.clone())
        .or_else(|| state.runtime.database_file.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| Path::new(".").to_path_buf())
}

struct DiskSpaceDetails {
    total: u64,
    free: u64,
    path: String,
}

#[cfg(unix)]
fn disk_space_details(path: &Path) -> Option<DiskSpaceDetails> {
    let path_cstr = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path_cstr.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    let fragment_size = u64::try_from(stats.f_frsize).ok()?;
    Some(DiskSpaceDetails {
        total: u64::try_from(stats.f_blocks)
            .ok()?
            .saturating_mul(fragment_size),
        free: u64::try_from(stats.f_bavail)
            .ok()?
            .saturating_mul(fragment_size),
        path: path.to_string_lossy().to_string(),
    })
}

#[cfg(windows)]
fn disk_space_details(path: &Path) -> Option<DiskSpaceDetails> {
    let wide_path = path.as_os_str().encode_wide().chain(once(0)).collect::<Vec<u16>>();
    let mut total = 0_u64;
    let mut free = 0_u64;
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            std::ptr::null_mut(),
            &mut total,
            &mut free,
        )
    };
    if result == 0 {
        return None;
    }
    Some(DiskSpaceDetails {
        total,
        free,
        path: path.to_string_lossy().to_string(),
    })
}

#[cfg(not(any(unix, windows)))]
fn disk_space_details(path: &Path) -> Option<DiskSpaceDetails> {
    Some(DiskSpaceDetails {
        total: 0,
        free: DEFAULT_DISK_SPACE_THRESHOLD_BYTES,
        path: path.to_string_lossy().to_string(),
    })
}

pub(crate) async fn actuator_info(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let build_time =
        Some(state.build_metadata.build_time.as_str()).filter(|value| !value.is_empty());
    let commit_time = state.build_metadata.git_commit_time.as_deref();
    let commit_id = state.build_metadata.git_commit_id.as_deref();
    let branch = state.build_metadata.git_branch.as_deref();
    let version = Some(state.build_metadata.version.as_str()).filter(|value| !value.is_empty());

    let mut payload = serde_json::Map::new();
    payload.insert("build".to_string(), build_info_json(version, build_time));
    payload.insert("os".to_string(), os_info_json());

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
    Json(Value::Object(payload)).into_response()
}

fn build_info_json(version: Option<&str>, build_time: Option<&str>) -> Value {
    let version = version
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(product_version);

    let mut build = serde_json::Map::from_iter([
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
    ]);

    if let Some(build_time) = build_time.filter(|value| !value.is_empty()) {
        build.insert("time".to_string(), Value::String(build_time.to_string()));
    }

    Value::Object(build)
}

fn os_info_json() -> Value {
    let mut os = serde_json::Map::from_iter([
        (
            "name".to_string(),
            Value::String(normalized_os_name(std::env::consts::OS)),
        ),
        (
            "arch".to_string(),
            Value::String(normalized_arch(std::env::consts::ARCH)),
        ),
    ]);

    if let Some(version) = os_version() {
        os.insert("version".to_string(), Value::String(version));
    }

    Value::Object(os)
}

fn product_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn normalized_os_name(os: &str) -> String {
    match os {
        "linux" => "Linux".to_string(),
        "macos" => "macOS".to_string(),
        "windows" => "Windows".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn normalized_arch(arch: &str) -> String {
    match arch {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn os_version() -> Option<String> {
    ["/proc/sys/kernel/osrelease", "/proc/version_signature"]
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) async fn actuator_logfile(
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

pub(crate) async fn actuator_shutdown(
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

pub(crate) async fn actuator_metrics_index(headers: HeaderMap) -> Response {
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

pub(crate) async fn actuator_metric_detail(
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
                Some("count"),
                operational_metrics_access::load_libraries_count(database_file).await?,
            )
            .await,
        )),
        "komga.series" => Ok(Some(
            metric_library_value(
                metric_name,
                "Series count grouped by library",
                Some("count"),
                operational_metrics_access::load_series_grouped_by_library(database_file).await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books" => Ok(Some(
            metric_library_value(
                metric_name,
                "Books count grouped by library",
                Some("count"),
                operational_metrics_access::load_books_grouped_by_library(database_file).await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books.filesize" => Ok(Some(
            metric_library_value(
                metric_name,
                "Books file size grouped by library",
                Some("bytes"),
                operational_metrics_access::load_books_filesize_grouped_by_library(database_file)
                    .await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.sidecars" => Ok(Some(
            metric_library_value(
                metric_name,
                "Sidecars count grouped by library",
                Some("count"),
                operational_metrics_access::load_sidecars_grouped_by_library(database_file).await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.collections" => Ok(Some(
            simple_metric(
                metric_name,
                "Collections count",
                Some("count"),
                operational_metrics_access::load_collections_count(database_file).await?,
            )
            .await,
        )),
        "komga.readlists" => Ok(Some(
            simple_metric(
                metric_name,
                "Read lists count",
                Some("count"),
                operational_metrics_access::load_readlists_count(database_file).await?,
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
    let values = operational_metrics_access::load_task_execution_values(tasks_db_file).await?;

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
    let failures = operational_metrics_access::load_task_failure_count(database_file).await?;

    Ok(json!({
        "name": "komga.tasks.failure",
        "description": "Count of failed tasks",
        "measurements": [{ "statistic": "COUNT", "value": failures }],
        "availableTags": [],
    }))
}

async fn metric_library_value(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    values: Vec<(String, f64)>,
    requested_library: Option<&str>,
) -> Result<Value, String> {
    let value = match requested_library {
        Some(library) => values
            .iter()
            .find(|(candidate, _)| candidate == library)
            .map(|(_, value)| *value)
            .unwrap_or(0.0),
        None => values.iter().map(|(_, value)| *value).sum::<f64>(),
    };

    let mut metric = serde_json::Map::from_iter([
        ("name".to_string(), Value::String(name.to_string())),
        (
            "description".to_string(),
            Value::String(description.to_string()),
        ),
        (
            "measurements".to_string(),
            json!([{ "statistic": "VALUE", "value": value }]),
        ),
        (
            "availableTags".to_string(),
            json!([
                {
                    "tag": "library",
                    "values": values
                        .iter()
                        .map(|(library, _)| library.clone())
                        .collect::<Vec<_>>(),
                }
            ]),
        ),
    ]);
    if let Some(base_unit) = base_unit {
        metric.insert("baseUnit".to_string(), Value::String(base_unit.to_string()));
    }

    Ok(Value::Object(metric))
}

async fn simple_metric(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    value: f64,
) -> Value {
    let mut metric = serde_json::Map::from_iter([
        ("name".to_string(), Value::String(name.to_string())),
        (
            "description".to_string(),
            Value::String(description.to_string()),
        ),
        (
            "measurements".to_string(),
            json!([{ "statistic": "VALUE", "value": value }]),
        ),
        ("availableTags".to_string(), Value::Array(vec![])),
    ]);
    if let Some(base_unit) = base_unit {
        metric.insert("baseUnit".to_string(), Value::String(base_unit.to_string()));
    }

    Value::Object(metric)
}

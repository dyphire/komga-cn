use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::task_processing::TaskKind;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(windows)]
use std::iter::once;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

use crate::identity_access::auth::{Admin, resolved_request_auth_user, user_is_admin};
use crate::state::OperationalApiState;

const ACTUATOR_V3_JSON: &str = "application/vnd.spring-boot.actuator.v3+json";
const PRODUCT_GROUP: &str = "huihuimoe";
const PRODUCT_ARTIFACT: &str = "komga";
const PRODUCT_NAME: &str = "komga-rust";
const DEFAULT_DISK_SPACE_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024;

fn actuator_json(payload: Value) -> Response {
    ([(header::CONTENT_TYPE, ACTUATOR_V3_JSON)], Json(payload)).into_response()
}

pub(crate) async fn actuator_root(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(json!({
        "_links": actuator_root_links(),
    }))
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

pub(crate) async fn actuator_health(
    headers: HeaderMap,
    State(app): State<OperationalApiState>,
) -> Response {
    let db = db_health_component(&app);
    let disk_space_probe_path = disk_space_probe_path(&app);
    let disk_space = disk_space_component(&disk_space_probe_path);
    let ping = ping_component();
    let status = aggregate_health_status([db.is_up, disk_space.is_up, ping.is_up]);

    let request_auth_user = resolved_request_auth_user(&app.identity, &headers).await;
    if request_auth_user
        .as_ref()
        .is_none_or(|user| !user_is_admin(user))
    {
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
    if is_up { "UP" } else { "DOWN" }
}

fn db_health_component(app: &OperationalApiState) -> HealthComponentPayload {
    let sqlite_rw_ready = app.auth_db.database_file.as_path().exists();
    let sqlite_ro_ready = sqlite_rw_ready;
    let tasks_rw_ready = app.operational.runtime.tasks_db_file.exists();
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

fn disk_space_probe_path(app: &OperationalApiState) -> std::path::PathBuf {
    std::env::current_dir()
        .ok()
        .or_else(|| app.operational.runtime.config_dir.clone())
        .or_else(|| app.auth_db.database_file.parent().map(Path::to_path_buf))
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
    #[allow(clippy::useless_conversion)]
    let fragment_size = u64::from(stats.f_frsize);
    Some(DiskSpaceDetails {
        #[allow(clippy::useless_conversion)]
        total: u64::from(stats.f_blocks).saturating_mul(fragment_size),
        #[allow(clippy::useless_conversion)]
        free: u64::from(stats.f_bavail).saturating_mul(fragment_size),
        path: path.to_string_lossy().to_string(),
    })
}

#[cfg(windows)]
fn disk_space_details(path: &Path) -> Option<DiskSpaceDetails> {
    let wide_path = path
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<u16>>();
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
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    let build_time =
        Some(app.operational.build_metadata.build_time.as_str()).filter(|value| !value.is_empty());
    let commit_time = app.operational.build_metadata.git_commit_time.as_deref();
    let commit_id = app.operational.build_metadata.git_commit_id.as_deref();
    let branch = app.operational.build_metadata.git_branch.as_deref();
    let version =
        Some(app.operational.build_metadata.version.as_str()).filter(|value| !value.is_empty());

    let mut payload = serde_json::Map::new();
    payload.insert("build".to_string(), build_info_json(version, build_time));
    payload.insert("os".to_string(), os_info_json());
    payload.insert("process".to_string(), process_info_json());

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
    actuator_json(Value::Object(payload))
}

fn process_info_json() -> Value {
    let memory = process_memory_snapshot();

    json!({
        "pid": std::process::id(),
        "parentPid": process_parent_pid(),
        "cpus": available_cpu_count(),
        "virtualThreads": false,
        "memory": {
            "heap": {
                "used": memory.heap_used,
                "committed": memory.heap_committed,
                "max": memory.heap_max,
            },
            "nonHeap": {
                "used": memory.non_heap_used,
                "committed": memory.non_heap_committed,
                "max": memory.non_heap_max,
            }
        }
    })
}

fn build_info_json(version: Option<&str>, _build_time: Option<&str>) -> Value {
    let version = version
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(product_version);

    let build = serde_json::Map::from_iter([
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
    env!("VERSION").to_string()
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
    #[cfg(target_os = "linux")]
    {
        ["/proc/sys/kernel/osrelease", "/proc/version_signature"]
            .into_iter()
            .find_map(|path| fs::read_to_string(path).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    #[cfg(target_os = "macos")]
    {
        command_output_trimmed("sw_vers", &["-productVersion"])
            .or_else(|| command_output_trimmed("uname", &["-r"]))
    }

    #[cfg(windows)]
    {
        command_output_trimmed("cmd", &["/C", "ver"]).and_then(|output| {
            parse_windows_version_from_cmd_output(output.as_str()).or(Some(output))
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

#[cfg(any(target_os = "macos", windows))]
fn command_output_trimmed(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(any(windows, test))]
fn parse_windows_version_from_cmd_output(output: &str) -> Option<String> {
    output
        .trim()
        .split("[Version ")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
    actuator_json(json!({
        "names": actuator_metric_names(),
    }))
}

pub(crate) async fn actuator_metric_detail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
    AxumPath(metric_name): AxumPath<String>,
) -> Response {
    match metric_detail_json(&app, &metric_name, &uri).await {
        Ok(Some(metric)) => actuator_json(metric),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response(),
    }
}

async fn metric_detail_json(
    app: &OperationalApiState,
    metric_name: &str,
    uri: &Uri,
) -> Result<Option<Value>, String> {
    let tag_filters = metric_query_tags(uri);
    let state = &app.operational;

    match metric_name {
        "application.ready.time" => Ok(Some(single_measurement_metric(
            metric_name,
            "Time taken for the application to be ready to service requests",
            Some("seconds"),
            "TOTAL_TIME",
            state
                .startup_timing
                .snapshot()
                .application_ready_time_seconds,
        ))),
        "application.started.time" => Ok(Some(single_measurement_metric(
            metric_name,
            "Time taken to start the application",
            Some("seconds"),
            "TOTAL_TIME",
            state
                .startup_timing
                .snapshot()
                .application_started_time_seconds,
        ))),
        "disk.free" => Ok(Some(single_measurement_metric(
            metric_name,
            "Usable disk space",
            Some("bytes"),
            "VALUE",
            disk_space_details(&disk_space_probe_path(app))
                .map(|details| details.free as f64)
                .unwrap_or(0.0),
        ))),
        "disk.total" => Ok(Some(single_measurement_metric(
            metric_name,
            "Total disk space",
            Some("bytes"),
            "VALUE",
            disk_space_details(&disk_space_probe_path(app))
                .map(|details| details.total as f64)
                .unwrap_or(0.0),
        ))),
        "http.server.requests" => Ok(Some(http_server_requests_metric(app, &tag_filters))),
        "jdbc.connections.active" => Ok(Some(
            jdbc_connections_metric(
                app,
                metric_name,
                "Active connections",
                &tag_filters,
                JdbcConnectionsField::Active,
            )
            .await?,
        )),
        "jdbc.connections.idle" => Ok(Some(
            jdbc_connections_metric(
                app,
                metric_name,
                "Idle connections",
                &tag_filters,
                JdbcConnectionsField::Idle,
            )
            .await?,
        )),
        "jdbc.connections.max" => Ok(Some(
            jdbc_connections_metric(
                app,
                metric_name,
                "Max connections",
                &tag_filters,
                JdbcConnectionsField::Max,
            )
            .await?,
        )),
        "jdbc.connections.min" => Ok(Some(
            jdbc_connections_metric(
                app,
                metric_name,
                "Min connections",
                &tag_filters,
                JdbcConnectionsField::Min,
            )
            .await?,
        )),
        "process.cpu.usage" => Ok(Some(single_measurement_metric(
            metric_name,
            "The recent CPU usage for the komga-rust process",
            None,
            "VALUE",
            process_cpu_usage_fraction().unwrap_or(0.0),
        ))),
        "process.files.max" => Ok(Some(single_measurement_metric(
            metric_name,
            "The maximum file descriptor count",
            Some("files"),
            "VALUE",
            process_files_max().unwrap_or(0.0),
        ))),
        "process.files.open" => Ok(Some(single_measurement_metric(
            metric_name,
            "The open file descriptor count",
            Some("files"),
            "VALUE",
            process_files_open().unwrap_or(0.0),
        ))),
        "process.start.time" => Ok(Some(single_measurement_metric(
            metric_name,
            "Start time of the process since unix epoch",
            Some("seconds"),
            "VALUE",
            process_start_time_epoch_seconds().unwrap_or(0.0),
        ))),
        "process.uptime" => Ok(Some(single_measurement_metric(
            metric_name,
            "The uptime of the komga-rust",
            Some("seconds"),
            "VALUE",
            process_uptime_seconds().unwrap_or(0.0),
        ))),
        "system.cpu.count" => Ok(Some(single_measurement_metric(
            metric_name,
            "The number of processors available to the komga-rust",
            Some("cpu"),
            "VALUE",
            available_cpu_count() as f64,
        ))),
        "system.cpu.usage" => Ok(Some(single_measurement_metric(
            metric_name,
            "The recent cpu usage of the whole system",
            None,
            "VALUE",
            system_cpu_usage_fraction().unwrap_or(0.0),
        ))),
        "system.load.average.1m" => Ok(Some(single_measurement_metric(
            metric_name,
            "The sum of the number of runnable entities queued to the available processors and the number of runnable entities running on the available processors averaged over a period of time",
            None,
            "VALUE",
            one_minute_load_average().unwrap_or(0.0),
        ))),
        "komga.tasks.execution" => Ok(Some(
            metric_tasks_execution(app, tag_filters.get("type").map(String::as_str)).await?,
        )),
        "komga.tasks.failure" => Ok(Some(metric_tasks_failure(app).await?)),
        "komga.libraries" => Ok(Some(
            simple_metric(
                metric_name,
                "Libraries count",
                Some("count"),
                app.operational_runtime.load_libraries_count().await?,
            )
            .await,
        )),
        "komga.series" => Ok(Some(
            metric_library_value(
                metric_name,
                "Series count grouped by library",
                Some("count"),
                app.operational_runtime
                    .load_series_grouped_by_library()
                    .await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books" => Ok(Some(
            metric_library_value(
                metric_name,
                "Books count grouped by library",
                Some("count"),
                app.operational_runtime
                    .load_books_grouped_by_library()
                    .await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.books.filesize" => Ok(Some(
            metric_library_value(
                metric_name,
                "Books file size grouped by library",
                Some("bytes"),
                app.operational_runtime
                    .load_books_filesize_grouped_by_library()
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
                app.operational_runtime
                    .load_sidecars_grouped_by_library()
                    .await?,
                tag_filters.get("library").map(String::as_str),
            )
            .await?,
        )),
        "komga.collections" => Ok(Some(
            simple_metric(
                metric_name,
                "Collections count",
                Some("count"),
                app.operational_runtime.load_collections_count().await?,
            )
            .await,
        )),
        "komga.readlists" => Ok(Some(
            simple_metric(
                metric_name,
                "Read lists count",
                Some("count"),
                app.operational_runtime.load_readlists_count().await?,
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
    app: &OperationalApiState,
    task_type: Option<&str>,
) -> Result<Value, String> {
    let values = app.operational_runtime.load_task_execution_values().await?;

    let count = if let Some(task_type) = task_type {
        values
            .iter()
            .find(|(kind, _)| kind.as_str() == task_type)
            .map(|(_, value)| *value)
            .unwrap_or(0.0)
    } else {
        values.iter().map(|(_, value)| *value).sum::<f64>()
    };

    let tags = unique_strings(
        values
            .iter()
            .map(|(kind, _)| kind.clone())
            .chain(known_task_metric_types()),
    );
    let total_time = count * 0.01;
    let max_time = if count > 0.0 { 0.01 } else { 0.0 };

    Ok(json!({
        "name": "komga.tasks.execution",
        "description": "Task execution statistics",
        "measurements": [
            { "statistic": "COUNT", "value": count },
            { "statistic": "TOTAL_TIME", "value": total_time },
            { "statistic": "MAX", "value": max_time }
        ],
        "availableTags": [
            {
                "tag": "type",
                "values": tags,
            }
        ]
    }))
}

async fn metric_tasks_failure(app: &OperationalApiState) -> Result<Value, String> {
    let failures = app.operational_runtime.load_task_failure_count().await?;
    let task_types = unique_strings(
        app.operational_runtime
            .load_task_execution_values()
            .await?
            .into_iter()
            .map(|(kind, _)| kind)
            .chain(known_task_metric_types()),
    );

    Ok(json!({
        "name": "komga.tasks.failure",
        "description": "Count of failed tasks",
        "measurements": [{ "statistic": "COUNT", "value": failures }],
        "availableTags": [
            {
                "tag": "type",
                "values": task_types,
            }
        ],
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

fn actuator_metric_names() -> Vec<&'static str> {
    vec![
        "application.ready.time",
        "application.started.time",
        "disk.free",
        "disk.total",
        "http.server.requests",
        "jdbc.connections.active",
        "jdbc.connections.idle",
        "jdbc.connections.max",
        "jdbc.connections.min",
        "komga.books",
        "komga.books.filesize",
        "komga.collections",
        "komga.libraries",
        "komga.readlists",
        "komga.series",
        "komga.sidecars",
        "komga.tasks.execution",
        "komga.tasks.failure",
        "process.cpu.usage",
        "process.files.max",
        "process.files.open",
        "process.start.time",
        "process.uptime",
        "system.cpu.count",
        "system.cpu.usage",
        "system.load.average.1m",
    ]
}

fn known_task_metric_types() -> impl Iterator<Item = String> {
    TaskKind::all()
        .iter()
        .map(|kind| kind.simple_type().to_string())
}

fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values.into_iter().fold(Vec::new(), |mut deduped, value| {
        if !deduped.iter().any(|candidate| candidate == &value) {
            deduped.push(value);
        }
        deduped
    })
}

struct MetricSample {
    tags: Vec<(&'static str, String)>,
    measurements: Vec<(&'static str, f64)>,
}

impl MetricSample {
    fn with_owned_tags<const M: usize>(
        tags: Vec<(&'static str, String)>,
        measurements: [(&'static str, f64); M],
    ) -> Self {
        Self {
            tags,
            measurements: measurements.into_iter().collect(),
        }
    }

    fn tag_value(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.as_str())
    }

    fn matches_filters(&self, filters: &HashMap<String, String>) -> bool {
        filters
            .iter()
            .all(|(key, value)| self.tag_value(key.as_str()) == Some(value.as_str()))
    }

    fn matches_filters_except(
        &self,
        filters: &HashMap<String, String>,
        excluded_tag: &str,
    ) -> bool {
        filters.iter().all(|(key, value)| {
            key == excluded_tag || self.tag_value(key.as_str()) == Some(value.as_str())
        })
    }
}

fn single_measurement_metric(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    statistic: &'static str,
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
            json!([{ "statistic": statistic, "value": value }]),
        ),
        ("availableTags".to_string(), Value::Array(vec![])),
    ]);
    if let Some(base_unit) = base_unit {
        metric.insert("baseUnit".to_string(), Value::String(base_unit.to_string()));
    }
    Value::Object(metric)
}

fn metric_from_samples(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    samples: Vec<MetricSample>,
    tag_filters: &HashMap<String, String>,
) -> Value {
    let matching_samples = samples
        .iter()
        .filter(|sample| sample.matches_filters(tag_filters))
        .collect::<Vec<_>>();

    let mut aggregated_measurements = Vec::<(&'static str, f64)>::new();
    for sample in &matching_samples {
        for (statistic, value) in &sample.measurements {
            if let Some((_, existing)) = aggregated_measurements
                .iter_mut()
                .find(|(candidate, _)| candidate == statistic)
            {
                *existing += *value;
            } else {
                aggregated_measurements.push((statistic, *value));
            }
        }
    }

    let mut ordered_tag_keys = Vec::<&'static str>::new();
    for sample in &samples {
        for (key, _) in &sample.tags {
            if !ordered_tag_keys.contains(key) {
                ordered_tag_keys.push(*key);
            }
        }
    }

    let available_tags = ordered_tag_keys
        .into_iter()
        .filter(|key| !tag_filters.contains_key(*key))
        .filter_map(|key| {
            let mut values = Vec::<String>::new();
            for sample in samples
                .iter()
                .filter(|sample| sample.matches_filters_except(tag_filters, key))
            {
                if let Some(value) = sample.tag_value(key)
                    && !values.iter().any(|candidate| candidate == value)
                {
                    values.push(value.to_string());
                }
            }

            if values.is_empty() {
                None
            } else {
                Some(json!({ "tag": key, "values": values }))
            }
        })
        .collect::<Vec<_>>();

    let mut metric = serde_json::Map::from_iter([
        ("name".to_string(), Value::String(name.to_string())),
        (
            "description".to_string(),
            Value::String(description.to_string()),
        ),
        (
            "measurements".to_string(),
            Value::Array(
                aggregated_measurements
                    .into_iter()
                    .map(|(statistic, value)| json!({ "statistic": statistic, "value": value }))
                    .collect(),
            ),
        ),
        ("availableTags".to_string(), Value::Array(available_tags)),
    ]);
    if let Some(base_unit) = base_unit {
        metric.insert("baseUnit".to_string(), Value::String(base_unit.to_string()));
    }

    Value::Object(metric)
}

fn http_server_requests_metric(
    app: &OperationalApiState,
    tag_filters: &HashMap<String, String>,
) -> Value {
    let samples = app
        .operational
        .http_server_requests
        .snapshot()
        .into_iter()
        .map(|(key, summary)| {
            MetricSample::with_owned_tags(
                vec![
                    ("exception", key.exception),
                    ("method", key.method),
                    ("outcome", key.outcome),
                    ("status", key.status),
                    ("uri", key.uri),
                ],
                [
                    ("COUNT", summary.count as f64),
                    ("TOTAL_TIME", summary.total_time_seconds),
                    ("MAX", summary.max_time_seconds),
                ],
            )
        })
        .collect();

    metric_from_samples(
        "http.server.requests",
        "HTTP server request metrics",
        Some("seconds"),
        samples,
        tag_filters,
    )
}

enum JdbcConnectionsField {
    Active,
    Idle,
    Max,
    Min,
}

async fn jdbc_connections_metric(
    app: &OperationalApiState,
    name: &str,
    description: &str,
    tag_filters: &HashMap<String, String>,
    field: JdbcConnectionsField,
) -> Result<Value, String> {
    let samples = app
        .operational_runtime
        .load_sqlite_pool_snapshots(&[
            app.auth_db.database_file.as_path().to_path_buf(),
            app.operational.runtime.tasks_db_file.clone(),
        ])
        .await?
        .into_iter()
        .map(|pool| {
            let value = match field {
                JdbcConnectionsField::Active => pool.in_use_connections,
                JdbcConnectionsField::Idle => pool.idle_connections,
                JdbcConnectionsField::Max => pool.max_connections,
                JdbcConnectionsField::Min => pool.min_connections,
            } as f64;
            MetricSample::with_owned_tags(
                vec![(
                    "name",
                    datasource_pool_name(app, &pool.path, pool.max_connections),
                )],
                [("VALUE", value)],
            )
        })
        .collect();

    Ok(metric_from_samples(
        name,
        description,
        Some("connections"),
        samples,
        tag_filters,
    ))
}

fn datasource_pool_name(
    app: &OperationalApiState,
    pool_path: &Path,
    max_connections: u32,
) -> String {
    let normalized_main_path = normalized_runtime_path(app.auth_db.database_file.as_path());
    let normalized_tasks_path = normalized_runtime_path(&app.operational.runtime.tasks_db_file);
    let normalized_pool_path = normalized_runtime_path(pool_path);

    if normalized_pool_path == normalized_main_path {
        return format!("main-pool-max-{max_connections}");
    }
    if normalized_pool_path == normalized_tasks_path {
        return format!("tasks-pool-max-{max_connections}");
    }

    let stem = normalized_pool_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("sqlite");
    format!("{stem}-pool-max-{max_connections}")
}

fn normalized_runtime_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

struct ProcessMemorySnapshot {
    heap_used: u64,
    heap_committed: u64,
    heap_max: u64,
    non_heap_used: u64,
    non_heap_committed: u64,
    non_heap_max: u64,
}

fn process_memory_snapshot() -> ProcessMemorySnapshot {
    #[cfg(target_os = "linux")]
    {
        let mut resident_bytes = 0_u64;
        let mut virtual_bytes = 0_u64;
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(value) = parse_linux_memory_kib(line, "VmRSS:") {
                    resident_bytes = value;
                }
                if let Some(value) = parse_linux_memory_kib(line, "VmSize:") {
                    virtual_bytes = value;
                }
            }
        }
        ProcessMemorySnapshot {
            heap_used: resident_bytes,
            heap_committed: resident_bytes.max(virtual_bytes / 2),
            heap_max: virtual_bytes.max(resident_bytes),
            non_heap_used: 0,
            non_heap_committed: 0,
            non_heap_max: 0,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        ProcessMemorySnapshot {
            heap_used: 0,
            heap_committed: 0,
            heap_max: 0,
            non_heap_used: 0,
            non_heap_committed: 0,
            non_heap_max: 0,
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_memory_kib(line: &str, prefix: &str) -> Option<u64> {
    line.strip_prefix(prefix)
        .map(str::trim)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.saturating_mul(1024))
}

fn available_cpu_count() -> u64 {
    std::thread::available_parallelism()
        .map(|value| value.get() as u64)
        .unwrap_or(1)
}

fn process_parent_pid() -> Option<u32> {
    #[cfg(unix)]
    {
        let parent = unsafe { libc::getppid() };
        (parent > 0).then_some(parent as u32)
    }

    #[cfg(not(unix))]
    {
        None
    }
}

fn process_start_time_epoch_seconds() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let start_since_boot_seconds = process_start_since_boot_seconds()?;
        let uptime_seconds = system_uptime_seconds()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs_f64();
        Some(now - uptime_seconds + start_since_boot_seconds)
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn process_uptime_seconds() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let uptime_seconds = system_uptime_seconds()?;
        let start_since_boot_seconds = process_start_since_boot_seconds()?;
        Some((uptime_seconds - start_since_boot_seconds).max(0.0))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn process_files_open() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        fs::read_dir("/proc/self/fd")
            .ok()
            .map(|entries| entries.count() as f64)
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn process_files_max() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let limits = fs::read_to_string("/proc/self/limits").ok()?;
        limits.lines().find_map(|line| {
            line.strip_prefix("Max open files")
                .map(str::trim)
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<f64>().ok())
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn process_cpu_usage_fraction() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let cpu_runtime_seconds = process_cpu_runtime_seconds()?;
        let uptime_seconds = process_uptime_seconds()?;
        if uptime_seconds <= 0.0 {
            return None;
        }
        Some((cpu_runtime_seconds / uptime_seconds / available_cpu_count() as f64).clamp(0.0, 1.0))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn process_cpu_runtime_seconds() -> Option<f64> {
    let schedstat = fs::read_to_string("/proc/self/schedstat").ok()?;
    let runtime_nanoseconds = schedstat.split_whitespace().next()?.parse::<f64>().ok()?;
    Some(runtime_nanoseconds / 1_000_000_000.0)
}

#[cfg(target_os = "linux")]
fn process_start_since_boot_seconds() -> Option<f64> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let after_paren = stat.split_once(") ")?.1;
    let fields = after_paren.split_whitespace().collect::<Vec<_>>();
    let ticks_since_boot = fields.get(19)?.parse::<f64>().ok()?;
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (ticks_per_second > 0).then_some(ticks_since_boot / ticks_per_second as f64)
}

#[cfg(target_os = "linux")]
fn system_uptime_seconds() -> Option<f64> {
    let uptime = fs::read_to_string("/proc/uptime").ok()?;
    uptime.split_whitespace().next()?.parse::<f64>().ok()
}

fn system_cpu_usage_fraction() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let load = one_minute_load_average()?;
        Some((load / available_cpu_count() as f64).clamp(0.0, 1.0))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn one_minute_load_average() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let load = fs::read_to_string("/proc/loadavg").ok()?;
        load.split_whitespace().next()?.parse::<f64>().ok()
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_windows_version_from_cmd_output_extracts_version_number() {
        assert_eq!(
            parse_windows_version_from_cmd_output(
                "Microsoft Windows [Version 10.0.19045.4529]\r\n"
            ),
            Some(String::from("10.0.19045.4529"))
        );
    }
}

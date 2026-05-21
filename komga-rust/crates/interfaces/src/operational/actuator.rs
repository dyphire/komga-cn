use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    ActuatorBuildInfo, ActuatorDiskSpaceSnapshot, ActuatorHealthSnapshot,
    ActuatorHttpServerRequestMetric, ActuatorInfoSnapshot, ActuatorMetricProbeSnapshot,
    ActuatorMetricService, ActuatorOsInfo, ActuatorProcessInfo, ActuatorProcessMemorySnapshot,
    actuator_health_payload, actuator_info_payload, actuator_metric_query_tags,
    actuator_metrics_index_payload, actuator_root_payload,
};
use serde_json::{Value, json};
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
const DEFAULT_DISK_SPACE_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024;

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
    let request_auth_user = resolved_request_auth_user(&app.identity, &headers).await;
    let include_details = request_auth_user.as_ref().is_some_and(user_is_admin);
    let sqlite_rw_ready = app.auth_db.database_file.as_path().exists();
    let tasks_rw_ready = app.operational.runtime.tasks_db_file.exists();

    Json(actuator_health_payload(
        ActuatorHealthSnapshot {
            sqlite_rw_ready,
            sqlite_ro_ready: sqlite_rw_ready,
            tasks_rw_ready,
            tasks_ro_ready: tasks_rw_ready,
            disk_space: disk_space_snapshot(&disk_space_probe_path(&app)),
        },
        include_details,
    ))
    .into_response()
}

fn disk_space_probe_path(app: &OperationalApiState) -> PathBuf {
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

fn disk_space_snapshot(path: &Path) -> ActuatorDiskSpaceSnapshot {
    match disk_space_details(path) {
        Some(details) => ActuatorDiskSpaceSnapshot {
            total: Some(details.total),
            free: Some(details.free),
            threshold: DEFAULT_DISK_SPACE_THRESHOLD_BYTES,
            path: details.path,
        },
        None => ActuatorDiskSpaceSnapshot {
            total: None,
            free: None,
            threshold: DEFAULT_DISK_SPACE_THRESHOLD_BYTES,
            path: path.to_string_lossy().to_string(),
        },
    }
}

pub(crate) async fn actuator_info(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_info_payload(ActuatorInfoSnapshot {
        build: ActuatorBuildInfo {
            version: non_empty_string(app.operational.build_metadata.version.as_str()),
            build_time: non_empty_string(app.operational.build_metadata.build_time.as_str()),
            git_branch: app.operational.build_metadata.git_branch.clone(),
            git_commit_id: app.operational.build_metadata.git_commit_id.clone(),
            git_commit_time: app.operational.build_metadata.git_commit_time.clone(),
        },
        os: ActuatorOsInfo {
            name: normalized_os_name(std::env::consts::OS),
            arch: normalized_arch(std::env::consts::ARCH),
            version: os_version(),
        },
        process: ActuatorProcessInfo {
            pid: std::process::id(),
            parent_pid: process_parent_pid(),
            cpus: available_cpu_count(),
            virtual_threads: false,
            memory: process_memory_snapshot(),
        },
    }))
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
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
    actuator_json(actuator_metrics_index_payload())
}

pub(crate) async fn actuator_metric_detail(
    State(app): State<OperationalApiState>,
    _admin: Admin,
    uri: Uri,
    AxumPath(metric_name): AxumPath<String>,
) -> Response {
    let probes = metric_probe_snapshot(&app);
    let tag_filters = actuator_metric_query_tags(uri.query());
    let service = ActuatorMetricService::new(app.operational_runtime.as_ref());

    match service
        .metric_detail_payload(&metric_name, &probes, &tag_filters)
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

fn metric_probe_snapshot(app: &OperationalApiState) -> ActuatorMetricProbeSnapshot {
    let startup = app.operational.startup_timing.snapshot();
    let disk_space = disk_space_details(&disk_space_probe_path(app));
    let http_server_requests = app
        .operational
        .http_server_requests
        .snapshot()
        .into_iter()
        .map(|(key, summary)| ActuatorHttpServerRequestMetric {
            exception: key.exception,
            method: key.method,
            outcome: key.outcome,
            status: key.status,
            uri: key.uri,
            count: summary.count,
            total_time_seconds: summary.total_time_seconds,
            max_time_seconds: summary.max_time_seconds,
        })
        .collect();

    ActuatorMetricProbeSnapshot {
        application_ready_time_seconds: startup.application_ready_time_seconds,
        application_started_time_seconds: startup.application_started_time_seconds,
        disk_free_bytes: disk_space
            .as_ref()
            .map(|details| details.free as f64)
            .unwrap_or(0.0),
        disk_total_bytes: disk_space
            .as_ref()
            .map(|details| details.total as f64)
            .unwrap_or(0.0),
        process_cpu_usage: process_cpu_usage_fraction().unwrap_or(0.0),
        process_files_max: process_files_max().unwrap_or(0.0),
        process_files_open: process_files_open().unwrap_or(0.0),
        process_start_time_seconds: process_start_time_epoch_seconds().unwrap_or(0.0),
        process_uptime_seconds: process_uptime_seconds().unwrap_or(0.0),
        system_cpu_count: available_cpu_count() as f64,
        system_cpu_usage: system_cpu_usage_fraction().unwrap_or(0.0),
        system_load_average_1m: one_minute_load_average().unwrap_or(0.0),
        http_server_requests,
        main_db_path: app.auth_db.database_file.as_path().to_path_buf(),
        tasks_db_path: app.operational.runtime.tasks_db_file.clone(),
    }
}

fn process_memory_snapshot() -> ActuatorProcessMemorySnapshot {
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
        ActuatorProcessMemorySnapshot {
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
        ActuatorProcessMemorySnapshot {
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

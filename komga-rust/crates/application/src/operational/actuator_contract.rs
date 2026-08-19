use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::metrics_port::{LibraryMetricValue, OperationalMetricsPort};
use crate::task_processing::TaskKind;

#[derive(Clone, Debug)]
pub struct ActuatorHealthSnapshot {
    pub sqlite_rw_ready: bool,
    pub sqlite_ro_ready: bool,
    pub tasks_rw_ready: bool,
    pub tasks_ro_ready: bool,
    pub disk_space: ActuatorDiskSpaceSnapshot,
}

#[derive(Clone, Debug)]
pub struct ActuatorDiskSpaceSnapshot {
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub threshold: u64,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorHealthStatus {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorHealthReport {
    pub status: ActuatorHealthStatus,
    pub db: ActuatorDatabaseHealthReport,
    pub disk_space: ActuatorDiskSpaceHealthReport,
    pub ping: ActuatorPingHealthReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorDatabaseHealthReport {
    pub status: ActuatorHealthStatus,
    pub sqlite_rw: ActuatorDatasourceHealthReport,
    pub sqlite_ro: ActuatorDatasourceHealthReport,
    pub tasks_rw: ActuatorDatasourceHealthReport,
    pub tasks_ro: ActuatorDatasourceHealthReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorDatasourceHealthReport {
    pub status: ActuatorHealthStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorDiskSpaceHealthReport {
    pub status: ActuatorHealthStatus,
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub threshold: u64,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorPingHealthReport {
    pub status: ActuatorHealthStatus,
}

pub fn actuator_health_report(snapshot: ActuatorHealthSnapshot) -> ActuatorHealthReport {
    let db = db_health_component(&snapshot);
    let disk_space = disk_space_component(&snapshot.disk_space);
    let ping = ping_component();
    let status = aggregate_health_status([db.status, disk_space.status, ping.status]);

    ActuatorHealthReport {
        status,
        db,
        disk_space,
        ping,
    }
}

fn aggregate_health_status(
    statuses: impl IntoIterator<Item = ActuatorHealthStatus>,
) -> ActuatorHealthStatus {
    if statuses
        .into_iter()
        .all(|status| status == ActuatorHealthStatus::Up)
    {
        ActuatorHealthStatus::Up
    } else {
        ActuatorHealthStatus::Down
    }
}

fn aggregate_health_is_up(statuses: impl IntoIterator<Item = bool>) -> bool {
    statuses.into_iter().all(|status| status)
}

fn component_status(is_up: bool) -> ActuatorHealthStatus {
    if is_up {
        ActuatorHealthStatus::Up
    } else {
        ActuatorHealthStatus::Down
    }
}

fn db_health_component(snapshot: &ActuatorHealthSnapshot) -> ActuatorDatabaseHealthReport {
    let is_up = aggregate_health_is_up([
        snapshot.sqlite_rw_ready,
        snapshot.sqlite_ro_ready,
        snapshot.tasks_rw_ready,
        snapshot.tasks_ro_ready,
    ]);

    ActuatorDatabaseHealthReport {
        status: component_status(is_up),
        sqlite_rw: sqlite_datasource_health_component(snapshot.sqlite_rw_ready),
        sqlite_ro: sqlite_datasource_health_component(snapshot.sqlite_ro_ready),
        tasks_rw: sqlite_datasource_health_component(snapshot.tasks_rw_ready),
        tasks_ro: sqlite_datasource_health_component(snapshot.tasks_ro_ready),
    }
}

fn sqlite_datasource_health_component(is_up: bool) -> ActuatorDatasourceHealthReport {
    ActuatorDatasourceHealthReport {
        status: component_status(is_up),
    }
}

fn ping_component() -> ActuatorPingHealthReport {
    ActuatorPingHealthReport {
        status: ActuatorHealthStatus::Up,
    }
}

fn disk_space_component(snapshot: &ActuatorDiskSpaceSnapshot) -> ActuatorDiskSpaceHealthReport {
    match (snapshot.total, snapshot.free) {
        (Some(total), Some(free)) => {
            let is_up = free >= snapshot.threshold;
            ActuatorDiskSpaceHealthReport {
                status: component_status(is_up),
                total: Some(total),
                free: Some(free),
                threshold: snapshot.threshold,
                path: snapshot.path.clone(),
            }
        }
        _ => ActuatorDiskSpaceHealthReport {
            status: ActuatorHealthStatus::Down,
            total: snapshot.total,
            free: snapshot.free,
            threshold: snapshot.threshold,
            path: snapshot.path.clone(),
        },
    }
}

#[derive(Clone, Debug, Default)]
pub struct ActuatorBuildInfo {
    pub version: Option<String>,
    pub build_time: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_id: Option<String>,
    pub git_commit_time: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ActuatorOsInfo {
    pub name: String,
    pub arch: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ActuatorProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub cpus: u64,
    pub virtual_threads: bool,
    pub memory: ActuatorProcessMemorySnapshot,
}

#[derive(Clone, Debug, Default)]
pub struct ActuatorProcessMemorySnapshot {
    pub heap_used: u64,
    pub heap_committed: u64,
    pub heap_max: u64,
    pub non_heap_used: u64,
    pub non_heap_committed: u64,
    pub non_heap_max: u64,
}

#[derive(Clone, Debug)]
pub struct ActuatorInfoSnapshot {
    pub build: ActuatorBuildInfo,
    pub os: ActuatorOsInfo,
    pub process: ActuatorProcessInfo,
}

#[derive(Clone, Debug, Default)]
pub struct ActuatorMetricProbeSnapshot {
    pub application_ready_time_seconds: f64,
    pub application_started_time_seconds: f64,
    pub disk_free_bytes: f64,
    pub disk_total_bytes: f64,
    pub process_cpu_usage: f64,
    pub process_files_max: f64,
    pub process_files_open: f64,
    pub process_start_time_seconds: f64,
    pub process_uptime_seconds: f64,
    pub system_cpu_count: f64,
    pub system_cpu_usage: f64,
    pub system_load_average_1m: f64,
    pub http_server_requests: Vec<ActuatorHttpServerRequestMetric>,
    pub main_db_path: PathBuf,
    pub tasks_db_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ActuatorHttpServerRequestMetric {
    pub exception: String,
    pub method: String,
    pub outcome: String,
    pub status: String,
    pub uri: String,
    pub count: u64,
    pub total_time_seconds: f64,
    pub max_time_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorMetricDetail {
    pub name: String,
    pub description: String,
    pub base_unit: Option<String>,
    pub measurements: Vec<ActuatorMetricMeasurement>,
    pub available_tags: Vec<ActuatorMetricAvailableTag>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorMetricMeasurement {
    pub statistic: String,
    pub value: f64,
}

impl ActuatorMetricMeasurement {
    fn new(statistic: impl Into<String>, value: f64) -> Self {
        Self {
            statistic: statistic.into(),
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActuatorMetricAvailableTag {
    pub tag: String,
    pub values: Vec<String>,
}

impl ActuatorMetricAvailableTag {
    fn new(tag: impl Into<String>, values: Vec<String>) -> Self {
        Self {
            tag: tag.into(),
            values,
        }
    }
}

pub struct ActuatorMetricService<'a> {
    runtime: &'a dyn OperationalMetricsPort,
}

impl<'a> ActuatorMetricService<'a> {
    pub fn new(runtime: &'a dyn OperationalMetricsPort) -> Self {
        Self { runtime }
    }

    pub async fn metric_detail(
        &self,
        metric_name: &str,
        probes: &ActuatorMetricProbeSnapshot,
        tag_filters: &HashMap<String, String>,
    ) -> anyhow::Result<Option<ActuatorMetricDetail>> {
        match metric_name {
            "application.ready.time" => Ok(Some(single_measurement_metric(
                metric_name,
                "Time taken for the application to be ready to service requests",
                Some("seconds"),
                "TOTAL_TIME",
                probes.application_ready_time_seconds,
            ))),
            "application.started.time" => Ok(Some(single_measurement_metric(
                metric_name,
                "Time taken to start the application",
                Some("seconds"),
                "TOTAL_TIME",
                probes.application_started_time_seconds,
            ))),
            "disk.free" => Ok(Some(single_measurement_metric(
                metric_name,
                "Usable disk space",
                Some("bytes"),
                "VALUE",
                probes.disk_free_bytes,
            ))),
            "disk.total" => Ok(Some(single_measurement_metric(
                metric_name,
                "Total disk space",
                Some("bytes"),
                "VALUE",
                probes.disk_total_bytes,
            ))),
            "http.server.requests" => Ok(Some(http_server_requests_metric(
                &probes.http_server_requests,
                tag_filters,
            ))),
            "jdbc.connections.active" => {
                self.jdbc_connections_metric(
                    probes,
                    metric_name,
                    "Active connections",
                    tag_filters,
                    JdbcConnectionsField::Active,
                )
                .await
            }
            "jdbc.connections.idle" => {
                self.jdbc_connections_metric(
                    probes,
                    metric_name,
                    "Idle connections",
                    tag_filters,
                    JdbcConnectionsField::Idle,
                )
                .await
            }
            "jdbc.connections.max" => {
                self.jdbc_connections_metric(
                    probes,
                    metric_name,
                    "Max connections",
                    tag_filters,
                    JdbcConnectionsField::Max,
                )
                .await
            }
            "jdbc.connections.min" => {
                self.jdbc_connections_metric(
                    probes,
                    metric_name,
                    "Min connections",
                    tag_filters,
                    JdbcConnectionsField::Min,
                )
                .await
            }
            "process.cpu.usage" => Ok(Some(single_measurement_metric(
                metric_name,
                "The recent CPU usage for the komga-rust process",
                None,
                "VALUE",
                probes.process_cpu_usage,
            ))),
            "process.files.max" => Ok(Some(single_measurement_metric(
                metric_name,
                "The maximum file descriptor count",
                Some("files"),
                "VALUE",
                probes.process_files_max,
            ))),
            "process.files.open" => Ok(Some(single_measurement_metric(
                metric_name,
                "The open file descriptor count",
                Some("files"),
                "VALUE",
                probes.process_files_open,
            ))),
            "process.start.time" => Ok(Some(single_measurement_metric(
                metric_name,
                "Start time of the process since unix epoch",
                Some("seconds"),
                "VALUE",
                probes.process_start_time_seconds,
            ))),
            "process.uptime" => Ok(Some(single_measurement_metric(
                metric_name,
                "The uptime of the komga-rust",
                Some("seconds"),
                "VALUE",
                probes.process_uptime_seconds,
            ))),
            "system.cpu.count" => Ok(Some(single_measurement_metric(
                metric_name,
                "The number of processors available to the komga-rust",
                Some("cpu"),
                "VALUE",
                probes.system_cpu_count,
            ))),
            "system.cpu.usage" => Ok(Some(single_measurement_metric(
                metric_name,
                "The recent cpu usage of the whole system",
                None,
                "VALUE",
                probes.system_cpu_usage,
            ))),
            "system.load.average.1m" => Ok(Some(single_measurement_metric(
                metric_name,
                "The sum of the number of runnable entities queued to the available processors and the number of runnable entities running on the available processors averaged over a period of time",
                None,
                "VALUE",
                probes.system_load_average_1m,
            ))),
            "komga.tasks.execution" => self
                .metric_tasks_execution(tag_filters.get("type").map(String::as_str))
                .await
                .map(Some),
            "komga.tasks.failure" => self.metric_tasks_failure().await.map(Some),
            "komga.libraries" => Ok(Some(simple_metric(
                metric_name,
                "Libraries count",
                Some("count"),
                self.runtime.load_libraries_count().await?,
            ))),
            "komga.series" => Ok(Some(metric_library_value(
                metric_name,
                "Series count grouped by library",
                Some("count"),
                self.runtime.load_series_grouped_by_library().await?,
                tag_filters.get("library").map(String::as_str),
            ))),
            "komga.books" => Ok(Some(metric_library_value(
                metric_name,
                "Books count grouped by library",
                Some("count"),
                self.runtime.load_books_grouped_by_library().await?,
                tag_filters.get("library").map(String::as_str),
            ))),
            "komga.books.filesize" => Ok(Some(metric_library_value(
                metric_name,
                "Books file size grouped by library",
                Some("bytes"),
                self.runtime
                    .load_books_filesize_grouped_by_library()
                    .await?,
                tag_filters.get("library").map(String::as_str),
            ))),
            "komga.sidecars" => Ok(Some(metric_library_value(
                metric_name,
                "Sidecars count grouped by library",
                Some("count"),
                self.runtime.load_sidecars_grouped_by_library().await?,
                tag_filters.get("library").map(String::as_str),
            ))),
            "komga.collections" => Ok(Some(simple_metric(
                metric_name,
                "Collections count",
                Some("count"),
                self.runtime.load_collections_count().await?,
            ))),
            "komga.readlists" => Ok(Some(simple_metric(
                metric_name,
                "Read lists count",
                Some("count"),
                self.runtime.load_readlists_count().await?,
            ))),
            _ => Ok(None),
        }
    }

    async fn metric_tasks_execution(
        &self,
        task_type: Option<&str>,
    ) -> anyhow::Result<ActuatorMetricDetail> {
        let values = self.runtime.load_task_execution_values().await?;

        let count = if let Some(task_type) = task_type {
            values
                .iter()
                .find(|value| value.task_type == task_type)
                .map(|value| value.count)
                .unwrap_or(0.0)
        } else {
            values.iter().map(|value| value.count).sum::<f64>()
        };

        let tags = unique_strings(
            values
                .iter()
                .map(|value| value.task_type.clone())
                .chain(known_task_metric_types()),
        );
        let total_time = count * 0.01;
        let max_time = if count > 0.0 { 0.01 } else { 0.0 };

        Ok(ActuatorMetricDetail {
            name: "komga.tasks.execution".to_string(),
            description: "Task execution statistics".to_string(),
            base_unit: None,
            measurements: vec![
                ActuatorMetricMeasurement::new("COUNT", count),
                ActuatorMetricMeasurement::new("TOTAL_TIME", total_time),
                ActuatorMetricMeasurement::new("MAX", max_time),
            ],
            available_tags: vec![ActuatorMetricAvailableTag::new("type", tags)],
        })
    }

    async fn metric_tasks_failure(&self) -> anyhow::Result<ActuatorMetricDetail> {
        let failures = self.runtime.load_task_failure_count().await?;
        let task_types = unique_strings(
            self.runtime
                .load_task_execution_values()
                .await?
                .into_iter()
                .map(|value| value.task_type)
                .chain(known_task_metric_types()),
        );

        Ok(ActuatorMetricDetail {
            name: "komga.tasks.failure".to_string(),
            description: "Count of failed tasks".to_string(),
            base_unit: None,
            measurements: vec![ActuatorMetricMeasurement::new("COUNT", failures)],
            available_tags: vec![ActuatorMetricAvailableTag::new("type", task_types)],
        })
    }

    async fn jdbc_connections_metric(
        &self,
        probes: &ActuatorMetricProbeSnapshot,
        name: &str,
        description: &str,
        tag_filters: &HashMap<String, String>,
        field: JdbcConnectionsField,
    ) -> anyhow::Result<Option<ActuatorMetricDetail>> {
        let samples = self
            .runtime
            .load_database_pool_snapshots(&[
                probes.main_db_path.clone(),
                probes.tasks_db_path.clone(),
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
                    vec![MetricTag::new(
                        "name",
                        datasource_pool_name(
                            &probes.main_db_path,
                            &probes.tasks_db_path,
                            &pool.path,
                            pool.max_connections,
                        ),
                    )],
                    [MetricMeasurement::new("VALUE", value)],
                )
            })
            .collect();

        Ok(Some(metric_from_samples(
            name,
            description,
            Some("connections"),
            samples,
            tag_filters,
        )))
    }
}

fn metric_library_value(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    values: Vec<LibraryMetricValue>,
    requested_library_id: Option<&str>,
) -> ActuatorMetricDetail {
    let value = match requested_library_id {
        Some(library_id) => values
            .iter()
            .find(|value| value.library_id == library_id)
            .map(|value| value.value)
            .unwrap_or(0.0),
        None => values.iter().map(|value| value.value).sum::<f64>(),
    };

    ActuatorMetricDetail {
        name: name.to_string(),
        description: description.to_string(),
        base_unit: base_unit.map(str::to_string),
        measurements: vec![ActuatorMetricMeasurement::new("VALUE", value)],
        available_tags: vec![ActuatorMetricAvailableTag::new(
            "library",
            values
                .iter()
                .map(|value| value.library_id.clone())
                .collect::<Vec<_>>(),
        )],
    }
}

fn simple_metric(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    value: f64,
) -> ActuatorMetricDetail {
    single_measurement_metric(name, description, base_unit, "VALUE", value)
}

pub fn actuator_metric_names() -> Vec<&'static str> {
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
    tags: Vec<MetricTag>,
    measurements: Vec<MetricMeasurement>,
}

struct MetricTag {
    key: &'static str,
    value: String,
}

impl MetricTag {
    fn new(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
        }
    }
}

#[derive(Clone, Copy)]
struct MetricMeasurement {
    statistic: &'static str,
    value: f64,
}

impl MetricMeasurement {
    fn new(statistic: &'static str, value: f64) -> Self {
        Self { statistic, value }
    }
}

impl MetricSample {
    fn with_owned_tags<const M: usize>(
        tags: Vec<MetricTag>,
        measurements: [MetricMeasurement; M],
    ) -> Self {
        Self {
            tags,
            measurements: measurements.into_iter().collect(),
        }
    }

    fn tag_value(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|tag| tag.key == key)
            .map(|tag| tag.value.as_str())
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
) -> ActuatorMetricDetail {
    ActuatorMetricDetail {
        name: name.to_string(),
        description: description.to_string(),
        base_unit: base_unit.map(str::to_string),
        measurements: vec![ActuatorMetricMeasurement::new(statistic, value)],
        available_tags: Vec::new(),
    }
}

fn metric_from_samples(
    name: &str,
    description: &str,
    base_unit: Option<&str>,
    samples: Vec<MetricSample>,
    tag_filters: &HashMap<String, String>,
) -> ActuatorMetricDetail {
    let matching_samples = samples
        .iter()
        .filter(|sample| sample.matches_filters(tag_filters))
        .collect::<Vec<_>>();

    let mut aggregated_measurements = Vec::<MetricMeasurement>::new();
    for sample in &matching_samples {
        for measurement in &sample.measurements {
            if let Some(existing) = aggregated_measurements
                .iter_mut()
                .find(|candidate| candidate.statistic == measurement.statistic)
            {
                existing.value += measurement.value;
            } else {
                aggregated_measurements.push(*measurement);
            }
        }
    }

    let mut ordered_tag_keys = Vec::<&'static str>::new();
    for sample in &samples {
        for tag in &sample.tags {
            if !ordered_tag_keys.contains(&tag.key) {
                ordered_tag_keys.push(tag.key);
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
                Some(ActuatorMetricAvailableTag::new(key, values))
            }
        })
        .collect::<Vec<_>>();

    ActuatorMetricDetail {
        name: name.to_string(),
        description: description.to_string(),
        base_unit: base_unit.map(str::to_string),
        measurements: aggregated_measurements
            .into_iter()
            .map(|measurement| {
                ActuatorMetricMeasurement::new(measurement.statistic, measurement.value)
            })
            .collect(),
        available_tags,
    }
}

fn http_server_requests_metric(
    requests: &[ActuatorHttpServerRequestMetric],
    tag_filters: &HashMap<String, String>,
) -> ActuatorMetricDetail {
    let samples = requests
        .iter()
        .map(|request| {
            MetricSample::with_owned_tags(
                vec![
                    MetricTag::new("exception", request.exception.clone()),
                    MetricTag::new("method", request.method.clone()),
                    MetricTag::new("outcome", request.outcome.clone()),
                    MetricTag::new("status", request.status.clone()),
                    MetricTag::new("uri", request.uri.clone()),
                ],
                [
                    MetricMeasurement::new("COUNT", request.count as f64),
                    MetricMeasurement::new("TOTAL_TIME", request.total_time_seconds),
                    MetricMeasurement::new("MAX", request.max_time_seconds),
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

fn datasource_pool_name(
    main_db_path: &Path,
    tasks_db_path: &Path,
    pool_path: &Path,
    max_connections: u32,
) -> String {
    let normalized_main_path = normalized_runtime_path(main_db_path);
    let normalized_tasks_path = normalized_runtime_path(tasks_db_path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_report_aggregates_component_statuses() {
        let report = actuator_health_report(ActuatorHealthSnapshot {
            sqlite_rw_ready: true,
            sqlite_ro_ready: true,
            tasks_rw_ready: false,
            tasks_ro_ready: false,
            disk_space: ActuatorDiskSpaceSnapshot {
                total: Some(100),
                free: Some(50),
                threshold: 10,
                path: "/tmp".to_string(),
            },
        });

        assert_eq!(report.status, ActuatorHealthStatus::Down);
        assert_eq!(report.db.status, ActuatorHealthStatus::Down);
        assert_eq!(report.disk_space.status, ActuatorHealthStatus::Up);
        assert_eq!(report.ping.status, ActuatorHealthStatus::Up);
    }

    #[test]
    fn http_request_metric_filters_samples_and_exposes_remaining_tags() {
        let requests = vec![
            ActuatorHttpServerRequestMetric {
                exception: "None".to_string(),
                method: "GET".to_string(),
                outcome: "SUCCESS".to_string(),
                status: "200".to_string(),
                uri: "/actuator/info".to_string(),
                count: 2,
                total_time_seconds: 0.5,
                max_time_seconds: 0.3,
            },
            ActuatorHttpServerRequestMetric {
                exception: "None".to_string(),
                method: "GET".to_string(),
                outcome: "CLIENT_ERROR".to_string(),
                status: "401".to_string(),
                uri: "/actuator".to_string(),
                count: 1,
                total_time_seconds: 0.1,
                max_time_seconds: 0.1,
            },
        ];
        let filters = HashMap::from_iter([
            ("method".to_string(), "GET".to_string()),
            ("outcome".to_string(), "SUCCESS".to_string()),
        ]);

        let metric = http_server_requests_metric(&requests, &filters);

        assert_eq!(
            metric.measurements[0],
            ActuatorMetricMeasurement {
                statistic: "COUNT".to_string(),
                value: 2.0,
            }
        );
        assert_eq!(
            metric.available_tags[0],
            ActuatorMetricAvailableTag {
                tag: "exception".to_string(),
                values: vec!["None".to_string()],
            }
        );
        assert_eq!(
            metric.available_tags[1],
            ActuatorMetricAvailableTag {
                tag: "status".to_string(),
                values: vec!["200".to_string()],
            }
        );
    }
}

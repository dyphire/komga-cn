use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::metrics_port::OperationalMetricsPort;
use crate::task_processing::TaskKind;

const PRODUCT_GROUP: &str = "huihuimoe";
const PRODUCT_ARTIFACT: &str = "komga";
const PRODUCT_NAME: &str = "komga-rust";
const DEFAULT_PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn actuator_root_payload() -> Value {
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

pub struct ActuatorHealthSnapshot {
    pub sqlite_rw_ready: bool,
    pub sqlite_ro_ready: bool,
    pub tasks_rw_ready: bool,
    pub tasks_ro_ready: bool,
    pub disk_space: ActuatorDiskSpaceSnapshot,
}

pub struct ActuatorDiskSpaceSnapshot {
    pub total: Option<u64>,
    pub free: Option<u64>,
    pub threshold: u64,
    pub path: String,
}

pub fn actuator_health_payload(snapshot: ActuatorHealthSnapshot, include_details: bool) -> Value {
    let db = db_health_component(&snapshot);
    let disk_space = disk_space_component(&snapshot.disk_space);
    let ping = ping_component();
    let status = aggregate_health_status([db.is_up, disk_space.is_up, ping.is_up]);

    if !include_details {
        return json!({ "status": status });
    }

    json!({
        "status": status,
        "components": {
            "db": db.payload,
            "diskSpace": disk_space.payload,
            "ping": ping.payload,
        }
    })
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

fn db_health_component(snapshot: &ActuatorHealthSnapshot) -> HealthComponentPayload {
    let is_up = aggregate_health_is_up([
        snapshot.sqlite_rw_ready,
        snapshot.sqlite_ro_ready,
        snapshot.tasks_rw_ready,
        snapshot.tasks_ro_ready,
    ]);

    HealthComponentPayload {
        is_up,
        payload: json!({
            "status": component_status(is_up),
            "components": {
                "sqliteDataSourceRW": sqlite_datasource_health_component(snapshot.sqlite_rw_ready),
                "sqliteDataSourceRO": sqlite_datasource_health_component(snapshot.sqlite_ro_ready),
                "tasksDataSourceRW": sqlite_datasource_health_component(snapshot.tasks_rw_ready),
                "tasksDataSourceRO": sqlite_datasource_health_component(snapshot.tasks_ro_ready),
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

fn disk_space_component(snapshot: &ActuatorDiskSpaceSnapshot) -> HealthComponentPayload {
    match (snapshot.total, snapshot.free) {
        (Some(total), Some(free)) => {
            let is_up = free >= snapshot.threshold;
            HealthComponentPayload {
                is_up,
                payload: json!({
                    "status": component_status(is_up),
                    "details": {
                        "total": total,
                        "free": free,
                        "threshold": snapshot.threshold,
                        "path": snapshot.path,
                    }
                }),
            }
        }
        _ => HealthComponentPayload {
            is_up: false,
            payload: json!({
                "status": "DOWN",
                "details": {
                    "threshold": snapshot.threshold,
                    "path": snapshot.path,
                }
            }),
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

pub struct ActuatorInfoSnapshot {
    pub build: ActuatorBuildInfo,
    pub os: ActuatorOsInfo,
    pub process: ActuatorProcessInfo,
}

pub fn actuator_info_payload(snapshot: ActuatorInfoSnapshot) -> Value {
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

pub fn actuator_metrics_index_payload() -> Value {
    json!({
        "names": actuator_metric_names(),
    })
}

pub fn actuator_metric_query_tags(query: Option<&str>) -> HashMap<String, String> {
    query
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.strip_prefix("tag="))
        .filter_map(|pair| pair.split_once(':'))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
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

pub struct ActuatorMetricService<'a> {
    runtime: &'a dyn OperationalMetricsPort,
}

impl<'a> ActuatorMetricService<'a> {
    pub fn new(runtime: &'a dyn OperationalMetricsPort) -> Self {
        Self { runtime }
    }

    pub async fn metric_detail_payload(
        &self,
        metric_name: &str,
        probes: &ActuatorMetricProbeSnapshot,
        tag_filters: &HashMap<String, String>,
    ) -> Result<Option<Value>, String> {
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

    async fn metric_tasks_execution(&self, task_type: Option<&str>) -> Result<Value, String> {
        let values = self.runtime.load_task_execution_values().await?;

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

    async fn metric_tasks_failure(&self) -> Result<Value, String> {
        let failures = self.runtime.load_task_failure_count().await?;
        let task_types = unique_strings(
            self.runtime
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

    async fn jdbc_connections_metric(
        &self,
        probes: &ActuatorMetricProbeSnapshot,
        name: &str,
        description: &str,
        tag_filters: &HashMap<String, String>,
        field: JdbcConnectionsField,
    ) -> Result<Option<Value>, String> {
        let samples = self
            .runtime
            .load_sqlite_pool_snapshots(&[
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
                    vec![(
                        "name",
                        datasource_pool_name(
                            &probes.main_db_path,
                            &probes.tasks_db_path,
                            &pool.path,
                            pool.max_connections,
                        ),
                    )],
                    [("VALUE", value)],
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
    values: Vec<(String, f64)>,
    requested_library: Option<&str>,
) -> Value {
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

    Value::Object(metric)
}

fn simple_metric(name: &str, description: &str, base_unit: Option<&str>, value: f64) -> Value {
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
    requests: &[ActuatorHttpServerRequestMetric],
    tag_filters: &HashMap<String, String>,
) -> Value {
    let samples = requests
        .iter()
        .map(|request| {
            MetricSample::with_owned_tags(
                vec![
                    ("exception", request.exception.clone()),
                    ("method", request.method.clone()),
                    ("outcome", request.outcome.clone()),
                    ("status", request.status.clone()),
                    ("uri", request.uri.clone()),
                ],
                [
                    ("COUNT", request.count as f64),
                    ("TOTAL_TIME", request.total_time_seconds),
                    ("MAX", request.max_time_seconds),
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
    fn health_payload_hides_components_without_details() {
        let payload = actuator_health_payload(
            ActuatorHealthSnapshot {
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
            },
            false,
        );

        assert_eq!(payload, json!({ "status": "DOWN" }));
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
        let filters = actuator_metric_query_tags(Some("tag=method:GET&tag=outcome:SUCCESS"));

        let payload = http_server_requests_metric(&requests, &filters);

        assert_eq!(
            payload["measurements"][0],
            json!({"statistic": "COUNT", "value": 2.0})
        );
        assert_eq!(
            payload["availableTags"][0],
            json!({"tag": "exception", "values": ["None"]})
        );
        assert_eq!(
            payload["availableTags"][1],
            json!({"tag": "status", "values": ["200"]})
        );
    }
}

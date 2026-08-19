use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ActuatorRootDto {
    #[serde(rename = "_links")]
    pub links: BTreeMap<String, ActuatorLinkDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorLinkDto {
    pub href: String,
    pub templated: bool,
}

#[derive(Debug, Serialize)]
pub struct ActuatorHealthDto {
    pub status: ActuatorHealthStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<ActuatorHealthComponentsDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorHealthComponentsDto {
    pub db: ActuatorDatabaseHealthDto,
    pub disk_space: ActuatorDiskSpaceDto,
    pub ping: ActuatorPingHealthDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorDatabaseHealthDto {
    pub status: ActuatorHealthStatusDto,
    pub components: ActuatorDatabaseComponentsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorDatabaseComponentsDto {
    #[serde(rename = "sqliteDataSourceRW")]
    pub sqlite_rw: ActuatorDatasourceHealthDto,
    #[serde(rename = "sqliteDataSourceRO")]
    pub sqlite_ro: ActuatorDatasourceHealthDto,
    #[serde(rename = "tasksDataSourceRW")]
    pub tasks_rw: ActuatorDatasourceHealthDto,
    #[serde(rename = "tasksDataSourceRO")]
    pub tasks_ro: ActuatorDatasourceHealthDto,
}

#[derive(Debug, Serialize)]
pub struct ActuatorDatasourceHealthDto {
    pub status: ActuatorHealthStatusDto,
    pub details: ActuatorDatasourceDetailsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorDatasourceDetailsDto {
    pub database: &'static str,
    pub validation_query: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ActuatorDiskSpaceDto {
    pub status: ActuatorHealthStatusDto,
    pub details: ActuatorDiskSpaceDetailsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorDiskSpaceDetailsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free: Option<u64>,
    pub threshold: u64,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ActuatorPingHealthDto {
    pub status: ActuatorHealthStatusDto,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ActuatorHealthStatusDto {
    #[serde(rename = "UP")]
    Up,
    #[serde(rename = "DOWN")]
    Down,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorInfoDto {
    pub build: ActuatorBuildDto,
    pub os: ActuatorOsDto,
    pub process: ActuatorProcessDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<ActuatorGitDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorBuildDto {
    pub artifact: String,
    pub name: String,
    pub version: String,
    pub group: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorGitDto {
    pub branch: Option<String>,
    pub commit: ActuatorGitCommitDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorGitCommitDto {
    pub id: Option<String>,
    pub time: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorOsDto {
    pub name: String,
    pub arch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorProcessDto {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub cpus: u64,
    pub virtual_threads: bool,
    pub memory: ActuatorProcessMemoryDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorProcessMemoryDto {
    pub heap: ActuatorMemoryPoolDto,
    pub non_heap: ActuatorMemoryPoolDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorMemoryPoolDto {
    pub used: u64,
    pub committed: u64,
    pub max: u64,
}

#[derive(Debug, Serialize)]
pub struct ActuatorMetricsIndexDto {
    pub names: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorMetricDetailDto {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_unit: Option<String>,
    pub measurements: Vec<ActuatorMetricMeasurementDto>,
    pub available_tags: Vec<ActuatorMetricAvailableTagDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorMetricMeasurementDto {
    pub statistic: String,
    pub value: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorMetricAvailableTagDto {
    pub tag: String,
    pub values: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ActuatorMessageDto {
    pub message: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ActuatorErrorDto {
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorLogfileErrorDto {
    pub error: &'static str,
    pub path: String,
}

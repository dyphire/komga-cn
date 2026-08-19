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
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;

use crate::contracts::actuator::{
    ActuatorBuildDto, ActuatorDatabaseComponentsDto, ActuatorDatabaseHealthDto,
    ActuatorDatasourceDetailsDto, ActuatorDatasourceHealthDto, ActuatorDiskSpaceDetailsDto,
    ActuatorDiskSpaceDto, ActuatorErrorDto, ActuatorGitCommitDto, ActuatorGitDto,
    ActuatorHealthComponentsDto, ActuatorHealthDto, ActuatorHealthStatusDto, ActuatorInfoDto,
    ActuatorLinkDto, ActuatorLogfileErrorDto, ActuatorMemoryPoolDto, ActuatorMessageDto,
    ActuatorMetricAvailableTagDto, ActuatorMetricDetailDto, ActuatorMetricMeasurementDto,
    ActuatorMetricsIndexDto, ActuatorOsDto, ActuatorPingHealthDto, ActuatorProcessDto,
    ActuatorProcessMemoryDto, ActuatorRootDto,
};
use crate::identity_access::auth::{Admin, resolved_request_auth_user};
use crate::state::OperationalApiState;
use komga_application::identity_access::user_is_admin;

const ACTUATOR_V3_JSON: &str = "application/vnd.spring-boot.actuator.v3+json";
const PRODUCT_GROUP: &str = "huihuimoe";
const PRODUCT_ARTIFACT: &str = "komga";
const PRODUCT_NAME: &str = "komga-rust";
const DEFAULT_PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn actuator_json<T: Serialize>(payload: T) -> Response {
    ([(header::CONTENT_TYPE, ACTUATOR_V3_JSON)], Json(payload)).into_response()
}

pub(crate) async fn actuator_root(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_root_dto())
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

    Json(actuator_health_dto(health, include_details)).into_response()
}

pub(crate) async fn actuator_info(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_info_dto(
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
                Json(ActuatorLogfileErrorDto {
                    error: "log file not found",
                    path: app
                        .operational
                        .runtime
                        .log_file
                        .to_string_lossy()
                        .to_string(),
                }),
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

    Json(ActuatorMessageDto {
        message: "Shutting down, bye...",
    })
    .into_response()
}

pub(crate) async fn actuator_metrics_index(
    State(_app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    actuator_json(actuator_metrics_index_dto())
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
        Ok(Some(metric)) => actuator_json(actuator_metric_detail_dto(metric)),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(?error, "actuator metric detail failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ActuatorErrorDto {
                    error: format!("{error:#}"),
                }),
            )
                .into_response()
        }
    }
}

fn actuator_root_dto() -> ActuatorRootDto {
    ActuatorRootDto {
        links: actuator_root_links(),
    }
}

fn actuator_root_links() -> BTreeMap<String, ActuatorLinkDto> {
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

    links
        .into_iter()
        .map(|(name, href, templated)| {
            (
                name.to_string(),
                ActuatorLinkDto {
                    href: href.to_string(),
                    templated,
                },
            )
        })
        .collect()
}

fn actuator_health_dto(report: ActuatorHealthReport, include_details: bool) -> ActuatorHealthDto {
    ActuatorHealthDto {
        status: actuator_health_status(report.status),
        components: include_details.then(|| ActuatorHealthComponentsDto {
            db: actuator_database_health_dto(report.db),
            disk_space: actuator_disk_space_dto(report.disk_space),
            ping: actuator_ping_dto(report.ping),
        }),
    }
}

fn actuator_database_health_dto(report: ActuatorDatabaseHealthReport) -> ActuatorDatabaseHealthDto {
    ActuatorDatabaseHealthDto {
        status: actuator_health_status(report.status),
        components: ActuatorDatabaseComponentsDto {
            sqlite_rw: actuator_datasource_health_dto(report.sqlite_rw),
            sqlite_ro: actuator_datasource_health_dto(report.sqlite_ro),
            tasks_rw: actuator_datasource_health_dto(report.tasks_rw),
            tasks_ro: actuator_datasource_health_dto(report.tasks_ro),
        },
    }
}

fn actuator_datasource_health_dto(
    report: ActuatorDatasourceHealthReport,
) -> ActuatorDatasourceHealthDto {
    ActuatorDatasourceHealthDto {
        status: actuator_health_status(report.status),
        details: ActuatorDatasourceDetailsDto {
            database: "SQLite",
            validation_query: "isValid()",
        },
    }
}

fn actuator_disk_space_dto(report: ActuatorDiskSpaceHealthReport) -> ActuatorDiskSpaceDto {
    let (total, free) = report
        .total
        .zip(report.free)
        .map_or((None, None), |(total, free)| (Some(total), Some(free)));

    ActuatorDiskSpaceDto {
        status: actuator_health_status(report.status),
        details: ActuatorDiskSpaceDetailsDto {
            total,
            free,
            threshold: report.threshold,
            path: report.path,
        },
    }
}

fn actuator_ping_dto(report: ActuatorPingHealthReport) -> ActuatorPingHealthDto {
    ActuatorPingHealthDto {
        status: actuator_health_status(report.status),
    }
}

fn actuator_health_status(status: ActuatorHealthStatus) -> ActuatorHealthStatusDto {
    match status {
        ActuatorHealthStatus::Up => ActuatorHealthStatusDto::Up,
        ActuatorHealthStatus::Down => ActuatorHealthStatusDto::Down,
    }
}

fn actuator_info_dto(snapshot: ActuatorInfoSnapshot) -> ActuatorInfoDto {
    let git = if snapshot.build.git_branch.is_some()
        || snapshot.build.git_commit_id.is_some()
        || snapshot.build.git_commit_time.is_some()
    {
        Some(ActuatorGitDto {
            branch: snapshot.build.git_branch.clone(),
            commit: ActuatorGitCommitDto {
                id: snapshot.build.git_commit_id.clone(),
                time: snapshot.build.git_commit_time.clone(),
            },
        })
    } else {
        None
    };

    ActuatorInfoDto {
        build: build_info_dto(&snapshot.build),
        os: os_info_dto(snapshot.os),
        process: process_info_dto(snapshot.process),
        git,
    }
}

fn process_info_dto(process: ActuatorProcessInfo) -> ActuatorProcessDto {
    ActuatorProcessDto {
        pid: process.pid,
        parent_pid: process.parent_pid,
        cpus: process.cpus,
        virtual_threads: process.virtual_threads,
        memory: ActuatorProcessMemoryDto {
            heap: ActuatorMemoryPoolDto {
                used: process.memory.heap_used,
                committed: process.memory.heap_committed,
                max: process.memory.heap_max,
            },
            non_heap: ActuatorMemoryPoolDto {
                used: process.memory.non_heap_used,
                committed: process.memory.non_heap_committed,
                max: process.memory.non_heap_max,
            },
        },
    }
}

fn build_info_dto(build: &ActuatorBuildInfo) -> ActuatorBuildDto {
    let version = build
        .version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_PRODUCT_VERSION.to_string());

    ActuatorBuildDto {
        artifact: PRODUCT_ARTIFACT.to_string(),
        name: PRODUCT_NAME.to_string(),
        version,
        group: PRODUCT_GROUP.to_string(),
    }
}

fn os_info_dto(os: ActuatorOsInfo) -> ActuatorOsDto {
    ActuatorOsDto {
        name: os.name,
        arch: os.arch,
        version: os.version,
    }
}

fn actuator_metrics_index_dto() -> ActuatorMetricsIndexDto {
    ActuatorMetricsIndexDto {
        names: ActuatorService::metric_names()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
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

fn actuator_metric_detail_dto(metric: ActuatorMetricDetail) -> ActuatorMetricDetailDto {
    ActuatorMetricDetailDto {
        name: metric.name,
        description: metric.description,
        base_unit: metric.base_unit,
        measurements: metric
            .measurements
            .into_iter()
            .map(|measurement| ActuatorMetricMeasurementDto {
                statistic: measurement.statistic,
                value: measurement.value,
            })
            .collect(),
        available_tags: metric
            .available_tags
            .into_iter()
            .map(|tag| ActuatorMetricAvailableTagDto {
                tag: tag.tag,
                values: tag.values,
            })
            .collect(),
    }
}

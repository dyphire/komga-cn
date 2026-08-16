use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use super::*;

struct FakeActuatorSnapshots {
    health: ActuatorHealthSnapshot,
    info: ActuatorInfoSnapshot,
    probes: ActuatorMetricProbeSnapshot,
}

impl ActuatorSnapshotPort for FakeActuatorSnapshots {
    fn health_snapshot(&self) -> ActuatorHealthSnapshot {
        self.health.clone()
    }

    fn info_snapshot(&self) -> ActuatorInfoSnapshot {
        self.info.clone()
    }

    fn metric_probe_snapshot(&self) -> ActuatorMetricProbeSnapshot {
        self.probes.clone()
    }
}

#[derive(Default)]
struct FakeOperationalMetrics {
    requested_pool_paths: Mutex<Vec<PathBuf>>,
}

#[async_trait::async_trait]
impl OperationalMetricsPort for FakeOperationalMetrics {
    async fn load_task_execution_values(&self) -> anyhow::Result<Vec<TaskExecutionMetricValue>> {
        Ok(Vec::new())
    }

    async fn load_libraries_count(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn load_series_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        Ok(Vec::new())
    }

    async fn load_books_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        Ok(Vec::new())
    }

    async fn load_books_filesize_grouped_by_library(
        &self,
    ) -> anyhow::Result<Vec<LibraryMetricValue>> {
        Ok(Vec::new())
    }

    async fn load_sidecars_grouped_by_library(&self) -> anyhow::Result<Vec<LibraryMetricValue>> {
        Ok(Vec::new())
    }

    async fn load_collections_count(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn load_readlists_count(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn load_task_failure_count(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn load_database_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> anyhow::Result<Vec<DatabasePoolSnapshot>> {
        *self
            .requested_pool_paths
            .lock()
            .expect("requested pool paths lock should not be poisoned") = paths.to_vec();

        Ok(vec![DatabasePoolSnapshot {
            path: paths[0].clone(),
            max_connections: 7,
            min_connections: 1,
            total_connections: 3,
            idle_connections: 2,
            in_use_connections: 1,
            is_closed: false,
        }])
    }
}

fn fake_snapshots() -> FakeActuatorSnapshots {
    FakeActuatorSnapshots {
        health: ActuatorHealthSnapshot {
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
        info: ActuatorInfoSnapshot {
            build: ActuatorBuildInfo {
                version: Some("1.2.3".to_string()),
                build_time: None,
                git_branch: None,
                git_commit_id: None,
                git_commit_time: None,
            },
            os: ActuatorOsInfo {
                name: "Linux".to_string(),
                arch: "amd64".to_string(),
                version: Some("test".to_string()),
            },
            process: ActuatorProcessInfo {
                pid: 42,
                parent_pid: Some(1),
                cpus: 8,
                virtual_threads: false,
                memory: ActuatorProcessMemorySnapshot::default(),
            },
        },
        probes: ActuatorMetricProbeSnapshot {
            disk_free_bytes: 42.0,
            main_db_path: PathBuf::from("/runtime/main.sqlite"),
            tasks_db_path: PathBuf::from("/runtime/tasks.sqlite"),
            ..ActuatorMetricProbeSnapshot::default()
        },
    }
}

fn measurement_value(metric: &ActuatorMetricDetail, statistic: &str) -> f64 {
    metric
        .measurements
        .iter()
        .find(|measurement| measurement.statistic == statistic)
        .map(|measurement| measurement.value)
        .expect("metric should expose requested measurement")
}

#[tokio::test]
async fn metric_detail_uses_probe_snapshot_paths_for_runtime_pool_metrics() {
    let snapshots = fake_snapshots();
    let metrics = FakeOperationalMetrics::default();
    let service = ActuatorService::new(&snapshots, &metrics);

    let metric = service
        .metric_detail("jdbc.connections.max", &HashMap::new())
        .await
        .expect("metric detail should load")
        .expect("jdbc metric should exist");

    assert_eq!(measurement_value(&metric, "VALUE"), 7.0);
    assert_eq!(
        metrics
            .requested_pool_paths
            .lock()
            .expect("requested pool paths lock should not be poisoned")
            .clone(),
        vec![
            PathBuf::from("/runtime/main.sqlite"),
            PathBuf::from("/runtime/tasks.sqlite")
        ]
    );
    assert_eq!(
        metric.available_tags[0],
        ActuatorMetricAvailableTag {
            tag: "name".to_string(),
            values: vec!["main-pool-max-7".to_string()],
        }
    );
}

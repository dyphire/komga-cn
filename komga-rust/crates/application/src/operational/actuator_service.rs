use std::collections::HashMap;

use super::{
    ActuatorHealthReport, ActuatorHealthSnapshot, ActuatorInfoSnapshot, ActuatorMetricDetail,
    ActuatorMetricProbeSnapshot, ActuatorMetricService, OperationalMetricsPort,
    actuator_health_report, actuator_metric_names,
};

pub trait ActuatorSnapshotPort: Send + Sync {
    fn health_snapshot(&self) -> ActuatorHealthSnapshot;

    fn info_snapshot(&self) -> ActuatorInfoSnapshot;

    fn metric_probe_snapshot(&self) -> ActuatorMetricProbeSnapshot;
}

pub struct ActuatorService<'a> {
    snapshots: &'a dyn ActuatorSnapshotPort,
    metrics: &'a dyn OperationalMetricsPort,
}

impl<'a> ActuatorService<'a> {
    pub fn new(
        snapshots: &'a dyn ActuatorSnapshotPort,
        metrics: &'a dyn OperationalMetricsPort,
    ) -> Self {
        Self { snapshots, metrics }
    }

    pub fn health_report(&self) -> ActuatorHealthReport {
        actuator_health_report(self.snapshots.health_snapshot())
    }

    pub fn info_snapshot(&self) -> ActuatorInfoSnapshot {
        self.snapshots.info_snapshot()
    }

    pub fn metric_names() -> Vec<&'static str> {
        actuator_metric_names()
    }

    pub async fn metric_detail(
        &self,
        metric_name: &str,
        tag_filters: &HashMap<String, String>,
    ) -> Result<Option<ActuatorMetricDetail>, String> {
        let probes = self.snapshots.metric_probe_snapshot();
        ActuatorMetricService::new(self.metrics)
            .metric_detail(metric_name, &probes, tag_filters)
            .await
    }
}

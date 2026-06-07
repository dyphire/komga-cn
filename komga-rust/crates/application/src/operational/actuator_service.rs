use std::collections::HashMap;

use serde_json::Value;

use super::{
    ActuatorHealthSnapshot, ActuatorInfoSnapshot, ActuatorMetricProbeSnapshot,
    ActuatorMetricService, OperationalMetricsPort, actuator_health_payload, actuator_info_payload,
    actuator_metrics_index_payload, actuator_root_payload,
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

    pub fn root_payload() -> Value {
        actuator_root_payload()
    }

    pub fn health_payload(&self, include_details: bool) -> Value {
        actuator_health_payload(self.snapshots.health_snapshot(), include_details)
    }

    pub fn info_payload(&self) -> Value {
        actuator_info_payload(self.snapshots.info_snapshot())
    }

    pub fn metrics_index_payload() -> Value {
        actuator_metrics_index_payload()
    }

    pub async fn metric_detail_payload(
        &self,
        metric_name: &str,
        tag_filters: &HashMap<String, String>,
    ) -> Result<Option<Value>, String> {
        let probes = self.snapshots.metric_probe_snapshot();
        ActuatorMetricService::new(self.metrics)
            .metric_detail_payload(metric_name, &probes, tag_filters)
            .await
    }
}

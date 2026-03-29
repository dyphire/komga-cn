use super::TaskQueueRecord;

pub(super) fn task_target(task: &TaskQueueRecord) -> Option<&str> {
    task.id.split_once(':').map(|(_, value)| value)
}

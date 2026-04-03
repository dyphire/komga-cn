use super::TaskQueueRecord;

pub(super) fn task_target(task: &TaskQueueRecord) -> Option<&str> {
    task.id
        .strip_prefix(task.simple_type.as_str())
        .and_then(|suffix| {
            suffix
                .strip_prefix(':')
                .or_else(|| suffix.strip_prefix('_'))
        })
}

use crate::common_ids::TaskId;

use super::TaskCommand;

pub trait TaskWritePort {
    fn enqueue(&self, task: &TaskCommand) -> Result<(), String>;
    fn mark_running(&self, task_id: &TaskId) -> Result<(), String>;
    fn mark_succeeded(&self, task_id: &TaskId) -> Result<(), String>;
    fn mark_failed(&self, task_id: &TaskId, reason: &str) -> Result<(), String>;
}

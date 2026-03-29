use crate::common_ids::TaskId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskPayload {
    pub kind: String,
    pub payload: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCommand {
    pub id: TaskId,
    pub payload: TaskPayload,
    pub state: TaskState,
}

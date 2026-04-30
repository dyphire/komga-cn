use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryScanInterval {
    Disabled,
    Hourly,
    Every6h,
    Every12h,
    Daily,
    Weekly,
}

impl LibraryScanInterval {
    pub fn duration_seconds(self) -> Option<u64> {
        match self {
            Self::Disabled => None,
            Self::Hourly => Some(60 * 60),
            Self::Every6h => Some(6 * 60 * 60),
            Self::Every12h => Some(12 * 60 * 60),
            Self::Daily => Some(24 * 60 * 60),
            Self::Weekly => Some(7 * 24 * 60 * 60),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueueRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
    pub owner: Option<String>,
    pub order: usize,
}

impl TaskQueueRecord {
    pub fn new(id: impl Into<String>, priority: i32, group: Option<String>) -> Self {
        let id = id.into();
        let simple_type = id
            .split_once(':')
            .map(|(task_type, _)| task_type)
            .unwrap_or(id.as_str())
            .to_string();

        Self {
            id,
            simple_type,
            priority,
            group,
            payload: None,
            owner: None,
            order: 0,
        }
    }

    pub fn with_payload(mut self, payload: impl Into<String>) -> Self {
        self.payload = Some(payload.into());
        self
    }

    pub fn with_simple_type(mut self, simple_type: impl Into<String>) -> Self {
        self.simple_type = simple_type.into();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskProcessingError {
    pub message: String,
}

impl TaskProcessingError {
    pub fn invalid_task(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }

    pub fn unsupported_task(task_type: &str) -> Self {
        Self {
            message: format!("unsupported runtime task type: {task_type}"),
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn is_unsupported_task(&self) -> bool {
        self.message.starts_with("unsupported runtime task type: ")
    }
}

impl std::fmt::Display for TaskProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TaskProcessingError {}

pub trait TaskQueueRepository {
    fn persist_task(&self, task: &TaskQueueRecord);
    fn claim_task(&self, task_id: &str, owner: &str);
    fn delete_task(&self, task_id: &str) -> bool;
    fn disown_task(&self, task_id: &str);
    fn disown_all(&self);
    fn clear_unowned(&self) -> usize;
    fn load_records(&self) -> Vec<TaskQueueRecord>;
}

pub trait TaskQueueAdminPort {
    fn enqueue(&mut self, task: TaskQueueRecord);
    fn take_available(&mut self, owner: &str) -> Option<TaskQueueRecord>;
    fn complete(&mut self, task_id: &str) -> bool;
    fn disown(&mut self, task_id: &str) -> bool;
    fn disown_all(&mut self) -> usize;
    fn clear_unowned(&mut self) -> usize;
    fn count_by_simple_type(&self) -> BTreeMap<String, usize>;
}

pub trait TaskQueueExecutionPort {
    fn execute_claimed_task(&mut self, task: &TaskQueueRecord) -> Result<(), TaskProcessingError>;
}

#[derive(Clone, Debug)]
pub struct TaskQueueOrchestrator {
    consumer_owner: String,
    consumes_queue: bool,
    task_pool_size: usize,
    tasks: Vec<TaskQueueRecord>,
    next_order: usize,
}

impl TaskQueueOrchestrator {
    pub fn new(consumer_owner: impl Into<String>, consumes_queue: bool) -> Self {
        Self {
            consumer_owner: consumer_owner.into(),
            consumes_queue,
            task_pool_size: 1,
            tasks: Vec::new(),
            next_order: 0,
        }
    }

    pub fn set_task_pool_size(&mut self, task_pool_size: usize) {
        self.task_pool_size = task_pool_size.max(1);
    }

    pub fn tasks(&self) -> &[TaskQueueRecord] {
        &self.tasks
    }

    pub fn take_available_batch(&mut self) -> Vec<TaskQueueRecord> {
        if !self.consumes_queue {
            return Vec::new();
        }

        if self.task_pool_size <= 1 {
            return self.take_next().into_iter().collect();
        }

        let mut selected = Vec::new();
        while selected.len() < self.task_pool_size {
            let Some(task) = self.take_next() else {
                break;
            };
            selected.push(task);
        }
        selected
    }

    pub fn process_available<E>(&mut self, executor: &mut E) -> Result<usize, TaskProcessingError>
    where
        E: TaskQueueExecutionPort,
    {
        if !self.consumes_queue {
            return Ok(0);
        }

        let mut processed = 0usize;
        loop {
            let batch = self.take_available_batch();
            if batch.is_empty() {
                return Ok(processed);
            }

            let mut iter = batch.into_iter();
            while let Some(task) = iter.next() {
                match executor.execute_claimed_task(&task) {
                    Ok(()) => {
                        let _ = self.complete(&task.id);
                        processed += 1;
                    }
                    Err(error) => {
                        let _ = self.disown(&task.id);
                        for remaining in iter {
                            let _ = self.disown(&remaining.id);
                        }
                        return Err(error);
                    }
                }
            }
        }
    }

    pub fn claim(&mut self, task_id: &str, owner: &str) -> bool {
        match self.tasks.iter_mut().find(|task| task.id == task_id) {
            Some(task) => {
                task.owner = Some(owner.to_string());
                true
            }
            None => false,
        }
    }
}

impl TaskQueueAdminPort for TaskQueueOrchestrator {
    fn enqueue(&mut self, mut task: TaskQueueRecord) {
        task.order = self.next_order;
        self.next_order += 1;
        self.tasks.push(task);
    }

    fn take_available(&mut self, owner: &str) -> Option<TaskQueueRecord> {
        let mut locked_groups = BTreeSet::new();
        for task in &self.tasks {
            if task.owner.is_some()
                && let Some(group) = &task.group
            {
                locked_groups.insert(group.clone());
            }
        }

        let selected_index = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                task.owner.is_none()
                    && task
                        .group
                        .as_ref()
                        .is_none_or(|group| !locked_groups.contains(group))
            })
            .max_by(|(_, left), (_, right)| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| right.order.cmp(&left.order))
            })
            .map(|(index, _)| index)?;

        let task = self.tasks.get_mut(selected_index)?;
        task.owner = Some(owner.to_string());
        Some(task.clone())
    }

    fn complete(&mut self, task_id: &str) -> bool {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.id != task_id);
        self.tasks.len() != original
    }

    fn disown(&mut self, task_id: &str) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) else {
            return false;
        };
        task.owner = None;
        true
    }

    fn disown_all(&mut self) -> usize {
        let mut disowned = 0;
        for task in &mut self.tasks {
            if task.owner.take().is_some() {
                disowned += 1;
            }
        }
        disowned
    }

    fn clear_unowned(&mut self) -> usize {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.owner.is_some());
        original - self.tasks.len()
    }

    fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for task in &self.tasks {
            *counts.entry(task.simple_type.clone()).or_insert(0) += 1;
        }
        counts
    }
}

impl TaskQueueOrchestrator {
    fn take_next(&mut self) -> Option<TaskQueueRecord> {
        self.take_available(&self.consumer_owner.clone())
    }
}

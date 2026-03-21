use std::collections::BTreeMap;

use crate::config::{RuntimeConfig, WriterDecision, WriterKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueueRecord {
    pub id: String,
    pub priority: i32,
    pub group: Option<String>,
    pub owner: Option<String>,
    order: usize,
}

impl TaskQueueRecord {
    pub fn new(id: impl Into<String>, priority: i32, group: Option<String>) -> Self {
        Self {
            id: id.into(),
            priority,
            group,
            owner: None,
            order: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TaskQueueAdmin {
    tasks: Vec<TaskQueueRecord>,
    next_order: usize,
}

impl TaskQueueAdmin {
    pub fn enqueue(&mut self, mut task: TaskQueueRecord) {
        task.order = self.next_order;
        self.next_order += 1;
        self.tasks.push(task);
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

    pub fn complete(&mut self, task_id: &str) -> bool {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.id != task_id);
        self.tasks.len() != original
    }

    pub fn clear_unowned(&mut self) -> usize {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.owner.is_some());
        original - self.tasks.len()
    }

    pub fn read_grouped_by_owner(&self) -> BTreeMap<Option<String>, Vec<TaskQueueRecord>> {
        let mut grouped: BTreeMap<Option<String>, Vec<TaskQueueRecord>> = BTreeMap::new();
        for task in &self.tasks {
            grouped
                .entry(task.owner.clone())
                .or_default()
                .push(task.clone());
        }
        grouped
    }

    fn take_available(&mut self, owner: &str) -> Option<TaskQueueRecord> {
        let mut locked_groups = std::collections::BTreeSet::new();
        for task in &self.tasks {
            if task.owner.is_some() {
                if let Some(group) = &task.group {
                    locked_groups.insert(group.clone());
                }
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
}

#[derive(Clone, Debug)]
pub struct TaskQueueScheduler {
    admin: TaskQueueAdmin,
    consumer_owner: String,
    consumes_queue: bool,
}

impl TaskQueueScheduler {
    pub fn for_runtime(config: RuntimeConfig, consumer_owner: impl Into<String>) -> Self {
        let consumes_queue = matches!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        );

        Self {
            admin: TaskQueueAdmin::default(),
            consumer_owner: consumer_owner.into(),
            consumes_queue,
        }
    }

    pub fn enqueue(&mut self, task: TaskQueueRecord) {
        self.admin.enqueue(task);
    }

    pub fn take_next(&mut self) -> Option<TaskQueueRecord> {
        if !self.consumes_queue {
            return None;
        }

        self.admin.take_available(&self.consumer_owner)
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        self.admin.complete(task_id)
    }

    pub fn admin(&self) -> &TaskQueueAdmin {
        &self.admin
    }

    pub fn admin_mut(&mut self) -> &mut TaskQueueAdmin {
        &mut self.admin
    }
}

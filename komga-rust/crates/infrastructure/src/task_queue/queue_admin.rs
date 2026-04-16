use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueueRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
    pub owner: Option<String>,
    pub(super) order: usize,
}

impl TaskQueueRecord {
    pub fn new(id: impl Into<String>, priority: i32, group: Option<String>) -> Self {
        let id = id.into();
        Self {
            simple_type: id
                .split_once(':')
                .map(|(task_type, _)| task_type)
                .unwrap_or(id.as_str())
                .to_string(),
            id,
            priority,
            group,
            payload: None,
            owner: None,
            order: 0,
        }
    }

    pub fn with_simple_type(mut self, simple_type: impl Into<String>) -> Self {
        self.simple_type = simple_type.into();
        self
    }

    pub fn with_payload(mut self, payload: impl Into<String>) -> Self {
        self.payload = Some(payload.into());
        self
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

    pub fn disown_all(&mut self) -> usize {
        let mut disowned = 0;
        for task in &mut self.tasks {
            if task.owner.take().is_some() {
                disowned += 1;
            }
        }
        disowned
    }

    pub fn disown(&mut self, task_id: &str) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) else {
            return false;
        };
        task.owner = None;
        true
    }

    pub fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for task in &self.tasks {
            *counts.entry(task.simple_type.clone()).or_insert(0) += 1;
        }
        counts
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

    pub(super) fn take_available(&mut self, owner: &str) -> Option<TaskQueueRecord> {
        let mut locked_groups = BTreeSet::new();
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

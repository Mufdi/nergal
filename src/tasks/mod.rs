pub mod transcript_parser;

/// Status of a task in the task list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Deleted,
}

/// A single task tracked by Claude Code's TaskCreate/TaskUpdate tools.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub active_form: Option<String>,
    pub blocked_by: Vec<String>,
}

/// In-memory store that accumulates task events.
#[derive(Debug, Default)]
pub struct TaskStore {
    tasks: Vec<Task>,
    next_id: u32,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    /// Apply a TaskCreate tool_input. Returns the assigned ID on success.
    pub fn apply_create(&mut self, input: &serde_json::Value) -> Option<String> {
        let subject = input.get("subject")?.as_str()?.to_string();
        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let active_form = input
            .get("activeForm")
            .and_then(|v| v.as_str())
            .map(String::from);

        let id = self.next_id.to_string();
        self.next_id += 1;

        self.tasks.push(Task {
            id: id.clone(),
            subject,
            description,
            status: TaskStatus::Pending,
            active_form,
            blocked_by: Vec::new(),
        });

        Some(id)
    }

    /// Apply a TaskUpdate tool_input. Updates status/subject/description by taskId.
    pub fn apply_update(&mut self, input: &serde_json::Value) {
        let Some(task_id) = input.get("taskId").and_then(|v| v.as_str()) else {
            return;
        };

        let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) else {
            return;
        };

        if let Some(status_str) = input.get("status").and_then(|v| v.as_str()) {
            task.status = match status_str {
                "in_progress" => TaskStatus::InProgress,
                "completed" => TaskStatus::Completed,
                "pending" => TaskStatus::Pending,
                "deleted" => TaskStatus::Deleted,
                _ => task.status.clone(),
            };
        }

        if let Some(subject) = input.get("subject").and_then(|v| v.as_str()) {
            task.subject = subject.to_string();
        }

        if let Some(description) = input.get("description").and_then(|v| v.as_str()) {
            task.description = description.to_string();
        }

        if let Some(active_form) = input.get("activeForm").and_then(|v| v.as_str()) {
            task.active_form = Some(active_form.to_string());
        }

        if let Some(blocked_by) = input.get("addBlockedBy").and_then(|v| v.as_array()) {
            for id in blocked_by {
                if let Some(id_str) = id.as_str()
                    && !task.blocked_by.contains(&id_str.to_string())
                {
                    task.blocked_by.push(id_str.to_string());
                }
            }
        }
    }

    /// Replace all tasks (used by transcript full re-parse).
    pub fn replace_all(&mut self, other: TaskStore) {
        self.tasks = other.tasks;
        self.next_id = other.next_id;
    }

    pub fn visible_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Deleted)
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn visible_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Deleted)
            .count()
    }
}

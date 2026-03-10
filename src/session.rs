use gpui::*;

use crate::claude::transcript::TranscriptWatcher;
use crate::ui::activity_log::ActivityLog;
use crate::ui::plan_panel::PlanPanel;
use crate::ui::task_panel::TaskPanel;
use crate::ui::terminal_panel::TerminalPanel;

/// Encapsulates all per-session state: terminal, plan, tasks, activity log, and transcript watcher.
pub struct Session {
    pub id: Option<String>,
    pub label: String,
    pub terminal: Entity<TerminalPanel>,
    pub plan: Entity<PlanPanel>,
    pub tasks: Entity<TaskPanel>,
    pub activity_log: Entity<ActivityLog>,
    pub transcript_watcher: Option<TranscriptWatcher>,
}

impl Session {
    pub fn new(
        label: String,
        plans_dir: std::path::PathBuf,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let terminal = cx.new(|cx| TerminalPanel::new(window, cx));
        let plan = cx.new(|cx| PlanPanel::new(plans_dir, cx));
        let tasks = cx.new(TaskPanel::new);
        let activity_log = cx.new(ActivityLog::new);

        Self {
            id: None,
            label,
            terminal,
            plan,
            tasks,
            activity_log,
            transcript_watcher: None,
        }
    }

    /// Focus the terminal panel.
    pub fn focus_terminal(&self, window: &mut Window, cx: &mut App) {
        self.terminal.update(cx, |t, _| t.focus(window));
    }

    /// Focus the plan panel.
    pub fn focus_plan(&self, window: &mut Window, cx: &mut App) {
        self.plan.update(cx, |p, _| p.focus(window));
    }

    /// Focus the tasks panel.
    pub fn focus_tasks(&self, window: &mut Window, cx: &mut App) {
        self.tasks.update(cx, |t, _| t.focus(window));
    }

    /// Focus the activity log panel.
    pub fn focus_activity(&self, window: &mut Window, cx: &mut App) {
        self.activity_log.update(cx, |a, _| a.focus(window));
    }
}

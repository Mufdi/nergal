use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::claude::plan::PlanManager;

/// Thread-safe plan state managed as Tauri state.
pub type SharedPlanState = Arc<Mutex<PlanStateManager>>;

/// Manages in-memory plan editing state per session.
/// Plans on disk (.md files) are the source of truth — this is just for active editing.
pub struct PlanStateManager {
    sessions: HashMap<String, PlanManager>,
    plans_dir: PathBuf,
}

impl PlanStateManager {
    pub fn new(plans_dir: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            plans_dir,
        }
    }

    pub fn get_or_create(&mut self, session_id: &str) -> &mut PlanManager {
        self.sessions
            .entry(session_id.to_string())
            .or_insert_with(|| PlanManager::new(self.plans_dir.clone()))
    }

    pub fn plans_dir(&self) -> &std::path::Path {
        &self.plans_dir
    }
}

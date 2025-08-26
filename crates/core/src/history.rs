use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoryAction {
    Set {
        name: String,
        old_value: Option<String>,
        new_value: String,
    },
    Delete {
        name: String,
        old_value: String,
    },
    BatchUpdate {
        changes: Vec<(String, Option<String>, String)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: uuid::Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: HistoryAction,
}

impl HistoryEntry {
    #[must_use]
    pub fn new(action: HistoryAction) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            action,
        }
    }
}

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

impl History {
    #[must_use]
    pub const fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn add(&mut self, entry: HistoryEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    #[must_use]
    pub fn recent(&self, count: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(count).collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

use crate::snapshot::Snapshot;
use crate::{EnvVar, EnvVarManager};
use color_eyre::Result;
use color_eyre::eyre::eyre;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct SnapshotManager {
    storage_dir: PathBuf,
}

impl SnapshotManager {
    /// Creates a new `SnapshotManager`.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The system data/config directory cannot be found
    /// - The snapshots directory cannot be created due to filesystem errors
    pub fn new() -> Result<Self> {
        let storage_dir = if cfg!(windows) {
            dirs::data_dir()
                .ok_or_else(|| eyre!("Could not find data directory"))?
                .join("envx")
                .join("snapshots")
        } else {
            dirs::config_dir()
                .ok_or_else(|| eyre!("Could not find config directory"))?
                .join("envx")
                .join("snapshots")
        };

        fs::create_dir_all(&storage_dir)?;
        Ok(Self { storage_dir })
    }

    /// Creates a new snapshot with the given name, description, and environment variables.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - There are file system errors when writing the snapshot file to disk
    /// - JSON serialization of the snapshot fails
    pub fn create(&self, name: String, description: Option<String>, vars: Vec<EnvVar>) -> Result<Snapshot> {
        let snapshot = Snapshot::from_vars(name, description, vars);
        self.save_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    /// Lists all snapshots sorted by creation date (newest first).
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - There are file system errors when reading the snapshots directory
    /// - There are file system errors when reading individual snapshot files
    pub fn list(&self) -> Result<Vec<Snapshot>> {
        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&self.storage_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(entry.path())?;
                if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) {
                    snapshots.push(snapshot);
                }
            }
        }

        // Sort by creation date (newest first)
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(snapshots)
    }

    /// Gets a snapshot by ID or name.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The snapshot cannot be found by ID or name
    /// - There are file system errors when reading the snapshot file
    /// - JSON deserialization fails for the snapshot file
    pub fn get(&self, id_or_name: &str) -> Result<Snapshot> {
        // Try by ID first
        let id_path = self.storage_dir.join(format!("{id_or_name}.json"));
        if id_path.exists() {
            let content = fs::read_to_string(&id_path)?;
            return Ok(serde_json::from_str(&content)?);
        }

        // Try by name
        for snapshot in self.list()? {
            if snapshot.name == id_or_name {
                return Ok(snapshot);
            }
        }

        Err(eyre!("Snapshot not found: {}", id_or_name))
    }

    /// Deletes a snapshot by ID or name.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The snapshot cannot be found by ID or name
    /// - There are file system errors when deleting the snapshot file
    pub fn delete(&self, id_or_name: &str) -> Result<()> {
        let snapshot = self.get(id_or_name)?;
        let path = self.storage_dir.join(format!("{}.json", snapshot.id));
        fs::remove_file(path)?;
        Ok(())
    }

    /// Restores environment variables from a snapshot by clearing current variables and applying snapshot values.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The snapshot cannot be found by ID or name
    /// - There are file system errors when reading the snapshot file
    /// - JSON deserialization fails for the snapshot file
    /// - Setting environment variables in the manager fails
    pub fn restore(&self, id_or_name: &str, manager: &mut EnvVarManager) -> Result<()> {
        let snapshot = self.get(id_or_name)?;

        // Clear current variables
        manager.clear();

        // Restore from snapshot
        for (_, var) in snapshot.variables {
            manager.set(&var.name, &var.value, true)?;
        }

        Ok(())
    }

    /// Compares two snapshots and returns the differences between them.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Either snapshot cannot be found by ID or name
    /// - There are file system errors when reading snapshot files
    /// - JSON deserialization fails for the snapshot files
    pub fn diff(&self, snapshot1: &str, snapshot2: &str) -> Result<SnapshotDiff> {
        let snap1 = self.get(snapshot1)?;
        let snap2 = self.get(snapshot2)?;

        let mut diff = SnapshotDiff::default();

        // Find added and modified
        for (name, var2) in &snap2.variables {
            match snap1.variables.get(name) {
                Some(var1) => {
                    if var1.value != var2.value {
                        diff.modified.insert(name.clone(), (var1.clone(), var2.clone()));
                    }
                }
                None => {
                    diff.added.insert(name.clone(), var2.clone());
                }
            }
        }

        // Find removed
        for (name, var1) in &snap1.variables {
            if !snap2.variables.contains_key(name) {
                diff.removed.insert(name.clone(), var1.clone());
            }
        }

        Ok(diff)
    }

    fn save_snapshot(&self, snapshot: &Snapshot) -> color_eyre::Result<()> {
        let path = self.storage_dir.join(format!("{}.json", snapshot.id));
        let content = serde_json::to_string_pretty(snapshot)?;
        fs::write(path, content)?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct SnapshotDiff {
    pub added: HashMap<String, EnvVar>,
    pub removed: HashMap<String, EnvVar>,
    pub modified: HashMap<String, (EnvVar, EnvVar)>, // (old, new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvVar, EnvVarSource};
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_snapshot_manager() -> (SnapshotManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().join("snapshots");
        fs::create_dir_all(&storage_dir).unwrap();

        let manager = SnapshotManager { storage_dir };
        (manager, temp_dir)
    }

    fn create_test_env_var(name: &str, value: &str) -> EnvVar {
        EnvVar {
            name: name.to_string(),
            value: value.to_string(),
            source: EnvVarSource::User,
            modified: Utc::now(),
            original_value: None,
        }
    }

    fn create_test_env_manager() -> EnvVarManager {
        let mut manager = EnvVarManager::new();
        manager.set("VAR1", "value1", false).unwrap();
        manager.set("VAR2", "value2", false).unwrap();
        manager.set("VAR3", "value3", false).unwrap();
        manager
    }

    #[test]
    fn test_snapshot_manager_new() {
        // Test with temporary directory to avoid system dependencies
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().join("envx").join("snapshots");

        // Manually create the manager with test directory
        let manager = SnapshotManager {
            storage_dir: storage_dir.clone(),
        };

        // Verify storage directory is set correctly
        assert_eq!(manager.storage_dir, storage_dir);
    }

    #[test]
    fn test_create_snapshot() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![
            create_test_env_var("TEST_VAR1", "test_value1"),
            create_test_env_var("TEST_VAR2", "test_value2"),
        ];

        let result = manager.create("test-snapshot".to_string(), Some("Test description".to_string()), vars);

        assert!(result.is_ok());
        let snapshot = result.unwrap();

        assert_eq!(snapshot.name, "test-snapshot");
        assert_eq!(snapshot.description, Some("Test description".to_string()));
        assert_eq!(snapshot.variables.len(), 2);
        assert!(snapshot.variables.contains_key("TEST_VAR1"));
        assert!(snapshot.variables.contains_key("TEST_VAR2"));

        // Verify snapshot was saved to disk
        let snapshot_path = manager.storage_dir.join(format!("{}.json", snapshot.id));
        assert!(snapshot_path.exists());
    }

    #[test]
    fn test_create_snapshot_without_description() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("TEST_VAR", "test_value")];
        let result = manager.create("no-desc".to_string(), None, vars);

        assert!(result.is_ok());
        assert!(result.unwrap().description.is_none());
    }

    #[test]
    fn test_list_snapshots_empty() {
        let (manager, _temp) = create_test_snapshot_manager();

        let result = manager.list();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_snapshots_multiple() {
        let (manager, _temp) = create_test_snapshot_manager();

        // Create multiple snapshots
        let vars = vec![create_test_env_var("VAR", "value")];
        manager.create("snap1".to_string(), None, vars.clone()).unwrap();

        // Add a small delay to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));

        manager.create("snap2".to_string(), None, vars.clone()).unwrap();
        manager.create("snap3".to_string(), None, vars).unwrap();

        let snapshots = manager.list().unwrap();
        assert_eq!(snapshots.len(), 3);

        // Verify they are sorted by creation date (newest first)
        assert_eq!(snapshots[0].name, "snap3");
        assert_eq!(snapshots[1].name, "snap2");
        assert_eq!(snapshots[2].name, "snap1");
    }

    #[test]
    fn test_list_snapshots_handles_invalid_files() {
        let (manager, _temp) = create_test_snapshot_manager();

        // Create a valid snapshot
        let vars = vec![create_test_env_var("VAR", "value")];
        manager.create("valid".to_string(), None, vars).unwrap();

        // Create an invalid JSON file
        let invalid_path = manager.storage_dir.join("invalid.json");
        fs::write(invalid_path, "{ invalid json }").unwrap();

        // Create a non-JSON file
        let non_json_path = manager.storage_dir.join("not-json.txt");
        fs::write(non_json_path, "some content").unwrap();

        // List should only return valid snapshots
        let snapshots = manager.list().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "valid");
    }

    #[test]
    fn test_get_snapshot_by_id() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("VAR", "value")];
        let created = manager.create("test".to_string(), None, vars).unwrap();

        let retrieved = manager.get(&created.id).unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, created.name);
    }

    #[test]
    fn test_get_snapshot_by_name() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("VAR", "value")];
        manager.create("test-name".to_string(), None, vars).unwrap();

        let retrieved = manager.get("test-name").unwrap();
        assert_eq!(retrieved.name, "test-name");
    }

    #[test]
    fn test_get_snapshot_not_found() {
        let (manager, _temp) = create_test_snapshot_manager();

        let result = manager.get("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Snapshot not found"));
    }

    #[test]
    fn test_get_snapshot_prefers_id_over_name() {
        let (manager, _temp) = create_test_snapshot_manager();

        // Create two snapshots where one's name matches another's ID
        let vars = vec![create_test_env_var("VAR", "value")];
        let snap1 = manager.create("first".to_string(), None, vars.clone()).unwrap();

        // Create second snapshot with name equal to first snapshot's ID
        manager.create(snap1.id.clone(), None, vars).unwrap();

        // Getting by snap1.id should return snap1, not the one named with snap1.id
        let retrieved = manager.get(&snap1.id).unwrap();
        assert_eq!(retrieved.name, "first");
    }

    #[test]
    fn test_delete_snapshot() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("VAR", "value")];
        let snapshot = manager.create("to-delete".to_string(), None, vars).unwrap();

        // Verify it exists
        assert!(manager.get(&snapshot.id).is_ok());

        // Delete it
        let result = manager.delete(&snapshot.id);
        assert!(result.is_ok());

        // Verify it's gone
        assert!(manager.get(&snapshot.id).is_err());

        // Verify file is deleted
        let snapshot_path = manager.storage_dir.join(format!("{}.json", snapshot.id));
        assert!(!snapshot_path.exists());
    }

    #[test]
    fn test_delete_snapshot_by_name() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("VAR", "value")];
        manager.create("delete-by-name".to_string(), None, vars).unwrap();

        let result = manager.delete("delete-by-name");
        assert!(result.is_ok());
        assert!(manager.get("delete-by-name").is_err());
    }

    #[test]
    fn test_delete_nonexistent_snapshot() {
        let (manager, _temp) = create_test_snapshot_manager();

        let result = manager.delete("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_restore_snapshot() {
        let (manager, _temp) = create_test_snapshot_manager();
        let mut env_manager = create_test_env_manager();

        // Create snapshot
        let vars = vec![
            create_test_env_var("NEW_VAR1", "new_value1"),
            create_test_env_var("NEW_VAR2", "new_value2"),
        ];
        let snapshot = manager.create("to-restore".to_string(), None, vars).unwrap();

        // Restore it
        let result = manager.restore(&snapshot.id, &mut env_manager);
        assert!(result.is_ok());

        // Verify old variables are cleared and new ones are set
        assert!(env_manager.get("VAR1").is_none());
        assert!(env_manager.get("VAR2").is_none());
        assert!(env_manager.get("VAR3").is_none());

        assert_eq!(env_manager.get("NEW_VAR1").unwrap().value, "new_value1");
        assert_eq!(env_manager.get("NEW_VAR2").unwrap().value, "new_value2");
    }

    #[test]
    fn test_restore_nonexistent_snapshot() {
        let (manager, _temp) = create_test_snapshot_manager();
        let mut env_manager = create_test_env_manager();

        let result = manager.restore("nonexistent", &mut env_manager);
        assert!(result.is_err());
    }

    #[test]
    fn test_diff_snapshots_no_changes() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![
            create_test_env_var("VAR1", "value1"),
            create_test_env_var("VAR2", "value2"),
        ];

        let snap1 = manager.create("snap1".to_string(), None, vars.clone()).unwrap();
        let snap2 = manager.create("snap2".to_string(), None, vars).unwrap();

        let diff = manager.diff(&snap1.id, &snap2.id).unwrap();
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn test_diff_snapshots_with_changes() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars1 = vec![
            create_test_env_var("VAR1", "value1"),
            create_test_env_var("VAR2", "old_value"),
            create_test_env_var("VAR3", "value3"),
        ];

        let vars2 = vec![
            create_test_env_var("VAR1", "value1"),    // Same
            create_test_env_var("VAR2", "new_value"), // Modified
            create_test_env_var("VAR4", "value4"),    // Added
        ];

        let snap1 = manager.create("snap1".to_string(), None, vars1).unwrap();
        let snap2 = manager.create("snap2".to_string(), None, vars2).unwrap();

        let diff = manager.diff(&snap1.id, &snap2.id).unwrap();

        // Check added
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.contains_key("VAR4"));
        assert_eq!(diff.added.get("VAR4").unwrap().value, "value4");

        // Check removed
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains_key("VAR3"));
        assert_eq!(diff.removed.get("VAR3").unwrap().value, "value3");

        // Check modified
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified.contains_key("VAR2"));
        let (old, new) = diff.modified.get("VAR2").unwrap();
        assert_eq!(old.value, "old_value");
        assert_eq!(new.value, "new_value");
    }

    #[test]
    fn test_diff_nonexistent_snapshots() {
        let (manager, _temp) = create_test_snapshot_manager();

        let result = manager.diff("nonexistent1", "nonexistent2");
        assert!(result.is_err());
    }

    #[test]
    fn test_save_snapshot_creates_pretty_json() {
        let (manager, _temp) = create_test_snapshot_manager();

        let vars = vec![create_test_env_var("TEST_VAR", "test_value")];
        let snapshot = manager
            .create("pretty-test".to_string(), Some("Pretty JSON test".to_string()), vars)
            .unwrap();

        // Read the saved file
        let snapshot_path = manager.storage_dir.join(format!("{}.json", snapshot.id));
        let content = fs::read_to_string(snapshot_path).unwrap();

        // Verify it's pretty-printed (contains indentation)
        assert!(content.contains("\n  "));
        assert!(content.contains("\"name\": \"pretty-test\""));
        assert!(content.contains("\"description\": \"Pretty JSON test\""));
    }

    #[test]
    fn test_concurrent_operations() {
        let (manager, _temp) = create_test_snapshot_manager();

        // Create multiple snapshots in quick succession
        let mut snapshot_ids = Vec::new();
        for i in 0..5 {
            let vars = vec![create_test_env_var(&format!("VAR{i}"), &format!("value{i}"))];
            let snapshot = manager.create(format!("concurrent-{i}"), None, vars).unwrap();
            snapshot_ids.push(snapshot.id);
        }

        // Verify all can be retrieved
        for id in &snapshot_ids {
            assert!(manager.get(id).is_ok());
        }

        // Verify list returns all
        let snapshots = manager.list().unwrap();
        assert_eq!(snapshots.len(), 5);
    }
}

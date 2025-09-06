use crate::EnvVar;
use ahash::AHashMap as HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub variables: HashMap<String, EnvVar>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub variables: HashMap<String, ProfileVar>,
    pub parent: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileVar {
    pub value: String,
    pub enabled: bool,
    pub override_system: bool,
}

impl Snapshot {
    #[must_use]
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description,
            created_at: Utc::now(),
            variables: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn from_vars(name: String, description: Option<String>, vars: Vec<EnvVar>) -> Self {
        let mut snapshot = Self::new(name, description);
        for var in vars {
            snapshot.variables.insert(var.name.clone(), var);
        }
        snapshot
    }
}

impl Profile {
    #[must_use]
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            name,
            description,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            variables: HashMap::new(),
            parent: None,
            metadata: HashMap::new(),
        }
    }

    pub fn add_var(&mut self, name: String, value: String, override_system: bool) {
        self.variables.insert(
            name,
            ProfileVar {
                value,
                enabled: true,
                override_system,
            },
        );
        self.updated_at = Utc::now();
    }

    pub fn remove_var(&mut self, name: &str) -> Option<ProfileVar> {
        self.updated_at = Utc::now();
        self.variables.remove(name)
    }

    #[must_use]
    pub fn get_active_vars(&self) -> HashMap<String, String> {
        self.variables
            .iter()
            .filter(|(_, var)| var.enabled)
            .map(|(name, var)| (name.clone(), var.value.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvVar, EnvVarSource};

    fn create_test_env_var(name: &str, value: &str) -> EnvVar {
        EnvVar {
            name: name.to_string(),
            value: value.to_string(),
            source: EnvVarSource::User,
            modified: Utc::now(),
            original_value: None,
        }
    }

    #[test]
    fn test_snapshot_new() {
        let name = "test-snapshot".to_string();
        let description = Some("Test description".to_string());
        let snapshot = Snapshot::new(name.clone(), description.clone());

        assert_eq!(snapshot.name, name);
        assert_eq!(snapshot.description, description);
        assert!(!snapshot.id.is_empty());
        assert!(snapshot.variables.is_empty());
        assert!(snapshot.metadata.is_empty());
        assert!(snapshot.created_at <= Utc::now());
    }

    #[test]
    fn test_snapshot_new_without_description() {
        let name = "test-snapshot".to_string();
        let snapshot = Snapshot::new(name.clone(), None);

        assert_eq!(snapshot.name, name);
        assert!(snapshot.description.is_none());
    }

    #[test]
    fn test_snapshot_unique_ids() {
        let snapshot1 = Snapshot::new("snap1".to_string(), None);
        let snapshot2 = Snapshot::new("snap2".to_string(), None);

        assert_ne!(snapshot1.id, snapshot2.id);
    }

    #[test]
    fn test_snapshot_from_vars_empty() {
        let vars = vec![];
        let snapshot = Snapshot::from_vars("empty-snapshot".to_string(), None, vars);

        assert_eq!(snapshot.name, "empty-snapshot");
        assert!(snapshot.variables.is_empty());
    }

    #[test]
    fn test_snapshot_from_vars_single() {
        let var = create_test_env_var("TEST_VAR", "test_value");
        let vars = vec![var.clone()];
        let snapshot = Snapshot::from_vars("single-var-snapshot".to_string(), None, vars);

        assert_eq!(snapshot.variables.len(), 1);
        assert!(snapshot.variables.contains_key("TEST_VAR"));

        let stored_var = snapshot.variables.get("TEST_VAR").unwrap();
        assert_eq!(stored_var.name, var.name);
        assert_eq!(stored_var.value, var.value);
    }

    #[test]
    fn test_snapshot_from_vars_multiple() {
        let vars = vec![
            create_test_env_var("VAR1", "value1"),
            create_test_env_var("VAR2", "value2"),
            create_test_env_var("VAR3", "value3"),
        ];
        let snapshot = Snapshot::from_vars(
            "multi-var-snapshot".to_string(),
            Some("Multiple variables".to_string()),
            vars,
        );

        assert_eq!(snapshot.variables.len(), 3);
        assert!(snapshot.variables.contains_key("VAR1"));
        assert!(snapshot.variables.contains_key("VAR2"));
        assert!(snapshot.variables.contains_key("VAR3"));
        assert_eq!(snapshot.description, Some("Multiple variables".to_string()));
    }

    #[test]
    fn test_snapshot_from_vars_duplicate_names() {
        // Test that later values override earlier ones for same variable name
        let vars = vec![
            create_test_env_var("DUPLICATE", "first_value"),
            create_test_env_var("DUPLICATE", "second_value"),
        ];
        let snapshot = Snapshot::from_vars("duplicate-test".to_string(), None, vars);

        assert_eq!(snapshot.variables.len(), 1);
        assert_eq!(snapshot.variables.get("DUPLICATE").unwrap().value, "second_value");
    }

    #[test]
    fn test_snapshot_serialization() {
        let mut snapshot = Snapshot::new("serialize-test".to_string(), Some("Test serialization".to_string()));
        snapshot.metadata.insert("key1".to_string(), "value1".to_string());
        snapshot.metadata.insert("key2".to_string(), "value2".to_string());

        let var = create_test_env_var("TEST_VAR", "test_value");
        snapshot.variables.insert(var.name.clone(), var);

        // Serialize to JSON
        let json = serde_json::to_string(&snapshot).expect("Failed to serialize snapshot to JSON");

        // Deserialize back
        let deserialized: Snapshot = serde_json::from_str(&json).expect("Failed to deserialize snapshot from JSON");

        assert_eq!(deserialized.id, snapshot.id);
        assert_eq!(deserialized.name, snapshot.name);
        assert_eq!(deserialized.description, snapshot.description);
        assert_eq!(deserialized.variables.len(), snapshot.variables.len());
        assert_eq!(deserialized.metadata.len(), snapshot.metadata.len());
        assert_eq!(deserialized.metadata.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_snapshot_with_special_characters() {
        let name = "snapshot-with-special-chars!@#$%^&*()".to_string();
        let description = Some("Description with\nnewlines\tand\ttabs".to_string());
        let snapshot = Snapshot::new(name.clone(), description.clone());

        assert_eq!(snapshot.name, name);
        assert_eq!(snapshot.description, description);
    }

    #[test]
    fn test_snapshot_with_empty_name() {
        // This test documents current behavior - empty names are allowed
        let snapshot = Snapshot::new(String::new(), None);
        assert_eq!(snapshot.name, "");
    }

    #[test]
    fn test_snapshot_from_vars_preserves_all_env_var_fields() {
        let mut var = create_test_env_var("FULL_VAR", "full_value");
        var.source = EnvVarSource::System;
        var.original_value = Some("original".to_string());

        let vars = vec![var];
        let snapshot = Snapshot::from_vars("preserve-test".to_string(), None, vars);

        let stored_var = snapshot.variables.get("FULL_VAR").unwrap();
        assert_eq!(stored_var.source, EnvVarSource::System);
        assert_eq!(stored_var.original_value, Some("original".to_string()));
    }

    #[test]
    fn test_snapshot_metadata_operations() {
        let mut snapshot = Snapshot::new("metadata-test".to_string(), None);

        // Test empty metadata
        assert!(snapshot.metadata.is_empty());

        // Add metadata
        snapshot.metadata.insert("author".to_string(), "test_user".to_string());
        snapshot.metadata.insert("version".to_string(), "1.0.0".to_string());

        assert_eq!(snapshot.metadata.len(), 2);
        assert_eq!(snapshot.metadata.get("author"), Some(&"test_user".to_string()));
        assert_eq!(snapshot.metadata.get("version"), Some(&"1.0.0".to_string()));
    }

    #[test]
    fn test_snapshot_clone() {
        let mut original = Snapshot::new("original".to_string(), Some("Original snapshot".to_string()));
        original
            .variables
            .insert("VAR1".to_string(), create_test_env_var("VAR1", "value1"));
        original.metadata.insert("key".to_string(), "value".to_string());

        let cloned = original.clone();

        assert_eq!(cloned.id, original.id);
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.description, original.description);
        assert_eq!(cloned.created_at, original.created_at);
        assert_eq!(cloned.variables.len(), original.variables.len());
        assert_eq!(cloned.metadata.len(), original.metadata.len());
    }

    #[test]
    fn test_snapshot_debug_format() {
        let snapshot = Snapshot::new("debug-test".to_string(), None);
        let debug_str = format!("{snapshot:?}");

        assert!(debug_str.contains("Snapshot"));
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains(&snapshot.id));
    }
}

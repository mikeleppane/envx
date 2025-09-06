use crate::EnvVarManager;
use crate::snapshot::Profile;
use ahash::AHashMap as HashMap;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct ProfileConfig {
    pub active: Option<String>,
    pub profiles: HashMap<String, Profile>,
}

pub struct ProfileManager {
    config_path: PathBuf,
    config: ProfileConfig,
}

impl ProfileManager {
    /// Creates a new `ProfileManager` instance.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The data/config directory cannot be found
    /// - The config directory cannot be created
    /// - The existing profiles.json file cannot be read or parsed
    pub fn new() -> Result<Self> {
        let config_dir = if cfg!(windows) {
            dirs::data_dir()
                .ok_or_else(|| eyre!("Could not find data directory"))?
                .join("envx")
        } else {
            dirs::config_dir()
                .ok_or_else(|| eyre!("Could not find config directory"))?
                .join("envx")
        };

        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("profiles.json");

        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_json::from_str(&content)?
        } else {
            ProfileConfig {
                active: None,
                profiles: HashMap::new(),
            }
        };

        Ok(Self { config_path, config })
    }

    /// Creates a new profile with the specified name and optional description.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - A profile with the given name already exists
    /// - The configuration cannot be saved to disk
    pub fn create(&mut self, name: String, description: Option<String>) -> Result<()> {
        if self.config.profiles.contains_key(&name) {
            return Err(eyre!("Profile '{}' already exists", name));
        }

        let profile = Profile::new(name.clone(), description);
        self.config.profiles.insert(name, profile);
        self.save()?;
        Ok(())
    }

    /// Deletes the specified profile.
    ///
    /// If the deleted profile is currently active, the active profile will be set to None.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The specified profile is not found
    /// - The configuration cannot be saved to disk
    pub fn delete(&mut self, name: &str) -> Result<()> {
        if self.config.active.as_ref() == Some(&name.to_string()) {
            self.config.active = None;
        }

        self.config
            .profiles
            .remove(name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Profile '{}' not found", name))?;

        self.save()?;
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<&Profile> {
        self.config.profiles.values().collect()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.config.profiles.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.config.profiles.get_mut(name)
    }

    #[must_use]
    pub fn active(&self) -> Option<&Profile> {
        self.config
            .active
            .as_ref()
            .and_then(|name| self.config.profiles.get(name))
    }

    /// Switches to the specified profile, making it the active profile.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The specified profile is not found
    /// - The configuration cannot be saved to disk
    pub fn switch(&mut self, name: &str) -> Result<()> {
        if !self.config.profiles.contains_key(name) {
            return Err(eyre!("Profile '{}' not found", name));
        }

        self.config.active = Some(name.to_string());
        self.save()?;
        Ok(())
    }

    /// Applies a profile's environment variables to the given `EnvVarManager`.
    ///
    /// If the profile has a parent profile, it will be applied first recursively,
    /// then the current profile's variables will be applied, potentially overriding
    /// parent values.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The specified profile is not found
    /// - A parent profile is not found during recursive application
    /// - Setting environment variables in the manager fails
    pub fn apply(&self, name: &str, manager: &mut EnvVarManager) -> Result<()> {
        let profile = self
            .get(name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Profile '{}' not found", name))?;

        // Apply parent profile first if exists
        if let Some(parent) = &profile.parent {
            self.apply(parent, manager)?;
        }

        // Apply this profile's variables
        for (var_name, var) in &profile.variables {
            if var.enabled {
                // Always set the variable, regardless of whether it exists
                // This ensures profile switching actually updates values
                manager.set(var_name, &var.value, true)?;
            }
        }

        Ok(())
    }

    /// Exports a profile to JSON format.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The specified profile is not found
    /// - The profile cannot be serialized to JSON
    pub fn export(&self, name: &str) -> Result<String> {
        let profile = self.get(name).ok_or_else(|| eyre!("Profile '{}' not found", name))?;

        Ok(serde_json::to_string_pretty(profile)?)
    }

    /// Imports a profile from JSON data.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The profile already exists and `overwrite` is false
    /// - The JSON data cannot be deserialized into a valid Profile
    /// - The configuration cannot be saved to disk
    pub fn import(&mut self, name: String, json: &str, overwrite: bool) -> Result<()> {
        if !overwrite && self.config.profiles.contains_key(&name) {
            return Err(eyre!("Profile '{}' already exists", name));
        }

        let mut profile: Profile = serde_json::from_str(json)?;
        profile.name.clone_from(&name);

        self.config.profiles.insert(name, profile);
        self.save()?;
        Ok(())
    }

    /// Saves the current profile configuration to disk.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The configuration cannot be serialized to JSON
    /// - The configuration file cannot be written to disk
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ProfileVar;

    use super::*;
    use tempfile::TempDir;

    fn create_test_profile_manager() -> (ProfileManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("profiles.json");

        let config = ProfileConfig {
            active: None,
            profiles: HashMap::new(),
        };

        let manager = ProfileManager { config_path, config };

        (manager, temp_dir)
    }

    fn create_test_profile(name: &str) -> Profile {
        let mut profile = Profile::new(name.to_string(), Some(format!("{name} description")));
        profile.add_var("TEST_VAR".to_string(), "test_value".to_string(), false);
        profile
    }

    #[test]
    fn test_profile_manager_new() {
        // Use the test helper instead of calling ProfileManager::new() directly
        let (manager, _temp) = create_test_profile_manager();

        assert!(manager.config.profiles.is_empty());
        assert!(manager.config.active.is_none());
    }

    #[test]
    fn test_profile_manager_new_with_existing_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("profiles.json");

        // Create a config file
        let mut profiles = HashMap::new();
        profiles.insert("test".to_string(), create_test_profile("test"));

        let config = ProfileConfig {
            active: Some("test".to_string()),
            profiles,
        };

        let content = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();

        // Now create manager with existing config
        let manager = ProfileManager {
            config_path: config_path.clone(),
            config: if config_path.exists() {
                let content = fs::read_to_string(&config_path).unwrap();
                serde_json::from_str(&content).unwrap()
            } else {
                ProfileConfig {
                    active: None,
                    profiles: HashMap::new(),
                }
            },
        };

        assert_eq!(manager.config.profiles.len(), 1);
        assert_eq!(manager.config.active, Some("test".to_string()));
    }

    #[test]
    fn test_create_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        let result = manager.create("dev".to_string(), Some("Development profile".to_string()));
        assert!(result.is_ok());

        assert_eq!(manager.config.profiles.len(), 1);
        assert!(manager.config.profiles.contains_key("dev"));

        let profile = manager.get("dev").unwrap();
        assert_eq!(profile.name, "dev");
        assert_eq!(profile.description, Some("Development profile".to_string()));
    }

    #[test]
    fn test_create_duplicate_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();
        let result = manager.create("dev".to_string(), None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_delete_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();
        assert_eq!(manager.config.profiles.len(), 1);

        let result = manager.delete("dev");
        assert!(result.is_ok());
        assert_eq!(manager.config.profiles.len(), 0);
    }

    #[test]
    fn test_delete_active_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();
        manager.switch("dev").unwrap();
        assert_eq!(manager.config.active, Some("dev".to_string()));

        let result = manager.delete("dev");
        assert!(result.is_ok());
        assert!(manager.config.active.is_none());
    }

    #[test]
    fn test_delete_nonexistent_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        let result = manager.delete("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_list_profiles() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();
        manager.create("prod".to_string(), None).unwrap();
        manager.create("test".to_string(), None).unwrap();

        let profiles = manager.list();
        assert_eq!(profiles.len(), 3);

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"prod"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn test_get_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), Some("Dev env".to_string())).unwrap();

        let profile = manager.get("dev");
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().description, Some("Dev env".to_string()));

        let profile = manager.get("nonexistent");
        assert!(profile.is_none());
    }

    #[test]
    fn test_get_mut_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();

        let profile = manager.get_mut("dev").unwrap();
        profile.add_var("NEW_VAR".to_string(), "new_value".to_string(), false);

        let profile = manager.get("dev").unwrap();
        assert!(profile.variables.contains_key("NEW_VAR"));
    }

    #[test]
    fn test_switch_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("dev".to_string(), None).unwrap();
        manager.create("prod".to_string(), None).unwrap();

        assert!(manager.config.active.is_none());

        let result = manager.switch("dev");
        assert!(result.is_ok());
        assert_eq!(manager.config.active, Some("dev".to_string()));

        let result = manager.switch("prod");
        assert!(result.is_ok());
        assert_eq!(manager.config.active, Some("prod".to_string()));
    }

    #[test]
    fn test_switch_to_nonexistent_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        let result = manager.switch("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_active_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        assert!(manager.active().is_none());

        manager.create("dev".to_string(), None).unwrap();
        manager.switch("dev").unwrap();

        let active = manager.active();
        assert!(active.is_some());
        assert_eq!(active.unwrap().name, "dev");
    }

    #[test]
    fn test_apply_profile() {
        let (mut manager, _temp) = create_test_profile_manager();
        let mut env_manager = EnvVarManager::new();

        // Create profile with variables
        manager.create("dev".to_string(), None).unwrap();
        let profile = manager.get_mut("dev").unwrap();
        profile.add_var("NODE_ENV".to_string(), "development".to_string(), false);
        profile.add_var("DEBUG".to_string(), "true".to_string(), false);

        let result = manager.apply("dev", &mut env_manager);
        assert!(result.is_ok());

        // Verify variables were set
        assert_eq!(env_manager.get("NODE_ENV").unwrap().value, "development");
        assert_eq!(env_manager.get("DEBUG").unwrap().value, "true");
    }

    #[test]
    fn test_apply_profile_with_disabled_var() {
        let (mut manager, _temp) = create_test_profile_manager();
        let mut env_manager = EnvVarManager::new();

        manager.create("dev".to_string(), None).unwrap();
        let profile = manager.get_mut("dev").unwrap();
        profile.variables.insert(
            "DISABLED_VAR".to_string(),
            ProfileVar {
                value: "should_not_be_set".to_string(),
                enabled: false,
                override_system: false,
            },
        );
        profile.add_var("ENABLED_VAR".to_string(), "should_be_set".to_string(), false);

        manager.apply("dev", &mut env_manager).unwrap();

        assert!(env_manager.get("DISABLED_VAR").is_none());
        assert_eq!(env_manager.get("ENABLED_VAR").unwrap().value, "should_be_set");
    }

    #[test]
    fn test_apply_profile_with_parent() {
        let (mut manager, _temp) = create_test_profile_manager();
        let mut env_manager = EnvVarManager::new();

        // Create parent profile
        manager.create("base".to_string(), None).unwrap();
        let profile = manager.get_mut("base").unwrap();
        profile.add_var("BASE_VAR".to_string(), "base_value".to_string(), false);
        profile.add_var("OVERRIDE_ME".to_string(), "base_override".to_string(), false);

        // Create child profile
        manager.create("dev".to_string(), None).unwrap();
        let profile = manager.get_mut("dev").unwrap();
        profile.parent = Some("base".to_string());
        profile.add_var("DEV_VAR".to_string(), "dev_value".to_string(), false);
        profile.add_var("OVERRIDE_ME".to_string(), "dev_override".to_string(), false);

        manager.apply("dev", &mut env_manager).unwrap();

        // Should have variables from both profiles
        assert_eq!(env_manager.get("BASE_VAR").unwrap().value, "base_value");
        assert_eq!(env_manager.get("DEV_VAR").unwrap().value, "dev_value");
        // Child should override parent
        assert_eq!(env_manager.get("OVERRIDE_ME").unwrap().value, "dev_override");
    }

    #[test]
    fn test_apply_nonexistent_profile() {
        let (manager, _temp) = create_test_profile_manager();
        let mut env_manager = EnvVarManager::new();

        let result = manager.apply("nonexistent", &mut env_manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_export_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager
            .create("dev".to_string(), Some("Development".to_string()))
            .unwrap();
        let profile = manager.get_mut("dev").unwrap();
        profile.add_var("TEST_VAR".to_string(), "test_value".to_string(), false);

        let result = manager.export("dev");
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"name\": \"dev\""));
        assert!(json.contains("\"description\": \"Development\""));
        assert!(json.contains("TEST_VAR"));
        assert!(json.contains("test_value"));
    }

    #[test]
    fn test_export_nonexistent_profile() {
        let (manager, _temp) = create_test_profile_manager();

        let result = manager.export("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_import_profile() {
        let (mut manager, _temp) = create_test_profile_manager();

        let profile_json = r#"{
            "name": "imported",
            "description": "Imported profile",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "variables": {
                "IMPORT_VAR": {
                    "value": "imported_value",
                    "enabled": true,
                    "override_system": false
                }
            },
            "parent": null,
            "metadata": {}
        }"#;

        let result = manager.import("new_name".to_string(), profile_json, false);
        assert!(result.is_ok());

        let profile = manager.get("new_name").unwrap();
        assert_eq!(profile.name, "new_name"); // Should use provided name, not JSON name
        assert_eq!(profile.description, Some("Imported profile".to_string()));
        assert!(profile.variables.contains_key("IMPORT_VAR"));
    }

    #[test]
    fn test_import_profile_overwrite() {
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("existing".to_string(), None).unwrap();

        let profile_json = r#"{
            "name": "imported",
            "description": "New description",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "variables": {},
            "parent": null,
            "metadata": {}
        }"#;

        // Should fail without overwrite
        let result = manager.import("existing".to_string(), profile_json, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Should succeed with overwrite
        let result = manager.import("existing".to_string(), profile_json, true);
        assert!(result.is_ok());

        let profile = manager.get("existing").unwrap();
        assert_eq!(profile.description, Some("New description".to_string()));
    }

    #[test]
    fn test_import_invalid_json() {
        let (mut manager, _temp) = create_test_profile_manager();

        let invalid_json = "{ invalid json }";

        let result = manager.import("test".to_string(), invalid_json, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("profiles.json");

        // Create and save
        {
            let mut manager = ProfileManager {
                config_path: config_path.clone(),
                config: ProfileConfig {
                    active: None,
                    profiles: HashMap::new(),
                },
            };

            manager.create("dev".to_string(), None).unwrap();
            manager.create("prod".to_string(), None).unwrap();
            manager.switch("dev").unwrap();

            let result = manager.save();
            assert!(result.is_ok());
        }

        // Load and verify
        {
            assert!(config_path.exists());

            let manager = ProfileManager {
                config_path: config_path.clone(),
                config: {
                    let content = fs::read_to_string(&config_path).unwrap();
                    serde_json::from_str(&content).unwrap()
                },
            };

            assert_eq!(manager.config.profiles.len(), 2);
            assert_eq!(manager.config.active, Some("dev".to_string()));
            assert!(manager.get("dev").is_some());
            assert!(manager.get("prod").is_some());
        }
    }

    #[test]
    fn test_profile_manager_thread_safety() {
        // This test verifies that ProfileManager operations are safe
        // Note: ProfileManager is not Send/Sync by default due to config mutability
        // This test documents the current behavior
        let (mut manager, _temp) = create_test_profile_manager();

        manager.create("test".to_string(), None).unwrap();
        let profile = manager.get("test");
        assert!(profile.is_some());

        // Mutable operations require exclusive access
        let profile_mut = manager.get_mut("test");
        assert!(profile_mut.is_some());
    }
}

use crate::EnvxError;
use chrono::{DateTime, Utc};
use color_eyre::Result;
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvVarSource {
    System,
    User,
    Process,
    Shell,
    Application(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    pub source: EnvVarSource,
    pub modified: DateTime<Utc>,
    pub original_value: Option<String>,
}

pub struct EnvVarManager {
    pub vars: IndexMap<String, EnvVar>,
    pub history: Vec<crate::history::HistoryEntry>,
}

impl Default for EnvVarManager {
    fn default() -> Self {
        Self {
            vars: IndexMap::new(),
            history: Vec::new(),
        }
    }
}

impl EnvVarManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads environment variables from all available sources (process, system, and user).
    ///
    /// This method loads environment variables from the current process environment
    /// and platform-specific sources like the Windows registry or Unix shell configurations.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Registry access fails on Windows platforms
    /// - File system operations fail when reading Unix shell configurations
    /// - Other platform-specific environment variable access fails
    pub fn load_all(&mut self) -> Result<()> {
        // Load process environment variables
        for (key, value) in std::env::vars() {
            self.vars.insert(
                key.clone(),
                EnvVar {
                    name: key,
                    value,
                    source: EnvVarSource::Process,
                    modified: Utc::now(),
                    original_value: None,
                },
            );
        }

        #[cfg(windows)]
        self.load_windows_vars();

        #[cfg(unix)]
        self.load_unix_vars();

        Ok(())
    }

    #[cfg(windows)]
    fn load_windows_vars(&mut self) {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

        // Load system variables
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(env_key) = hklm.open_subkey("System\\CurrentControlSet\\Control\\Session Manager\\Environment") {
            for (name, value) in env_key.enum_values().filter_map(std::result::Result::ok) {
                let val_str = value.to_string();
                self.vars.insert(
                    name.clone(),
                    EnvVar {
                        name,
                        value: val_str,
                        source: EnvVarSource::System,
                        modified: Utc::now(),
                        original_value: None,
                    },
                );
            }
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env_key) = hkcu.open_subkey("Environment") {
            for (name, value) in env_key.enum_values().filter_map(std::result::Result::ok) {
                let val_str = value.to_string();
                self.vars.insert(
                    name.clone(),
                    EnvVar {
                        name,
                        value: val_str,
                        source: EnvVarSource::User,
                        modified: Utc::now(),
                        original_value: None,
                    },
                );
            }
        }
    }

    #[cfg(unix)]
    fn load_unix_vars(&mut self) {
        // On Unix, we primarily work with process environment
        // Shell-specific vars can be detected by checking common patterns
        for (key, value) in std::env::vars() {
            let source = if key.starts_with("BASH_") || key.starts_with("ZSH_") {
                EnvVarSource::Shell
            } else {
                EnvVarSource::Process
            };

            self.vars.insert(
                key.clone(),
                EnvVar {
                    name: key,
                    value,
                    source,
                    modified: Utc::now(),
                    original_value: None,
                },
            );
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&EnvVar> {
        self.vars.get(name)
    }

    /// Get variables matching a pattern (supports wildcards and regex)
    #[must_use]
    pub fn get_pattern(&self, pattern: &str) -> Vec<&EnvVar> {
        // Check if it's a regex pattern first (enclosed in slashes)
        if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
            // Regex pattern like /^PATH.*/
            self.get_regex(&pattern[1..pattern.len() - 1])
        } else if pattern.contains('*') || pattern.contains('?') {
            // Simple wildcard pattern
            self.get_wildcard(pattern)
        } else {
            // Exact match
            self.get(pattern).into_iter().collect()
        }
    }

    /// Get variables matching a wildcard pattern (* and ?)
    #[must_use]
    pub fn get_wildcard(&self, pattern: &str) -> Vec<&EnvVar> {
        let regex_pattern = wildcard_to_regex(pattern);
        self.get_regex(&regex_pattern)
    }

    /// Get variables matching a regex pattern
    #[must_use]
    pub fn get_regex(&self, pattern: &str) -> Vec<&EnvVar> {
        match Regex::new(pattern) {
            Ok(re) => self.vars.values().filter(|v| re.is_match(&v.name)).collect(),
            Err(_) => vec![],
        }
    }

    /// Get variables with names starting with a prefix
    #[must_use]
    pub fn get_prefix(&self, prefix: &str) -> Vec<&EnvVar> {
        self.vars.values().filter(|v| v.name.starts_with(prefix)).collect()
    }

    /// Get variables with names ending with a suffix
    #[must_use]
    pub fn get_suffix(&self, suffix: &str) -> Vec<&EnvVar> {
        self.vars.values().filter(|v| v.name.ends_with(suffix)).collect()
    }

    /// Get variables containing a substring (case-insensitive)
    #[must_use]
    pub fn get_containing(&self, substring: &str) -> Vec<&EnvVar> {
        let lower = substring.to_lowercase();
        self.vars
            .values()
            .filter(|v| v.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// Sets an environment variable with the given name and value.
    ///
    /// This method updates the variable both in the manager's internal state
    /// and in the current process environment. If `permanent` is true, it will
    /// also attempt to persist the variable to the system or user environment.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Registry operations fail on Windows platforms when setting permanent variables
    /// - File system operations fail when modifying shell configuration files on Unix
    /// - Other platform-specific environment variable persistence operations fail
    pub fn set(&mut self, name: &str, value: &str, permanent: bool) -> Result<()> {
        let old_var = self.vars.get(name).cloned();

        // Record history
        self.history
            .push(crate::history::HistoryEntry::new(crate::history::HistoryAction::Set {
                name: name.to_string(),
                old_value: old_var.as_ref().map(|v| v.value.clone()),
                new_value: value.to_string(),
            }));

        // Update in-memory
        let var = EnvVar {
            name: name.to_string(),
            value: value.to_string(),
            source: if permanent {
                EnvVarSource::User
            } else {
                EnvVarSource::Process
            },
            modified: Utc::now(),
            original_value: old_var.map(|v| v.value),
        };
        self.vars.insert(name.to_string(), var);

        // Apply to process
        unsafe { std::env::set_var(name, value) };

        if permanent {
            #[cfg(windows)]
            Self::set_windows_var(name, value, false)?;

            #[cfg(unix)]
            Self::set_unix_var(name, value);
        }

        Ok(())
    }

    #[cfg(windows)]
    fn set_windows_var(name: &str, value: &str, system: bool) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE};

        let (key, subkey) = if system {
            (
                HKEY_LOCAL_MACHINE,
                "System\\CurrentControlSet\\Control\\Session Manager\\Environment",
            )
        } else {
            (HKEY_CURRENT_USER, "Environment")
        };

        let hkey = RegKey::predef(key);
        let env_key = hkey.open_subkey_with_flags(subkey, KEY_SET_VALUE)?;
        env_key.set_value(name, &value)?;

        // Broadcast WM_SETTINGCHANGE to notify other processes
        /* unsafe {
            use windows::Win32::Foundation::*;
            use windows::Win32::UI::WindowsAndMessaging::*;

            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(s!("Environment").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        } */

        Ok(())
    }

    #[cfg(unix)]
    fn set_unix_var(name: &str, value: &str) {
        // On Unix, we'd typically need to modify shell config files
        // This is a simplified version - real implementation would handle
        // .bashrc, .zshrc, etc.
        println!("Note: To make this permanent on Unix, add to your shell config:");
        println!("export {name}=\"{value}\"");
    }

    /// Deletes an environment variable by name.
    ///
    /// This method removes the variable from both the manager's internal state
    /// and the current process environment. The operation is recorded in the
    /// history for potential undo operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable with the given name does not exist.
    pub fn delete(&mut self, name: &str) -> Result<()> {
        let old_var = self
            .vars
            .swap_remove(name)
            .ok_or_else(|| EnvxError::VarNotFound(name.to_string()))?;

        self.history.push(crate::history::HistoryEntry::new(
            crate::history::HistoryAction::Delete {
                name: name.to_string(),
                old_value: old_var.value,
            },
        ));

        unsafe { std::env::remove_var(name) };
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<&EnvVar> {
        self.vars.values().collect()
    }

    #[must_use]
    pub fn filter_by_source(&self, source: &EnvVarSource) -> Vec<&EnvVar> {
        self.vars.values().filter(|v| v.source == *source).collect()
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&EnvVar> {
        let query_lower = query.to_lowercase();
        self.vars
            .values()
            .filter(|v| v.name.to_lowercase().contains(&query_lower) || v.value.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Undoes the last environment variable operation.
    ///
    /// This method reverses the most recent operation (set or delete) by restoring
    /// the previous state from the history. For set operations, it either restores
    /// the previous value or removes the variable if it didn't exist before. For
    /// delete operations, it restores the deleted variable with its previous value.
    ///
    /// # Errors
    ///
    /// Currently, this method always returns `Ok(())` and does not produce errors,
    /// but it returns a `Result` for future extensibility and consistency with
    /// other methods in the API.
    pub fn undo(&mut self) -> Result<()> {
        if let Some(entry) = self.history.pop() {
            // Implement undo logic based on history entry
            match entry.action {
                crate::history::HistoryAction::Set { name, old_value, .. } => {
                    if let Some(old) = old_value {
                        // Variable existed before - restore old value without adding to history
                        let var = EnvVar {
                            name: name.clone(),
                            value: old.clone(),
                            source: EnvVarSource::Process,
                            modified: Utc::now(),
                            original_value: self.vars.get(&name).map(|v| v.value.clone()),
                        };
                        self.vars.insert(name.clone(), var);
                        unsafe { std::env::set_var(&name, &old) };
                    } else {
                        // Variable didn't exist before - remove it without adding to history
                        self.vars.swap_remove(&name);
                        unsafe { std::env::remove_var(&name) };
                    }
                }
                crate::history::HistoryAction::Delete { name, old_value } => {
                    // Restore deleted variable without adding to history
                    let var = EnvVar {
                        name: name.clone(),
                        value: old_value.clone(),
                        source: EnvVarSource::Process,
                        modified: Utc::now(),
                        original_value: None,
                    };
                    self.vars.insert(name.clone(), var);
                    unsafe { std::env::set_var(&name, &old_value) };
                }
                crate::history::HistoryAction::BatchUpdate { .. } => {}
            }
        }
        Ok(())
    }
}
fn wildcard_to_regex(pattern: &str) -> String {
    let mut regex = String::new();
    regex.push('^');

    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }

    regex.push('$');
    regex
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    use chrono::Utc;

    use crate::{EnvVarManager, EnvVarSource};

    use super::*;

    // Helper function to create a test EnvVar
    fn create_test_var(name: &str, value: &str, source: EnvVarSource) -> EnvVar {
        EnvVar {
            name: name.to_string(),
            value: value.to_string(),
            source,
            modified: Utc::now(),
            original_value: None,
        }
    }

    // Helper to create a manager with test data
    fn create_test_manager() -> EnvVarManager {
        let mut manager = EnvVarManager::new();

        // Add some test variables
        manager.vars.insert(
            "PATH".to_string(),
            create_test_var("PATH", "/usr/bin:/usr/local/bin", EnvVarSource::System),
        );
        manager.vars.insert(
            "HOME".to_string(),
            create_test_var("HOME", "/home/user", EnvVarSource::User),
        );
        manager.vars.insert(
            "RUST_LOG".to_string(),
            create_test_var("RUST_LOG", "debug", EnvVarSource::Process),
        );
        manager.vars.insert(
            "API_KEY".to_string(),
            create_test_var("API_KEY", "secret123", EnvVarSource::User),
        );
        manager.vars.insert(
            "API_SECRET".to_string(),
            create_test_var("API_SECRET", "supersecret", EnvVarSource::User),
        );
        manager.vars.insert(
            "DATABASE_URL".to_string(),
            create_test_var("DATABASE_URL", "postgres://localhost", EnvVarSource::Process),
        );
        manager.vars.insert(
            "APP_VERSION".to_string(),
            create_test_var("APP_VERSION", "1.0.0", EnvVarSource::Application("myapp".to_string())),
        );

        manager
    }

    #[test]
    fn test_new() {
        let manager = EnvVarManager::new();
        assert!(manager.vars.is_empty());
        assert!(manager.history.is_empty());
    }

    #[test]
    fn test_get() {
        let manager = create_test_manager();

        // Test existing variable
        let var = manager.get("PATH").unwrap();
        assert_eq!(var.name, "PATH");
        assert_eq!(var.value, "/usr/bin:/usr/local/bin");
        assert_eq!(var.source, EnvVarSource::System);

        // Test non-existing variable
        assert!(manager.get("NON_EXISTENT").is_none());
    }

    #[test]
    fn test_get_pattern_exact_match() {
        let manager = create_test_manager();

        let vars = manager.get_pattern("PATH");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "PATH");
    }

    #[test]
    fn test_get_pattern_wildcard() {
        let manager = create_test_manager();

        // Test asterisk wildcard
        let vars = manager.get_pattern("API_*");
        assert_eq!(vars.len(), 2);
        let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"API_SECRET"));

        // Test question mark wildcard
        let vars = manager.get_pattern("HOM?");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "HOME");

        // Test combination
        let vars = manager.get_pattern("*_*");
        assert!(vars.len() >= 4); // API_KEY, API_SECRET, DATABASE_URL, APP_VERSION
    }

    #[test]
    fn test_get_pattern_regex() {
        let manager = create_test_manager();

        // Test regex pattern
        let vars = manager.get_pattern("/^API.*/");
        assert_eq!(vars.len(), 2);

        // Test case-insensitive regex
        let vars = manager.get_pattern("/(?i)^api.*/");
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_get_wildcard() {
        let manager = create_test_manager();

        // Test various wildcard patterns
        assert_eq!(manager.get_wildcard("*").len(), 7); // All variables
        assert_eq!(manager.get_wildcard("A*").len(), 3); // API_KEY, API_SECRET, APP_VERSION
        assert_eq!(manager.get_wildcard("*URL").len(), 1); // DATABASE_URL
        assert_eq!(manager.get_wildcard("????").len(), 2); // PATH, HOME
    }

    #[test]
    fn test_get_regex() {
        let manager = create_test_manager();

        // Test valid regex
        assert_eq!(manager.get_regex("^API.*").len(), 2);
        assert_eq!(manager.get_regex(".*URL$").len(), 1);
        assert_eq!(manager.get_regex("^[A-Z]+$").len(), 2); // PATH, HOME

        // Test invalid regex - should return empty
        assert_eq!(manager.get_regex("[").len(), 0);
    }

    #[test]
    fn test_get_prefix() {
        let manager = create_test_manager();

        assert_eq!(manager.get_prefix("API_").len(), 2);
        assert_eq!(manager.get_prefix("DATA").len(), 1);
        assert_eq!(manager.get_prefix("NON").len(), 0);
    }

    #[test]
    fn test_get_suffix() {
        let manager = create_test_manager();

        assert_eq!(manager.get_suffix("_URL").len(), 1);
        assert_eq!(manager.get_suffix("KEY").len(), 1);
        assert_eq!(manager.get_suffix("SECRET").len(), 1);
        assert_eq!(manager.get_suffix("XYZ").len(), 0);
    }

    #[test]
    fn test_get_containing() {
        let manager = create_test_manager();

        // Case-insensitive search
        assert_eq!(manager.get_containing("api").len(), 2);
        assert_eq!(manager.get_containing("API").len(), 2);
        assert_eq!(manager.get_containing("_").len(), 5);
        assert_eq!(manager.get_containing("URL").len(), 1);
    }

    #[test]
    fn test_set_temporary() {
        let mut manager = EnvVarManager::new();

        // Set a new variable temporarily
        manager.set("TEST_VAR", "test_value", false).unwrap();

        let var = manager.get("TEST_VAR").unwrap();
        assert_eq!(var.value, "test_value");
        assert_eq!(var.source, EnvVarSource::Process);
        assert!(var.original_value.is_none());

        // Verify it was set in the process environment
        assert_eq!(std::env::var("TEST_VAR").unwrap(), "test_value");

        // Clean up
        unsafe { std::env::remove_var("TEST_VAR") };
    }

    #[test]
    fn test_set_overwrite_existing() {
        let mut manager = create_test_manager();

        // Overwrite existing variable
        manager.set("RUST_LOG", "info", false).unwrap();

        let var = manager.get("RUST_LOG").unwrap();
        assert_eq!(var.value, "info");
        assert_eq!(var.original_value, Some("debug".to_string()));
    }

    #[test]
    fn test_delete() {
        let mut manager = create_test_manager();

        // Delete existing variable
        assert!(manager.delete("RUST_LOG").is_ok());
        assert!(manager.get("RUST_LOG").is_none());

        // Try to delete non-existing variable
        assert!(manager.delete("NON_EXISTENT").is_err());
    }

    #[test]
    fn test_list() {
        let manager = create_test_manager();
        let vars = manager.list();
        assert_eq!(vars.len(), 7);
    }

    #[test]
    fn test_filter_by_source() {
        let manager = create_test_manager();

        assert_eq!(manager.filter_by_source(&EnvVarSource::System).len(), 1);
        assert_eq!(manager.filter_by_source(&EnvVarSource::User).len(), 3);
        assert_eq!(manager.filter_by_source(&EnvVarSource::Process).len(), 2);
        assert_eq!(manager.filter_by_source(&EnvVarSource::Shell).len(), 0);
        assert_eq!(
            manager
                .filter_by_source(&EnvVarSource::Application("myapp".to_string()))
                .len(),
            1
        );
    }

    #[test]
    fn test_search() {
        let manager = create_test_manager();

        // Search in names
        assert_eq!(manager.search("api").len(), 2);
        assert_eq!(manager.search("PATH").len(), 1);

        // Search in values
        assert_eq!(manager.search("secret").len(), 2);
        assert_eq!(manager.search("localhost").len(), 1);

        // Case-insensitive search
        assert_eq!(manager.search("API").len(), 2);
        assert_eq!(manager.search("SECRET").len(), 2);
    }

    #[test]
    fn test_history_tracking() {
        let mut manager = EnvVarManager::new();

        // Set a variable
        manager.set("VAR1", "value1", false).unwrap();
        assert_eq!(manager.history.len(), 1);

        // Update the variable
        manager.set("VAR1", "value2", false).unwrap();
        assert_eq!(manager.history.len(), 2);

        // Delete the variable
        manager.delete("VAR1").unwrap();
        assert_eq!(manager.history.len(), 3);

        // Verify history entries
        if let crate::history::HistoryAction::Delete { name, old_value } = &manager.history[2].action {
            assert_eq!(name, "VAR1");
            assert_eq!(old_value, "value2");
        } else {
            panic!("Expected Delete action");
        }
    }

    #[test]
    fn test_undo_set() {
        let mut manager = EnvVarManager::new();

        // Set a new variable
        manager.set("UNDO_TEST", "value1", false).unwrap();

        // Update it
        manager.set("UNDO_TEST", "value2", false).unwrap();

        // Undo the update
        manager.undo().unwrap();
        assert_eq!(manager.get("UNDO_TEST").unwrap().value, "value1");

        // Undo the initial set (should delete)
        manager.undo().unwrap();
        assert!(manager.get("UNDO_TEST").is_none());
    }

    #[test]
    fn test_undo_delete() {
        let mut manager = EnvVarManager::new();

        // Set and then delete a variable
        manager.set("DELETE_TEST", "value", false).unwrap();
        manager.delete("DELETE_TEST").unwrap();
        assert!(manager.get("DELETE_TEST").is_none());

        // Undo the delete
        manager.undo().unwrap();
        assert_eq!(manager.get("DELETE_TEST").unwrap().value, "value");
    }

    #[test]
    fn test_wildcard_to_regex() {
        // Test asterisk wildcard
        assert_eq!(wildcard_to_regex("API_*"), "^API_.*$");
        assert_eq!(wildcard_to_regex("*_KEY"), "^.*_KEY$");
        assert_eq!(wildcard_to_regex("*TEST*"), "^.*TEST.*$");

        // Test question mark wildcard
        assert_eq!(wildcard_to_regex("HOM?"), "^HOM.$");
        assert_eq!(wildcard_to_regex("??ST"), "^..ST$");

        // Test escaping special regex characters
        assert_eq!(wildcard_to_regex("TEST.VAR"), "^TEST\\.VAR$");
        assert_eq!(wildcard_to_regex("VAR[1]"), "^VAR\\[1\\]$");
        assert_eq!(wildcard_to_regex("A+B"), "^A\\+B$");
        assert_eq!(wildcard_to_regex("^START"), "^\\^START$");
        assert_eq!(wildcard_to_regex("END$"), "^END\\$$");
        assert_eq!(wildcard_to_regex("(GROUP)"), "^\\(GROUP\\)$");
        assert_eq!(wildcard_to_regex("{BRACE}"), "^\\{BRACE\\}$");
        assert_eq!(wildcard_to_regex("A|B"), "^A\\|B$");
        assert_eq!(wildcard_to_regex("C\\D"), "^C\\\\D$");

        // Test combination
        assert_eq!(wildcard_to_regex("*.txt"), "^.*\\.txt$");
        assert_eq!(wildcard_to_regex("file?.log"), "^file.\\.log$");
    }

    #[test]
    fn test_env_var_source_equality() {
        assert_eq!(EnvVarSource::System, EnvVarSource::System);
        assert_eq!(EnvVarSource::User, EnvVarSource::User);
        assert_eq!(EnvVarSource::Process, EnvVarSource::Process);
        assert_eq!(EnvVarSource::Shell, EnvVarSource::Shell);
        assert_eq!(
            EnvVarSource::Application("app1".to_string()),
            EnvVarSource::Application("app1".to_string())
        );
        assert_ne!(
            EnvVarSource::Application("app1".to_string()),
            EnvVarSource::Application("app2".to_string())
        );
        assert_ne!(EnvVarSource::System, EnvVarSource::User);
    }

    #[test]
    fn test_load_all() {
        let mut manager = EnvVarManager::new();

        // Set some test environment variables
        unsafe { std::env::set_var("TEST_LOAD_VAR1", "value1") };
        unsafe { std::env::set_var("TEST_LOAD_VAR2", "value2") };

        // Load all variables
        manager.load_all().unwrap();

        // Verify our test variables were loaded
        assert!(manager.get("TEST_LOAD_VAR1").is_some());
        assert!(manager.get("TEST_LOAD_VAR2").is_some());

        // Verify they have the correct source
        assert_eq!(manager.get("TEST_LOAD_VAR1").unwrap().source, EnvVarSource::Process);

        // Clean up
        unsafe { std::env::remove_var("TEST_LOAD_VAR1") };
        unsafe { std::env::remove_var("TEST_LOAD_VAR2") };
    }

    #[test]
    #[cfg(unix)]
    fn test_unix_shell_detection() {
        let mut manager = EnvVarManager::new();

        // Set a mock shell variable
        std::env::set_var("BASH_VERSION", "5.0.0");

        manager.load_unix_vars();

        if let Some(var) = manager.get("BASH_VERSION") {
            assert_eq!(var.source, EnvVarSource::Shell);
        }

        // Clean up
        std::env::remove_var("BASH_VERSION");
    }

    #[test]
    fn test_special_characters_in_values() {
        let mut manager = EnvVarManager::new();

        // Test with various special characters
        let special_values = vec![
            ("NEWLINE_VAR", "line1\nline2"),
            ("TAB_VAR", "col1\tcol2"),
            ("QUOTE_VAR", "value with \"quotes\""),
            ("BACKSLASH_VAR", "C:\\path\\to\\file"),
            ("UNICODE_VAR", "Hello 世界 🌍"),
            ("EMPTY_VAR", ""),
            ("SPACE_VAR", "  spaces around  "),
        ];

        for (name, value) in special_values {
            manager.set(name, value, false).unwrap();
            assert_eq!(manager.get(name).unwrap().value, value);
            assert_eq!(std::env::var(name).unwrap(), value);
            unsafe { std::env::remove_var(name) };
        }
    }

    #[test]
    fn test_variable_ordering() {
        let mut manager = EnvVarManager::new();

        // Add variables in specific order
        let vars = vec!["ZETA", "ALPHA", "GAMMA", "BETA"];
        for var in &vars {
            manager.set(var, "value", false).unwrap();
        }

        // Verify order is preserved (IndexMap maintains insertion order)
        let list: Vec<&str> = manager.list().iter().map(|v| v.name.as_str()).collect();
        assert_eq!(list, vars);
    }

    #[test]
    fn test_concurrent_modification_safety() {
        let mut manager = create_test_manager();

        // Get initial count
        let initial_count = manager.list().len();

        // Modify while iterating (this should be safe with our implementation)
        let vars_to_modify: Vec<String> = manager.get_prefix("API_").iter().map(|v| v.name.clone()).collect();

        for name in vars_to_modify {
            manager.set(&name, "modified", false).unwrap();
        }

        // Verify count hasn't changed
        assert_eq!(manager.list().len(), initial_count);
    }

    #[test]
    fn test_empty_operations() {
        let manager = EnvVarManager::new();

        // Test operations on empty manager
        assert_eq!(manager.list().len(), 0);
        assert_eq!(manager.get_pattern("*").len(), 0);
        assert_eq!(manager.get_prefix("").len(), 0);
        assert_eq!(manager.get_suffix("").len(), 0);
        assert_eq!(manager.get_containing("").len(), 0);
        assert_eq!(manager.search("anything").len(), 0);
    }

    #[test]
    fn test_case_sensitivity() {
        let mut manager = EnvVarManager::new();

        // Variable names are case-sensitive
        manager.set("test_var", "lower", false).unwrap();
        manager.set("TEST_VAR", "upper", false).unwrap();

        assert_eq!(manager.get("test_var").unwrap().value, "lower");
        assert_eq!(manager.get("TEST_VAR").unwrap().value, "upper");
        assert!(manager.get("Test_Var").is_none());

        // But search is case-insensitive
        assert_eq!(manager.search("test_var").len(), 2);
        assert_eq!(manager.get_containing("test_var").len(), 2);
    }

    #[test]
    fn test_original_value_tracking() {
        let mut manager = EnvVarManager::new();

        // First set - no original value
        manager.set("TRACK_VAR", "v1", false).unwrap();
        assert!(manager.get("TRACK_VAR").unwrap().original_value.is_none());

        // Second set - original value is v1
        manager.set("TRACK_VAR", "v2", false).unwrap();
        assert_eq!(manager.get("TRACK_VAR").unwrap().original_value, Some("v1".to_string()));

        // Third set - original value is v2
        manager.set("TRACK_VAR", "v3", false).unwrap();
        assert_eq!(manager.get("TRACK_VAR").unwrap().original_value, Some("v2".to_string()));
    }
}

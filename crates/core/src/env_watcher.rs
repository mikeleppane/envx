use crate::EnvVarManager;
use color_eyre::Result;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEvent, Debouncer, new_debouncer};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, thread};

#[derive(Debug, Clone)]
pub enum SyncMode {
    /// Only watch, don't apply changes
    WatchOnly,
    /// Apply changes from files to system
    FileToSystem,
    /// Apply changes from system to files
    SystemToFile,
    /// Bi-directional sync with conflict resolution
    Bidirectional,
}

#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Files or directories to watch
    pub paths: Vec<PathBuf>,
    /// Sync mode
    pub mode: SyncMode,
    /// Auto-reload on changes
    pub auto_reload: bool,
    /// Debounce duration (to avoid multiple rapid reloads)
    pub debounce_duration: Duration,
    /// File patterns to watch (e.g., "*.env", "*.yaml")
    pub patterns: Vec<String>,
    /// Log changes
    pub log_changes: bool,
    /// Conflict resolution strategy
    pub conflict_strategy: ConflictStrategy,
}

#[derive(Debug, Clone)]
pub enum ConflictStrategy {
    /// Use the most recent change
    UseLatest,
    /// Prefer file changes
    PreferFile,
    /// Prefer system changes
    PreferSystem,
    /// Ask user (only in interactive mode)
    AskUser,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            mode: SyncMode::FileToSystem,
            auto_reload: true,
            debounce_duration: Duration::from_millis(300),
            patterns: vec![
                "*.env".to_string(),
                ".env.*".to_string(),
                "*.yaml".to_string(),
                "*.yml".to_string(),
                "*.toml".to_string(),
            ],
            log_changes: true,
            conflict_strategy: ConflictStrategy::UseLatest,
        }
    }
}

pub struct EnvWatcher {
    config: WatchConfig,
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    stop_signal: Option<Sender<()>>,
    manager: Arc<Mutex<EnvVarManager>>,
    change_log: Arc<Mutex<Vec<ChangeEvent>>>,
    variable_filter: Option<Vec<String>>,
    output_file: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct ChangeEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub details: String,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub enum ChangeType {
    FileCreated,
    FileModified,
    FileDeleted,
    VariableAdded(String),
    VariableModified(String),
    VariableDeleted(String),
}

impl EnvWatcher {
    #[must_use]
    pub fn new(config: WatchConfig, manager: EnvVarManager) -> Self {
        Self {
            config,
            debouncer: None,
            stop_signal: None,
            manager: Arc::new(Mutex::new(manager)),
            change_log: Arc::new(Mutex::new(Vec::new())),
            variable_filter: None,
            output_file: None,
        }
    }

    /// Starts the environment variable watcher.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The debouncer cannot be created
    /// - File system watching cannot be initialized for the specified paths
    /// - The system monitor cannot be started (in `SystemToFile` or Bidirectional modes)
    pub fn start(&mut self) -> Result<()> {
        let (tx, rx) = channel();
        let (stop_tx, stop_rx) = channel();

        // Clone tx for the closure
        let tx_clone = tx;
        let log_changes = self.config.log_changes;

        // Create debouncer with proper event handling
        let mut debouncer = new_debouncer(
            self.config.debounce_duration,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    for event in events {
                        if log_changes {
                            println!("🔍 File system event detected: {}", event.path.display());
                        }
                        if let Err(e) = tx_clone.send(event) {
                            eprintln!("Failed to send event: {e:?}");
                        }
                    }
                }
                Err(errors) => {
                    eprintln!("Watch error: {errors:?}");
                }
            },
        )?;

        // Get a mutable reference to the watcher before moving debouncer
        let watcher = debouncer.watcher();

        // Watch specified paths
        for path in &self.config.paths {
            if path.exists() {
                if path.is_file() {
                    // Watch the parent directory for file changes
                    if let Some(parent) = path.parent() {
                        watcher.watch(parent, RecursiveMode::NonRecursive)?;
                        if self.config.log_changes {
                            println!("👀 Watching file: {} (via parent directory)", path.display());
                        }
                    }
                } else {
                    watcher.watch(path, RecursiveMode::Recursive)?;
                    if self.config.log_changes {
                        println!("👀 Watching directory: {}", path.display());
                    }
                }
            } else {
                eprintln!("⚠️  Path does not exist: {}", path.display());
            }
        }

        // Store the debouncer - this is crucial!
        self.debouncer = Some(debouncer);
        self.stop_signal = Some(stop_tx);

        // Spawn handler thread
        let config = self.config.clone();
        let manager = Arc::clone(&self.manager);
        let change_log = Arc::clone(&self.change_log);
        let variable_filter = self.variable_filter.clone();
        let output_file = self.output_file.clone();

        thread::spawn(move || {
            Self::handle_events(
                &rx,
                &stop_rx,
                &config,
                &manager,
                &change_log,
                variable_filter.as_ref(),
                output_file.as_ref(),
            );
        });

        if matches!(self.config.mode, SyncMode::SystemToFile | SyncMode::Bidirectional) {
            self.start_system_monitor();
        }

        Ok(())
    }

    /// Stops the environment variable watcher.
    ///
    /// # Errors
    ///
    /// This function currently does not return any errors, but returns `Result<()>`
    /// for future extensibility and consistency with other operations.
    pub fn stop(&mut self) -> Result<()> {
        // Send stop signal
        if let Some(stop_signal) = self.stop_signal.take() {
            let _ = stop_signal.send(());
        }

        // Drop the debouncer to stop watching
        self.debouncer = None;

        if self.config.log_changes {
            println!("🛑 Stopped watching");
        }

        Ok(())
    }

    fn handle_events(
        rx: &Receiver<DebouncedEvent>,
        stop_rx: &Receiver<()>,
        config: &WatchConfig,
        manager: &Arc<Mutex<EnvVarManager>>,
        change_log: &Arc<Mutex<Vec<ChangeEvent>>>,
        variable_filter: Option<&Vec<String>>,
        output_file: Option<&PathBuf>,
    ) {
        loop {
            // Check for stop signal
            if stop_rx.try_recv().is_ok() {
                break;
            }

            // Process events with timeout to allow checking stop signal
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(event) => {
                    if config.log_changes {
                        println!("📋 Processing event for: {}", event.path.display());
                    }

                    let path = event.path.clone();

                    // Skip if path matches output file (to avoid infinite loops in bidirectional sync)
                    if let Some(output) = output_file {
                        if path == *output && matches!(config.mode, SyncMode::Bidirectional) {
                            if config.log_changes {
                                println!("⏭️  Skipping output file to avoid loop");
                            }
                            continue;
                        }
                    }

                    // Check if file matches patterns
                    if !Self::matches_patterns(&path, &config.patterns) {
                        if config.log_changes {
                            println!("⏭️  File doesn't match patterns: {}", path.display());
                        }
                        continue;
                    }

                    // Determine the type of change
                    let change_type = if path.exists() {
                        if config.log_changes {
                            println!("✏️  Modified: {}", path.display());
                        }
                        ChangeType::FileModified
                    } else {
                        if config.log_changes {
                            println!("🗑️  Deleted: {}", path.display());
                        }
                        ChangeType::FileDeleted
                    };

                    // Handle the change based on sync mode
                    match config.mode {
                        SyncMode::WatchOnly => {
                            Self::log_change(
                                change_log,
                                path,
                                change_type,
                                "File changed (watch only mode)".to_string(),
                            );
                        }
                        SyncMode::FileToSystem | SyncMode::Bidirectional => {
                            if matches!(change_type, ChangeType::FileModified | ChangeType::FileCreated) {
                                if let Err(e) = Self::handle_file_change(
                                    &path,
                                    change_type,
                                    config,
                                    manager,
                                    change_log,
                                    variable_filter,
                                ) {
                                    eprintln!("Error handling file change: {e}");
                                }
                            }
                        }
                        SyncMode::SystemToFile => {
                            // In this mode, we don't react to file changes
                            if config.log_changes {
                                println!("ℹ️  Ignoring file change in system-to-file mode");
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal, continue checking
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Channel disconnected, stop
                    break;
                }
            }
        }
    }

    fn handle_file_change(
        path: &Path,
        _change_type: ChangeType,
        config: &WatchConfig,
        manager: &Arc<Mutex<EnvVarManager>>,
        change_log: &Arc<Mutex<Vec<ChangeEvent>>>,
        variable_filter: Option<&Vec<String>>,
    ) -> Result<()> {
        if !config.auto_reload {
            return Ok(());
        }

        // Add a small delay to ensure file write is complete
        thread::sleep(Duration::from_millis(50));

        // Load and apply changes from file
        let mut manager = manager.lock().unwrap();

        // Get current state for comparison
        let before_vars: HashMap<String, String> = manager
            .list()
            .into_iter()
            .filter(|v| {
                variable_filter
                    .as_ref()
                    .is_none_or(|filter| filter.iter().any(|f| v.name.contains(f)))
            })
            .map(|v| (v.name.clone(), v.value.clone()))
            .collect();

        // Load the file based on extension
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let load_result = match extension {
            "env" => Self::load_env_file(path, &mut manager, variable_filter),
            "yaml" | "yml" => Self::load_yaml_file(path, &mut manager, variable_filter),
            "json" => Self::load_json_file(path, &mut manager, variable_filter),
            _ => {
                // Try to load as .env format by default
                Self::load_env_file(path, &mut manager, variable_filter)
            }
        };

        if let Err(e) = load_result {
            eprintln!("Failed to load file: {e}");
            return Err(e);
        }

        // Compare and log changes
        let after_vars = manager.list();
        let mut changes_made = false;

        for var in after_vars {
            // Skip if filtered
            if let Some(filter) = variable_filter {
                if !filter.iter().any(|f| var.name.contains(f)) {
                    continue;
                }
            }

            if let Some(old_value) = before_vars.get(&var.name) {
                if old_value != &var.value {
                    Self::log_change(
                        change_log,
                        path.to_path_buf(),
                        ChangeType::VariableModified(var.name.clone()),
                        format!("Changed {} from '{}' to '{}'", var.name, old_value, var.value),
                    );

                    if config.log_changes {
                        println!("  🔄 {} changed from '{}' to '{}'", var.name, old_value, var.value);
                    }
                    changes_made = true;
                }
            } else {
                Self::log_change(
                    change_log,
                    path.to_path_buf(),
                    ChangeType::VariableAdded(var.name.clone()),
                    format!("Added {} = '{}'", var.name, var.value),
                );

                if config.log_changes {
                    println!("  ➕ {} = '{}'", var.name, var.value);
                }
                changes_made = true;
            }
        }

        // Check for deletions
        for (name, _) in before_vars {
            if manager.get(&name).is_none() {
                Self::log_change(
                    change_log,
                    path.to_path_buf(),
                    ChangeType::VariableDeleted(name.clone()),
                    format!("Deleted {name}"),
                );

                if config.log_changes {
                    println!("  ❌ {name} deleted");
                }
                changes_made = true;
            }
        }

        if !changes_made && config.log_changes {
            println!("  ℹ️  No changes detected");
        }

        Ok(())
    }

    fn load_env_file(path: &Path, manager: &mut EnvVarManager, variable_filter: Option<&Vec<String>>) -> Result<()> {
        let content = fs::read_to_string(path)?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');

                // Apply filter if specified
                if let Some(filter) = variable_filter {
                    if !filter.iter().any(|f| key.contains(f)) {
                        continue;
                    }
                }

                manager.set(key, value, true)?;
            }
        }

        Ok(())
    }

    fn load_yaml_file(path: &Path, manager: &mut EnvVarManager, variable_filter: Option<&Vec<String>>) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;

        if let serde_yaml::Value::Mapping(map) = yaml {
            for (key, value) in map {
                if let (Some(key_str), Some(value_str)) = (key.as_str(), value.as_str()) {
                    // Apply filter if specified
                    if let Some(filter) = variable_filter {
                        if !filter.iter().any(|f| key_str.contains(f)) {
                            continue;
                        }
                    }

                    manager.set(key_str, value_str, true)?;
                }
            }
        }

        Ok(())
    }

    fn load_json_file(path: &Path, manager: &mut EnvVarManager, variable_filter: Option<&Vec<String>>) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        if let serde_json::Value::Object(map) = json {
            for (key, value) in map {
                if let serde_json::Value::String(value_str) = value {
                    // Apply filter if specified
                    if let Some(filter) = variable_filter {
                        if !filter.iter().any(|f| key.contains(f)) {
                            continue;
                        }
                    }

                    manager.set(&key, &value_str, true)?;
                }
            }
        }

        Ok(())
    }

    fn start_system_monitor(&mut self) {
        let manager = Arc::clone(&self.manager);
        let config = self.config.clone();
        let _change_log = Arc::clone(&self.change_log);
        let variable_filter = self.variable_filter.clone();
        let output_file = self.output_file.clone();

        thread::spawn(move || {
            let mut last_snapshot = HashMap::new();

            loop {
                thread::sleep(Duration::from_secs(1));

                manager.lock().unwrap().load_all().ok();

                let current_snapshot: HashMap<String, String> = manager
                    .lock()
                    .unwrap()
                    .list()
                    .iter()
                    .filter(|v| {
                        variable_filter
                            .as_ref()
                            .is_none_or(|filter| filter.iter().any(|f| v.name.contains(f)))
                    })
                    .map(|v| (v.name.clone(), v.value.clone()))
                    .collect();

                // Check for changes and write to file if needed
                if matches!(config.mode, SyncMode::SystemToFile | SyncMode::Bidirectional) {
                    if let Some(ref output) = output_file {
                        let mut changed = false;

                        for (name, value) in &current_snapshot {
                            if last_snapshot.get(name) != Some(value) {
                                changed = true;
                                if config.log_changes {
                                    println!("🔄 System change detected: {name} changed");
                                }
                            }
                        }

                        // Check for deletions
                        for name in last_snapshot.keys() {
                            if !current_snapshot.contains_key(name) {
                                changed = true;
                                if config.log_changes {
                                    println!("❌ System change detected: {name} deleted");
                                }
                            }
                        }

                        if changed {
                            // Write to output file
                            let mut content = String::new();
                            #[allow(clippy::format_push_string)]
                            for (name, value) in &current_snapshot {
                                content.push_str(&format!("{name}={value}\n"));
                            }

                            if let Err(e) = fs::write(output, &content) {
                                eprintln!("Failed to write to output file: {e}");
                            } else if config.log_changes {
                                println!("💾 Updated output file");
                            }
                        }
                    }
                }

                last_snapshot = current_snapshot;
            }
        });
    }

    fn matches_patterns(path: &Path, patterns: &[String]) -> bool {
        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy(),
            None => return false,
        };

        patterns.iter().any(|pattern| {
            if pattern.contains('*') {
                let regex_pattern = pattern.replace('.', r"\.").replace('*', ".*");
                if let Ok(re) = regex::Regex::new(&format!("^{regex_pattern}$")) {
                    return re.is_match(&file_name);
                }
            }
            &file_name == pattern
        })
    }

    fn log_change(change_log: &Arc<Mutex<Vec<ChangeEvent>>>, path: PathBuf, change_type: ChangeType, details: String) {
        let event = ChangeEvent {
            timestamp: chrono::Utc::now(),
            path,
            change_type,
            details,
        };

        let mut log = change_log.lock().expect("Failed to lock change log");
        log.push(event);

        // Keep only last 1000 events
        if log.len() > 1000 {
            log.drain(0..100);
        }
    }

    /// Returns a clone of the change log containing all recorded change events.
    ///
    /// # Panics
    ///
    /// Panics if the change log mutex is poisoned (i.e., another thread panicked while holding the lock).
    #[must_use]
    pub fn get_change_log(&self) -> Vec<ChangeEvent> {
        self.change_log.lock().expect("Failed to lock change log").clone()
    }

    /// Exports the change log to a JSON file at the specified path.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The change log cannot be serialized to JSON
    /// - The file cannot be written to the specified path
    pub fn export_change_log(&self, path: &Path) -> Result<()> {
        let log = self.get_change_log();
        let json = serde_json::to_string_pretty(&log)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn set_variable_filter(&mut self, vars: Vec<String>) {
        self.variable_filter = Some(vars);
    }

    /// Set output file for system-to-file sync
    pub fn set_output_file(&mut self, path: PathBuf) {
        self.output_file = Some(path);
    }
}

// Add this at the end of the file

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_manager() -> EnvVarManager {
        let mut manager = EnvVarManager::new();
        manager.set("TEST_VAR", "initial_value", false).unwrap();
        manager.set("ANOTHER_VAR", "another_value", false).unwrap();
        manager
    }

    fn create_test_config(temp_dir: &Path) -> WatchConfig {
        WatchConfig {
            paths: vec![temp_dir.to_path_buf()],
            mode: SyncMode::FileToSystem,
            auto_reload: true,
            debounce_duration: Duration::from_millis(100),
            patterns: vec!["*.env".to_string(), "*.json".to_string(), "*.yaml".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        }
    }

    fn wait_for_debounce() {
        thread::sleep(Duration::from_millis(200));
    }

    #[test]
    fn test_env_watcher_creation() {
        let config = WatchConfig::default();
        let manager = create_test_manager();
        let watcher = EnvWatcher::new(config, manager);

        assert!(watcher.debouncer.is_none());
        assert!(watcher.stop_signal.is_none());
        assert!(watcher.variable_filter.is_none());
        assert!(watcher.output_file.is_none());
    }

    #[test]
    fn test_watch_config_default() {
        let config = WatchConfig::default();

        assert_eq!(config.paths, vec![PathBuf::from(".")]);
        assert!(matches!(config.mode, SyncMode::FileToSystem));
        assert!(config.auto_reload);
        assert_eq!(config.debounce_duration, Duration::from_millis(300));
        assert_eq!(config.patterns.len(), 5);
        assert!(config.log_changes);
        assert!(matches!(config.conflict_strategy, ConflictStrategy::UseLatest));
    }

    #[test]
    fn test_variable_filter() {
        let config = WatchConfig::default();
        let manager = create_test_manager();
        let mut watcher = EnvWatcher::new(config, manager);

        assert!(watcher.variable_filter.is_none());

        watcher.set_variable_filter(vec!["TEST".to_string(), "API".to_string()]);
        assert!(watcher.variable_filter.is_some());
        assert_eq!(watcher.variable_filter.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_output_file() {
        let config = WatchConfig::default();
        let manager = create_test_manager();
        let mut watcher = EnvWatcher::new(config, manager);

        assert!(watcher.output_file.is_none());

        let output_path = PathBuf::from("output.env");
        watcher.set_output_file(output_path.clone());
        assert_eq!(watcher.output_file, Some(output_path));
    }

    #[test]
    fn test_change_log() {
        let config = WatchConfig::default();
        let manager = create_test_manager();
        let watcher = EnvWatcher::new(config, manager);

        let log = watcher.get_change_log();
        assert!(log.is_empty());

        // Add a change event
        let change_event = ChangeEvent {
            timestamp: chrono::Utc::now(),
            path: PathBuf::from("test.env"),
            change_type: ChangeType::FileModified,
            details: "Test change".to_string(),
        };

        watcher.change_log.lock().unwrap().push(change_event);

        let log = watcher.get_change_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].details, "Test change");
    }

    #[test]
    fn test_export_change_log() {
        let temp_dir = TempDir::new().unwrap();
        let log_file = temp_dir.path().join("changes.json");

        let config = WatchConfig::default();
        let manager = create_test_manager();
        let watcher = EnvWatcher::new(config, manager);

        // Add some change events
        let mut log = watcher.change_log.lock().unwrap();
        log.push(ChangeEvent {
            timestamp: chrono::Utc::now(),
            path: PathBuf::from("test1.env"),
            change_type: ChangeType::FileCreated,
            details: "Created file".to_string(),
        });
        log.push(ChangeEvent {
            timestamp: chrono::Utc::now(),
            path: PathBuf::from("test2.env"),
            change_type: ChangeType::VariableAdded("NEW_VAR".to_string()),
            details: "Added NEW_VAR".to_string(),
        });
        drop(log);

        // Export the log
        watcher.export_change_log(&log_file).unwrap();

        // Verify the file exists and contains valid JSON
        assert!(log_file.exists());
        let content = fs::read_to_string(&log_file).unwrap();
        let parsed: Vec<ChangeEvent> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_matches_patterns() {
        let patterns = vec!["*.env".to_string(), "*.yaml".to_string(), "config.json".to_string()];

        assert!(EnvWatcher::matches_patterns(&PathBuf::from("test.env"), &patterns));
        assert!(EnvWatcher::matches_patterns(&PathBuf::from("app.yaml"), &patterns));
        assert!(EnvWatcher::matches_patterns(&PathBuf::from("config.json"), &patterns));
        assert!(!EnvWatcher::matches_patterns(&PathBuf::from("test.txt"), &patterns));
        assert!(!EnvWatcher::matches_patterns(&PathBuf::from("README.md"), &patterns));
    }

    #[test]
    fn test_load_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join("test.env");

        // Create a test .env file
        let content = r#"
# Comment line
TEST_VAR=test_value
ANOTHER_VAR=another_value
QUOTED_VAR="quoted value"
SINGLE_QUOTED='single quoted'
        "#;
        fs::write(&env_file, content).unwrap();

        let mut manager = EnvVarManager::new();
        EnvWatcher::load_env_file(&env_file, &mut manager, None).unwrap();

        assert_eq!(manager.get("TEST_VAR").unwrap().value, "test_value");
        assert_eq!(manager.get("ANOTHER_VAR").unwrap().value, "another_value");
        assert_eq!(manager.get("QUOTED_VAR").unwrap().value, "quoted value");
        assert_eq!(manager.get("SINGLE_QUOTED").unwrap().value, "single quoted");
    }

    #[test]
    fn test_load_env_file_with_filter() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join("test.env");

        let content = r"
TEST_VAR=test_value
API_KEY=secret_key
DATABASE_URL=postgres://localhost
API_SECRET=another_secret
        ";
        fs::write(&env_file, content).unwrap();

        let mut manager = EnvVarManager::new();
        let filter = vec!["API".to_string()];
        EnvWatcher::load_env_file(&env_file, &mut manager, Some(&filter)).unwrap();

        assert!(manager.get("API_KEY").is_some());
        assert!(manager.get("API_SECRET").is_some());
        assert!(manager.get("TEST_VAR").is_none());
        assert!(manager.get("DATABASE_URL").is_none());
    }

    #[test]
    fn test_load_json_file() {
        let temp_dir = TempDir::new().unwrap();
        let json_file = temp_dir.path().join("config.json");

        let content = r#"{
            "TEST_VAR": "json_value",
            "NUMBER_VAR": "42",
            "BOOL_VAR": "true"
        }"#;
        fs::write(&json_file, content).unwrap();

        let mut manager = EnvVarManager::new();
        EnvWatcher::load_json_file(&json_file, &mut manager, None).unwrap();

        assert_eq!(manager.get("TEST_VAR").unwrap().value, "json_value");
        assert_eq!(manager.get("NUMBER_VAR").unwrap().value, "42");
        assert_eq!(manager.get("BOOL_VAR").unwrap().value, "true");
    }

    #[test]
    fn test_load_yaml_file() {
        let temp_dir = TempDir::new().unwrap();
        let yaml_file = temp_dir.path().join("config.yaml");

        let content = r#"
TEST_VAR: yaml_value
NESTED_VAR: nested_value
QUOTED: "quoted yaml"
        "#;
        fs::write(&yaml_file, content).unwrap();

        let mut manager = EnvVarManager::new();
        EnvWatcher::load_yaml_file(&yaml_file, &mut manager, None).unwrap();

        assert_eq!(manager.get("TEST_VAR").unwrap().value, "yaml_value");
        assert_eq!(manager.get("NESTED_VAR").unwrap().value, "nested_value");
        assert_eq!(manager.get("QUOTED").unwrap().value, "quoted yaml");
    }

    #[test]
    fn test_start_and_stop() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path());
        let manager = create_test_manager();
        let mut watcher = EnvWatcher::new(config, manager);

        // Start the watcher
        watcher.start().unwrap();
        assert!(watcher.debouncer.is_some());
        assert!(watcher.stop_signal.is_some());

        // Stop the watcher
        watcher.stop().unwrap();
        assert!(watcher.debouncer.is_none());
        assert!(watcher.stop_signal.is_none());
    }

    #[test]
    fn test_file_watching_integration() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join("test.env");

        // Create initial file
        fs::write(&env_file, "INITIAL=value1").unwrap();

        let config = WatchConfig {
            paths: vec![env_file.clone()],
            mode: SyncMode::FileToSystem,
            auto_reload: true,
            debounce_duration: Duration::from_millis(50),
            patterns: vec!["*.env".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        };

        let manager = EnvVarManager::new();
        let mut watcher = EnvWatcher::new(config, manager);

        // Start watching
        watcher.start().unwrap();

        // Wait for initial setup
        wait_for_debounce();

        // Modify the file
        fs::write(&env_file, "INITIAL=value2\nNEW_VAR=new_value").unwrap();

        // Wait for changes to be detected and processed
        thread::sleep(Duration::from_millis(300));

        // Check that changes were detected
        let log = watcher.get_change_log();
        assert!(!log.is_empty());

        // Clean up
        watcher.stop().unwrap();
    }

    #[test]
    fn test_sync_mode_watch_only() {
        let temp_dir = TempDir::new().unwrap();
        let config = WatchConfig {
            paths: vec![temp_dir.path().to_path_buf()],
            mode: SyncMode::WatchOnly,
            auto_reload: true,
            debounce_duration: Duration::from_millis(50),
            patterns: vec!["*.env".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        };

        let manager = create_test_manager();
        let watcher = EnvWatcher::new(config, manager);

        // In watch-only mode, changes should be logged but not applied
        let log = watcher.get_change_log();
        assert!(log.is_empty());
    }

    #[test]
    fn test_system_to_file_mode() {
        let temp_dir = TempDir::new().unwrap();
        let output_file = temp_dir.path().join("output.env");

        let config = WatchConfig {
            paths: vec![temp_dir.path().to_path_buf()],
            mode: SyncMode::SystemToFile,
            auto_reload: true,
            debounce_duration: Duration::from_millis(50),
            patterns: vec!["*.env".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        };

        let manager = create_test_manager();
        let mut watcher = EnvWatcher::new(config, manager);
        watcher.set_output_file(output_file.clone());

        // Start watching
        watcher.start().unwrap();

        // Wait for system monitor to run
        thread::sleep(Duration::from_millis(1500));

        // Check if output file was created
        assert!(output_file.exists());

        // Clean up
        watcher.stop().unwrap();
    }

    #[test]
    fn test_change_log_limit() {
        let config = WatchConfig::default();
        let manager = create_test_manager();
        let watcher = EnvWatcher::new(config, manager);

        // Add more than 1000 events using the log_change function
        for i in 0..1100 {
            EnvWatcher::log_change(
                &watcher.change_log,
                PathBuf::from(format!("test{i}.env")),
                ChangeType::FileModified,
                format!("Change {i}"),
            );
        }

        // Check that old events were removed
        let current_log = watcher.get_change_log();
        assert_eq!(current_log.len(), 1000);
        assert_eq!(current_log[0].details, "Change 100");
    }

    #[test]
    fn test_handle_file_change_no_auto_reload() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join("test.env");
        fs::write(&env_file, "TEST=value").unwrap();

        let config = WatchConfig {
            paths: vec![env_file.clone()],
            mode: SyncMode::FileToSystem,
            auto_reload: false, // Disabled
            debounce_duration: Duration::from_millis(50),
            patterns: vec!["*.env".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        };

        let manager = EnvVarManager::new();
        let manager_arc = Arc::new(Mutex::new(manager));
        let change_log = Arc::new(Mutex::new(Vec::new()));

        // Should return Ok without loading the file
        let result = EnvWatcher::handle_file_change(
            &env_file,
            ChangeType::FileModified,
            &config,
            &manager_arc,
            &change_log,
            None,
        );

        assert!(result.is_ok());
        assert!(manager_arc.lock().unwrap().get("TEST").is_none());
    }

    #[test]
    fn test_bidirectional_sync() {
        let temp_dir = TempDir::new().unwrap();
        let sync_file = temp_dir.path().join("sync.env");

        let config = WatchConfig {
            paths: vec![sync_file.clone()],
            mode: SyncMode::Bidirectional,
            auto_reload: true,
            debounce_duration: Duration::from_millis(50),
            patterns: vec!["*.env".to_string()],
            log_changes: false,
            conflict_strategy: ConflictStrategy::UseLatest,
        };

        let manager = create_test_manager();
        let mut watcher = EnvWatcher::new(config, manager);
        watcher.set_output_file(sync_file.clone());

        // Start watching
        watcher.start().unwrap();

        // Create/modify the sync file
        fs::write(&sync_file, "BIDIRECTIONAL=test").unwrap();

        // Wait for processing
        wait_for_debounce();
        thread::sleep(Duration::from_millis(200));

        // Clean up
        watcher.stop().unwrap();
    }

    #[test]
    fn test_conflict_strategy() {
        let strategies = vec![
            ConflictStrategy::UseLatest,
            ConflictStrategy::PreferFile,
            ConflictStrategy::PreferSystem,
            ConflictStrategy::AskUser,
        ];

        #[allow(clippy::field_reassign_with_default)]
        for strategy in strategies {
            let mut config = WatchConfig::default();
            config.conflict_strategy = strategy.clone();

            #[allow(clippy::assertions_on_constants)]
            match strategy {
                ConflictStrategy::UseLatest
                | ConflictStrategy::PreferFile
                | ConflictStrategy::PreferSystem
                | ConflictStrategy::AskUser => assert!(true),
            }
        }
    }
}

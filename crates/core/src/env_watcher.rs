use crate::EnvVarManager;
use color_eyre::Result;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEvent, Debouncer, new_debouncer};
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChangeEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub details: String,
}

#[derive(Debug, Clone, serde::Serialize)]
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

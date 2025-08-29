use chrono::Local;
use clap::Args;
use clap::ValueEnum;
use color_eyre::Result;
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use envx_core::EnvVarManager;
use envx_core::EnvVarSource;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    /// Live terminal output
    Live,
    /// Compact output
    Compact,
    /// JSON lines format
    JsonLines,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SourceFilter {
    #[value(name = "system")]
    System,
    #[value(name = "user")]
    User,
    #[value(name = "process")]
    Process,
    #[value(name = "shell")]
    Shell,
}

impl From<SourceFilter> for EnvVarSource {
    fn from(filter: SourceFilter) -> Self {
        match filter {
            SourceFilter::System => EnvVarSource::System,
            SourceFilter::User => EnvVarSource::User,
            SourceFilter::Process => EnvVarSource::Process,
            SourceFilter::Shell => EnvVarSource::Shell,
        }
    }
}

#[derive(Args)]
pub struct MonitorArgs {
    /// Variables to monitor (monitor all if not specified)
    #[arg(value_name = "VARIABLE")]
    pub vars: Vec<String>,

    /// Log file path
    #[arg(short, long)]
    pub log: Option<PathBuf>,

    /// Show only changes (hide unchanged variables)
    #[arg(long)]
    pub changes_only: bool,

    /// Filter by source
    #[arg(short, long, value_enum)]
    pub source: Option<SourceFilter>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "live")]
    pub format: OutputFormat,

    /// Check interval in seconds
    #[arg(long, default_value = "2")]
    pub interval: u64,

    /// Show initial state
    #[arg(long)]
    pub show_initial: bool,

    /// Export report on exit
    #[arg(long)]
    pub export_report: Option<PathBuf>,
}

struct MonitorState {
    initial: HashMap<String, String>,
    current: HashMap<String, String>,
    changes: Vec<ChangeRecord>,
    start_time: chrono::DateTime<Local>,
}

#[derive(Debug, Clone, Serialize)]
struct ChangeRecord {
    timestamp: chrono::DateTime<Local>,
    variable: String,
    change_type: String,
    old_value: Option<String>,
    new_value: Option<String>,
}

/// Handles the monitor command to track environment variable changes.
///
/// # Errors
///
/// Returns an error if:
/// - Failed to load environment variables
/// - Failed to set up Ctrl+C handler
/// - Failed to write to log file (if specified)
/// - Failed to export report (if specified)
pub fn handle_monitor(args: MonitorArgs) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    let mut state = MonitorState {
        initial: collect_variables(&manager, &args),
        current: HashMap::new(),
        changes: Vec::new(),
        start_time: Local::now(),
    };

    print_monitor_header(&args);

    if args.show_initial {
        print_initial_state(&state.initial);
    }

    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    // Monitoring loop
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(args.interval));

        let mut current_manager = EnvVarManager::new();
        current_manager.load_all()?;

        state.current = collect_variables(&current_manager, &args);

        let changes = detect_changes(&state);

        if !changes.is_empty() || !args.changes_only {
            display_changes(&changes, &args);

            // Log changes
            for change in changes {
                state.changes.push(change.clone());

                if let Some(log_path) = &args.log {
                    log_change(log_path, &change)?;
                }
            }
        }

        // Update state for next iteration
        for (name, value) in &state.current {
            state.initial.insert(name.clone(), value.clone());
        }
    }

    // Generate final report if requested
    if let Some(report_path) = args.export_report {
        export_report(&state, &report_path)?;
        println!("\n📊 Report exported to: {}", report_path.display());
    }

    print_monitor_summary(&state);

    Ok(())
}

fn collect_variables(manager: &EnvVarManager, args: &MonitorArgs) -> HashMap<String, String> {
    manager
        .list()
        .into_iter()
        .filter(|var| {
            // Filter by variable names if specified
            (args.vars.is_empty() || args.vars.iter().any(|v| var.name.contains(v))) &&
            // Filter by source if specified
            (args.source.is_none() || args.source.as_ref().map(|s| EnvVarSource::from(s.clone())) == Some(var.source.clone()))
        })
        .map(|var| (var.name.clone(), var.value.clone()))
        .collect()
}

fn detect_changes(state: &MonitorState) -> Vec<ChangeRecord> {
    let mut changes = Vec::new();
    let timestamp = Local::now();

    // Check for modifications and additions
    for (name, value) in &state.current {
        match state.initial.get(name) {
            Some(old_value) if old_value != value => {
                changes.push(ChangeRecord {
                    timestamp,
                    variable: name.clone(),
                    change_type: "modified".to_string(),
                    old_value: Some(old_value.clone()),
                    new_value: Some(value.clone()),
                });
            }
            None => {
                changes.push(ChangeRecord {
                    timestamp,
                    variable: name.clone(),
                    change_type: "added".to_string(),
                    old_value: None,
                    new_value: Some(value.clone()),
                });
            }
            _ => {} // No change
        }
    }

    // Check for deletions
    for (name, value) in &state.initial {
        if !state.current.contains_key(name) {
            changes.push(ChangeRecord {
                timestamp,
                variable: name.clone(),
                change_type: "deleted".to_string(),
                old_value: Some(value.clone()),
                new_value: None,
            });
        }
    }

    changes
}

fn display_changes(changes: &[ChangeRecord], args: &MonitorArgs) {
    match args.format {
        OutputFormat::Live => {
            for change in changes {
                let time = change.timestamp.format("%H:%M:%S");
                match change.change_type.as_str() {
                    "added" => {
                        println!(
                            "[{}] ➕ {} = '{}'",
                            time,
                            change.variable,
                            change.new_value.as_ref().unwrap_or(&String::new())
                        );
                    }
                    "modified" => {
                        println!(
                            "[{}] 🔄 {} changed from '{}' to '{}'",
                            time,
                            change.variable,
                            change.old_value.as_ref().unwrap_or(&String::new()),
                            change.new_value.as_ref().unwrap_or(&String::new())
                        );
                    }
                    "deleted" => {
                        println!(
                            "[{}] ❌ {} deleted (was: '{}')",
                            time,
                            change.variable,
                            change.old_value.as_ref().unwrap_or(&String::new())
                        );
                    }
                    _ => {}
                }
            }
        }
        OutputFormat::Compact => {
            for change in changes {
                println!(
                    "{} {} {}",
                    change.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    change.change_type.to_uppercase(),
                    change.variable
                );
            }
        }
        OutputFormat::JsonLines => {
            for change in changes {
                if let Ok(json) = serde_json::to_string(change) {
                    println!("{json}");
                }
            }
        }
    }
}

fn log_change(path: &PathBuf, change: &ChangeRecord) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    writeln!(file, "{}", serde_json::to_string(change)?)?;
    Ok(())
}

fn print_monitor_header(args: &MonitorArgs) {
    println!("📊 Environment Variable Monitor");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if args.vars.is_empty() {
        println!("Monitoring: All variables");
    } else {
        println!("Monitoring: {}", args.vars.join(", "));
    }

    if let Some(source) = &args.source {
        println!("Source filter: {source:?}");
    }

    println!("Check interval: {} seconds", args.interval);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Press Ctrl+C to stop\n");
}

fn print_initial_state(vars: &HashMap<String, String>) {
    if vars.is_empty() {
        println!("No variables match the criteria\n");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Variable", "Initial Value"]);

    for (name, value) in vars {
        let display_value = if value.len() > 50 {
            format!("{}...", &value[..47])
        } else {
            value.clone()
        };
        table.add_row(vec![name.clone(), display_value]);
    }

    println!("Initial State:\n{table}\n");
}

fn print_monitor_summary(state: &MonitorState) {
    let duration = Local::now().signed_duration_since(state.start_time);

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Monitoring Summary");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Duration: {}", format_duration(duration));
    println!("Total changes: {}", state.changes.len());

    let mut added = 0;
    let mut modified = 0;
    let mut deleted = 0;

    for change in &state.changes {
        match change.change_type.as_str() {
            "added" => added += 1,
            "modified" => modified += 1,
            "deleted" => deleted += 1,
            _ => {}
        }
    }

    println!("  ➕ Added: {added}");
    println!("  🔄 Modified: {modified}");
    println!("  ❌ Deleted: {deleted}");
}

fn format_duration(duration: chrono::Duration) -> String {
    let hours = duration.num_hours();
    let minutes = duration.num_minutes() % 60;
    let seconds = duration.num_seconds() % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn export_report(state: &MonitorState, path: &PathBuf) -> Result<()> {
    #[derive(Serialize)]
    struct Report {
        start_time: chrono::DateTime<Local>,
        end_time: chrono::DateTime<Local>,
        duration_seconds: i64,
        total_changes: usize,
        changes_by_type: HashMap<String, usize>,
        changes: Vec<ChangeRecord>,
    }

    let mut changes_by_type = HashMap::new();
    for change in &state.changes {
        *changes_by_type.entry(change.change_type.clone()).or_insert(0) += 1;
    }

    let report = Report {
        start_time: state.start_time,
        end_time: Local::now(),
        duration_seconds: Local::now().signed_duration_since(state.start_time).num_seconds(),
        total_changes: state.changes.len(),
        changes_by_type,
        changes: state.changes.clone(),
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, json)?;

    Ok(())
}

use std::{path::PathBuf, time::Duration};

use clap::{Args, ValueEnum};
use color_eyre::Result;
use envx_core::{ConflictStrategy, EnvVarManager, EnvWatcher, SyncMode, WatchConfig};

#[derive(Debug, Clone, ValueEnum)]
pub enum Direction {
    /// Sync from files to system (default)
    FileToSystem,
    /// Sync from system to files
    SystemToFile,
    /// Bidirectional synchronization
    Bidirectional,
}

#[derive(Args, Clone)]
pub struct WatchArgs {
    /// Files or directories to watch (defaults to current directory)
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Sync direction
    #[arg(short, long, value_enum, default_value = "file-to-system")]
    pub direction: Direction,

    /// Output file for system-to-file sync
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// File patterns to watch
    #[arg(short, long)]
    pub pattern: Vec<String>,

    /// Debounce duration in milliseconds
    #[arg(long, default_value = "300")]
    pub debounce: u64,

    /// Log changes to file
    #[arg(short, long)]
    pub log: Option<PathBuf>,

    /// Variables to sync (sync all if not specified)
    #[arg(short, long)]
    pub vars: Vec<String>,

    /// Quiet mode - less output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Handle file watching and synchronization operations.
///
/// # Errors
///
/// This function will return an error if:
/// - Required output file is not specified for system-to-file or bidirectional sync
/// - Environment variable manager operations fail (loading, setting)
/// - Profile or project manager initialization fails
/// - File watcher creation or operation fails
/// - File I/O operations fail during synchronization
/// - Ctrl+C signal handler setup fails
/// - Change log export operations fail
/// - Invalid watch configuration is provided
/// - File system permissions prevent watching or writing to specified paths
pub fn handle_watch(args: &WatchArgs) -> Result<()> {
    // Validate arguments
    if matches!(args.direction, Direction::SystemToFile | Direction::Bidirectional) && args.output.is_none() {
        return Err(color_eyre::eyre::eyre!(
            "Output file required for system-to-file synchronization. Use --output <file>"
        ));
    }

    let sync_mode = match args.direction {
        Direction::FileToSystem => SyncMode::FileToSystem,
        Direction::SystemToFile => SyncMode::SystemToFile,
        Direction::Bidirectional => SyncMode::Bidirectional,
    };

    let mut config = WatchConfig {
        paths: if args.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            args.paths.clone()
        },
        mode: sync_mode,
        auto_reload: true,
        debounce_duration: Duration::from_millis(args.debounce),
        log_changes: !args.quiet,
        conflict_strategy: ConflictStrategy::UseLatest,
        ..Default::default()
    };

    if !args.pattern.is_empty() {
        config.patterns.clone_from(&args.pattern);
    }

    // Add output file to watch paths if bidirectional
    if let Some(output) = &args.output {
        if matches!(args.direction, Direction::Bidirectional) {
            config.paths.push(output.clone());
        }
    }

    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    let mut watcher = EnvWatcher::new(config.clone(), manager);

    // Set up the watcher with variable filtering
    if !args.vars.is_empty() {
        watcher.set_variable_filter(args.vars.clone());
    }

    if let Some(output) = args.output.clone() {
        watcher.set_output_file(output);
    }

    print_watch_header(args, &config);

    watcher.start()?;

    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    // Keep running until Ctrl+C
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(1));

        if let Some(log_file) = &args.log {
            let _ = watcher.export_change_log(log_file);
        }
    }

    watcher.stop()?;
    println!("\n✅ Watch mode stopped");

    Ok(())
}

fn print_watch_header(args: &WatchArgs, config: &WatchConfig) {
    println!("🔄 Starting envx watch mode");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    match args.direction {
        Direction::FileToSystem => {
            println!("📂 → 💻 Syncing from files to system");
            println!(
                "Watching: {}",
                config
                    .paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Direction::SystemToFile => {
            println!("💻 → 📂 Syncing from system to file");
            if let Some(output) = &args.output {
                println!("Output: {}", output.display());
            }
        }
        Direction::Bidirectional => {
            println!("📂 ↔️ 💻 Bidirectional sync");
            println!(
                "Watching: {}",
                config
                    .paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Some(output) = &args.output {
                println!("Output: {}", output.display());
            }
        }
    }

    if !args.vars.is_empty() {
        println!("Variables: {}", args.vars.join(", "));
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Press Ctrl+C to stop\n");
}

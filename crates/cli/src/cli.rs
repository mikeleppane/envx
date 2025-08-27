use clap::Args;
use clap::{Parser, Subcommand};
use color_eyre::Result;
use color_eyre::eyre::eyre;
use comfy_table::Attribute;
use comfy_table::Cell;
use comfy_table::Color;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use console::Term;
use console::style;
use envx_core::PathManager;
use envx_core::profile_manager::ProfileManager;
use envx_core::snapshot_manager::SnapshotManager;
use envx_core::{Analyzer, EnvVarManager, ExportFormat, Exporter, ImportFormat, Importer};
use std::io::Write;
use std::path::Path;
#[derive(Parser)]
#[command(name = "envx")]
#[command(about = "System Environment Variable Manager")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List environment variables
    List {
        /// Filter by source (system, user, process, shell)
        #[arg(short, long)]
        source: Option<String>,

        /// Search query
        #[arg(short = 'q', long)]
        query: Option<String>,

        /// Output format (json, table, simple, compact)
        #[arg(short, long, default_value = "table")]
        format: String,

        /// Sort by (name, value, source)
        #[arg(long, default_value = "name")]
        sort: String,

        /// Show only variable names
        #[arg(long)]
        names_only: bool,

        /// Limit output to N entries
        #[arg(short, long)]
        limit: Option<usize>,

        /// Show statistics summary
        #[arg(long)]
        stats: bool,
    },

    /// Get a specific environment variable
    Get {
        /// Variable name or pattern (supports *, ?, and /regex/)
        /// Examples:
        ///   envx get PATH           - exact match
        ///   envx get PATH*          - starts with PATH
        ///   envx get *PATH          - ends with PATH
        ///   envx get *PATH*         - contains PATH
        ///   envx get P?TH           - P followed by any char, then TH
        ///   envx get /^JAVA.*/      - regex pattern
        pattern: String,

        /// Output format (simple, detailed, json)
        #[arg(short, long, default_value = "simple")]
        format: String,
    },

    /// Set an environment variable
    Set {
        /// Variable name
        name: String,

        /// Variable value
        value: String,

        /// Set as temporary (only for current session)
        #[arg(short, long)]
        temporary: bool,
    },

    /// Delete environment variable(s)
    Delete {
        /// Variable name or pattern
        pattern: String,

        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Analyze environment variables
    Analyze {
        /// Type of analysis (duplicates, invalid)
        #[arg(short, long, default_value = "all")]
        analysis_type: String,
    },

    /// Launch the TUI
    #[command(visible_alias = "ui")]
    Tui,

    /// Manage PATH variable
    Path {
        #[command(subcommand)]
        action: Option<PathAction>,

        /// Check if all paths exist
        #[arg(short, long)]
        check: bool,

        /// Target PATH variable (PATH, Path, or custom like PYTHONPATH)
        #[arg(short = 'v', long, default_value = "PATH")]
        var: String,

        /// Apply changes permanently
        #[arg(short = 'p', long)]
        permanent: bool,
    },

    /// Export environment variables to a file
    Export {
        /// Output file path
        file: String,

        /// Variable names or patterns to export (exports all if not specified)
        #[arg(short = 'v', long)]
        vars: Vec<String>,

        /// Export format (auto-detect from extension, or: env, json, yaml, txt)
        #[arg(short, long)]
        format: Option<String>,

        /// Include only specific sources (system, user, process, shell)
        #[arg(short, long)]
        source: Option<String>,

        /// Include metadata (source, modified time)
        #[arg(short, long)]
        metadata: bool,

        /// Overwrite existing file without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Import environment variables from a file
    Import {
        /// Input file path
        file: String,

        /// Variable names or patterns to import (imports all if not specified)
        #[arg(short = 'v', long)]
        vars: Vec<String>,

        /// Import format (auto-detect from extension, or: env, json, yaml, txt)
        #[arg(short, long)]
        format: Option<String>,

        /// Make imported variables permanent
        #[arg(short, long)]
        permanent: bool,

        /// Prefix to add to all imported variable names
        #[arg(long)]
        prefix: Option<String>,

        /// Overwrite existing variables without confirmation
        #[arg(long)]
        overwrite: bool,

        /// Dry run - show what would be imported without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Manage environment snapshots
    Snapshot(SnapshotArgs),

    /// Manage environment profiles
    Profile(ProfileArgs),
}

#[derive(Subcommand)]
pub enum PathAction {
    /// Add a directory to PATH
    Add {
        /// Directory to add
        directory: String,

        /// Add to the beginning of PATH (highest priority)
        #[arg(short, long)]
        first: bool,

        /// Create directory if it doesn't exist
        #[arg(short, long)]
        create: bool,
    },

    /// Remove a directory from PATH
    Remove {
        /// Directory to remove (supports wildcards)
        directory: String,

        /// Remove all occurrences (not just first)
        #[arg(short, long)]
        all: bool,
    },

    /// Clean invalid/non-existent entries from PATH
    Clean {
        /// Also remove duplicate entries
        #[arg(short, long)]
        dedupe: bool,

        /// Dry run - show what would be removed without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Remove duplicate entries from PATH
    Dedupe {
        /// Keep first occurrence (default is last)
        #[arg(short, long)]
        keep_first: bool,

        /// Dry run - show what would be removed
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Check PATH for issues
    Check {
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show PATH entries in order
    List {
        /// Show with index numbers
        #[arg(short, long)]
        numbered: bool,

        /// Check existence of each path
        #[arg(short, long)]
        check: bool,
    },

    /// Move a PATH entry to a different position
    Move {
        /// Path or index to move
        from: String,

        /// Target position (first, last, or index)
        to: String,
    },
}

#[derive(Args)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub command: SnapshotCommands,
}

#[derive(Subcommand)]
pub enum SnapshotCommands {
    /// Create a new snapshot
    Create {
        /// Snapshot name
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// List all snapshots
    List,
    /// Show details of a snapshot
    Show {
        /// Snapshot name or ID
        snapshot: String,
    },
    /// Restore from a snapshot
    Restore {
        /// Snapshot name or ID
        snapshot: String,
        /// Force restore without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Delete a snapshot
    Delete {
        /// Snapshot name or ID
        snapshot: String,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Compare two snapshots
    Diff {
        /// First snapshot
        snapshot1: String,
        /// Second snapshot
        snapshot2: String,
    },
}

#[derive(Args)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCommands,
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// Create a new profile
    Create {
        /// Profile name
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// List all profiles
    List,
    /// Show current or specific profile
    Show {
        /// Profile name (shows active if not specified)
        name: Option<String>,
    },
    /// Switch to a profile
    Switch {
        /// Profile name
        name: String,
        /// Apply immediately
        #[arg(short, long)]
        apply: bool,
    },
    /// Add a variable to a profile
    Add {
        /// Profile name
        profile: String,
        /// Variable name
        name: String,
        /// Variable value
        value: String,
        /// Override system variable
        #[arg(short, long)]
        override_system: bool,
    },
    /// Remove a variable from a profile
    Remove {
        /// Profile name
        profile: String,
        /// Variable name
        name: String,
    },
    /// Delete a profile
    Delete {
        /// Profile name
        name: String,
        /// Force deletion
        #[arg(short, long)]
        force: bool,
    },
    /// Export a profile
    Export {
        /// Profile name
        name: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Import a profile
    Import {
        /// Import file
        file: String,
        /// Profile name
        #[arg(short, long)]
        name: Option<String>,
        /// Overwrite if exists
        #[arg(short, long)]
        overwrite: bool,
    },
    /// Apply a profile to current environment
    Apply {
        /// Profile name
        name: String,
    },
}

/// Execute the CLI command with the given arguments.
///
/// # Errors
///
/// This function will return an error if:
/// - Environment variable operations fail (loading, setting, deleting)
/// - File I/O operations fail (import/export)
/// - User input cannot be read
/// - Invalid command arguments are provided
/// - TUI mode is requested (should be handled by main binary)
pub fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::List {
            source,
            query,
            format,
            sort,
            names_only,
            limit,
            stats,
        } => {
            handle_list_command(
                source.as_deref(),
                query.as_deref(),
                &format,
                &sort,
                names_only,
                limit,
                stats,
            )?;
        }

        Commands::Get { pattern, format } => {
            handle_get_command(&pattern, &format)?;
        }

        Commands::Set { name, value, temporary } => {
            handle_set_command(&name, &value, temporary)?;
        }

        Commands::Delete { pattern, force } => {
            handle_delete_command(&pattern, force)?;
        }

        Commands::Analyze { analysis_type } => {
            handle_analyze_command(&analysis_type)?;
        }

        Commands::Tui => {
            // Launch the TUI
            envx_tui::run()?;
        }

        Commands::Path {
            action,
            check,
            var,
            permanent,
        } => {
            handle_path_command(action, check, &var, permanent)?;
        }

        Commands::Export {
            file,
            vars,
            format,
            source,
            metadata,
            force,
        } => {
            handle_export(&file, &vars, format, source, metadata, force)?;
        }

        Commands::Import {
            file,
            vars,
            format,
            permanent,
            prefix,
            overwrite,
            dry_run,
        } => {
            handle_import(&file, &vars, format, permanent, prefix.as_ref(), overwrite, dry_run)?;
        }

        Commands::Snapshot(args) => {
            handle_snapshot(args)?;
        }
        Commands::Profile(args) => {
            handle_profile(args)?;
        }
    }

    Ok(())
}

fn handle_get_command(pattern: &str, format: &str) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    let vars = manager.get_pattern(pattern);

    if vars.is_empty() {
        eprintln!("No variables found matching pattern: {pattern}");
        return Ok(());
    }

    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&vars)?);
        }
        "detailed" => {
            for var in vars {
                println!("Name: {}", var.name);
                println!("Value: {}", var.value);
                println!("Source: {:?}", var.source);
                println!("Modified: {}", var.modified.format("%Y-%m-%d %H:%M:%S"));
                if let Some(orig) = &var.original_value {
                    println!("Original: {orig}");
                }
                println!("---");
            }
        }
        _ => {
            for var in vars {
                println!("{} = {}", var.name, var.value);
            }
        }
    }
    Ok(())
}

fn handle_set_command(name: &str, value: &str, temporary: bool) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    let permanent = !temporary;

    manager.set(name, value, permanent)?;
    if permanent {
        println!("✅ Set {name} = \"{value}\"");
        #[cfg(windows)]
        println!("📝 Note: You may need to restart your terminal for changes to take effect");
    } else {
        println!("⚡ Set {name} = \"{value}\" (temporary - current session only)");
    }
    Ok(())
}

fn handle_delete_command(pattern: &str, force: bool) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    // Collect the names to delete first (owned data, not references)
    let vars_to_delete: Vec<String> = manager
        .get_pattern(pattern)
        .into_iter()
        .map(|v| v.name.clone())
        .collect();

    if vars_to_delete.is_empty() {
        eprintln!("No variables found matching pattern: {pattern}");
        return Ok(());
    }

    if !force && vars_to_delete.len() > 1 {
        println!("About to delete {} variables:", vars_to_delete.len());
        for name in &vars_to_delete {
            println!("  - {name}");
        }
        print!("Continue? [y/N]: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Now we can safely delete since we're not holding any references to manager
    for name in vars_to_delete {
        manager.delete(&name)?;
        println!("Deleted: {name}");
    }
    Ok(())
}

fn handle_analyze_command(analysis_type: &str) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;
    let vars = manager.list().into_iter().cloned().collect();
    let analyzer = Analyzer::new(vars);

    match analysis_type {
        "duplicates" | "all" => {
            let duplicates = analyzer.find_duplicates();
            if !duplicates.is_empty() {
                println!("Duplicate variables found:");
                for (name, vars) in duplicates {
                    println!("  {}: {} instances", name, vars.len());
                }
            }
        }
        "invalid" => {
            let validation = analyzer.validate_all();
            for (name, result) in validation {
                if !result.valid {
                    println!("Invalid variable: {name}");
                    for error in result.errors {
                        println!("  Error: {error}");
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_path_command(action: Option<PathAction>, check: bool, var: &str, permanent: bool) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    // Get the PATH variable
    let path_var = manager.get(var).ok_or_else(|| eyre!("Variable '{}' not found", var))?;

    let mut path_mgr = PathManager::new(&path_var.value);

    // If no action specified, list PATH entries
    if action.is_none() {
        if check {
            handle_path_check(&path_mgr, true);
        }
        handle_path_list(&path_mgr, false, false);
    }

    let command = action.expect("Action should be Some if we reach here");
    match command {
        PathAction::Add {
            directory,
            first,
            create,
        } => {
            let path = Path::new(&directory);

            // Check if directory exists
            if !path.exists() {
                if create {
                    std::fs::create_dir_all(path)?;
                    println!("Created directory: {directory}");
                } else if !path.exists() {
                    eprintln!("Warning: Directory does not exist: {directory}");
                    print!("Add anyway? [y/N]: ");
                    std::io::stdout().flush()?;

                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;

                    if !input.trim().eq_ignore_ascii_case("y") {
                        return Ok(());
                    }
                }
            }

            // Check if already in PATH
            if path_mgr.contains(&directory) {
                println!("Directory already in {var}: {directory}");
                return Ok(());
            }

            // Add to PATH
            if first {
                path_mgr.add_first(directory.clone());
                println!("Added to beginning of {var}: {directory}");
            } else {
                path_mgr.add_last(directory.clone());
                println!("Added to end of {var}: {directory}");
            }

            // Save changes
            let new_value = path_mgr.to_string();
            manager.set(var, &new_value, permanent)?;
        }

        PathAction::Remove { directory, all } => {
            let removed = if all {
                path_mgr.remove_all(&directory)
            } else {
                path_mgr.remove_first(&directory)
            };

            if removed > 0 {
                println!("Removed {removed} occurrence(s) of: {directory}");
                let new_value = path_mgr.to_string();
                manager.set(var, &new_value, permanent)?;
            } else {
                println!("Directory not found in {var}: {directory}");
            }
        }

        PathAction::Clean { dedupe, dry_run } => {
            let invalid = path_mgr.get_invalid();
            let duplicates = if dedupe { path_mgr.get_duplicates() } else { vec![] };

            if invalid.is_empty() && duplicates.is_empty() {
                println!("No invalid or duplicate entries found in {var}");
                return Ok(());
            }

            if !invalid.is_empty() {
                println!("Invalid/non-existent paths to remove:");
                for path in &invalid {
                    println!("  - {path}");
                }
            }

            if !duplicates.is_empty() {
                println!("Duplicate paths to remove:");
                for path in &duplicates {
                    println!("  - {path}");
                }
            }

            if dry_run {
                println!("\n(Dry run - no changes made)");
            } else {
                let removed_invalid = path_mgr.remove_invalid();
                let removed_dupes = if dedupe {
                    path_mgr.deduplicate(false) // Keep last by default
                } else {
                    0
                };

                println!("Removed {removed_invalid} invalid and {removed_dupes} duplicate entries");
                let new_value = path_mgr.to_string();
                manager.set(var, &new_value, permanent)?;
            }
        }

        PathAction::Dedupe { keep_first, dry_run } => {
            let duplicates = path_mgr.get_duplicates();

            if duplicates.is_empty() {
                println!("No duplicate entries found in {var}");
                return Ok(());
            }

            println!("Duplicate paths to remove:");
            for path in &duplicates {
                println!("  - {path}");
            }
            println!(
                "Strategy: keep {} occurrence",
                if keep_first { "first" } else { "last" }
            );

            if dry_run {
                println!("\n(Dry run - no changes made)");
            } else {
                let removed = path_mgr.deduplicate(keep_first);
                println!("Removed {removed} duplicate entries");
                let new_value = path_mgr.to_string();
                manager.set(var, &new_value, permanent)?;
            }
        }

        PathAction::Check { verbose } => {
            handle_path_check(&path_mgr, verbose);
        }

        PathAction::List { numbered, check } => {
            handle_path_list(&path_mgr, numbered, check);
        }

        PathAction::Move { from, to } => {
            // Parse from (can be index or path)
            let from_idx = if let Ok(idx) = from.parse::<usize>() {
                idx
            } else {
                path_mgr
                    .find_index(&from)
                    .ok_or_else(|| eyre!("Path not found: {}", from))?
            };

            // Parse to (can be "first", "last", or index)
            let to_idx = match to.as_str() {
                "first" => 0,
                "last" => path_mgr.len() - 1,
                _ => to.parse::<usize>().map_err(|_| eyre!("Invalid position: {}", to))?,
            };

            path_mgr.move_entry(from_idx, to_idx)?;
            println!("Moved entry from position {from_idx} to {to_idx}");

            let new_value = path_mgr.to_string();
            manager.set(var, &new_value, permanent)?;
        }
    }

    Ok(())
}

fn handle_path_check(path_mgr: &PathManager, verbose: bool) {
    let entries = path_mgr.entries();
    let mut issues = Vec::new();
    let mut valid_count = 0;

    for (idx, entry) in entries.iter().enumerate() {
        let path = Path::new(entry);
        let exists = path.exists();
        let is_dir = path.is_dir();

        if verbose || !exists {
            let status = if !exists {
                issues.push(format!("Not found: {entry}"));
                "❌ NOT FOUND"
            } else if !is_dir {
                issues.push(format!("Not a directory: {entry}"));
                "⚠️  NOT DIR"
            } else {
                valid_count += 1;
                "✓ OK"
            };

            if verbose {
                println!("[{idx:3}] {status} - {entry}");
            }
        } else if exists && is_dir {
            valid_count += 1;
        }
    }

    // Summary
    println!("\nPATH Analysis:");
    println!("  Total entries: {}", entries.len());
    println!("  Valid entries: {valid_count}");

    let duplicates = path_mgr.get_duplicates();
    if !duplicates.is_empty() {
        println!("  Duplicates: {} entries", duplicates.len());
        if verbose {
            for dup in &duplicates {
                println!("    - {dup}");
            }
        }
    }

    let invalid = path_mgr.get_invalid();
    if !invalid.is_empty() {
        println!("  Invalid entries: {}", invalid.len());
        if verbose {
            for inv in &invalid {
                println!("    - {inv}");
            }
        }
    }

    if issues.is_empty() {
        println!("\n✅ No issues found!");
    } else {
        println!("\n⚠️  {} issue(s) found", issues.len());
        if !verbose {
            println!("Run with --verbose for details");
        }
    }
}

fn handle_path_list(path_mgr: &PathManager, numbered: bool, check: bool) {
    let entries = path_mgr.entries();

    if entries.is_empty() {
        println!("PATH is empty");
    }

    for (idx, entry) in entries.iter().enumerate() {
        let prefix = if numbered { format!("[{idx:3}] ") } else { String::new() };

        let suffix = if check {
            let path = Path::new(entry);
            if !path.exists() {
                " [NOT FOUND]"
            } else if !path.is_dir() {
                " [NOT A DIRECTORY]"
            } else {
                ""
            }
        } else {
            ""
        };

        println!("{prefix}{entry}{suffix}");
    }
}

fn handle_export(
    file: &str,
    vars: &[String],
    format: Option<String>,
    source: Option<String>,
    metadata: bool,
    force: bool,
) -> Result<()> {
    // Check if file exists
    if Path::new(&file).exists() && !force {
        print!("File '{file}' already exists. Overwrite? [y/N]: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Export cancelled.");
            return Ok(());
        }
    }

    // Load environment variables
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    // Filter variables to export
    let mut vars_to_export = if vars.is_empty() {
        manager.list().into_iter().cloned().collect()
    } else {
        let mut selected = Vec::new();
        for pattern in vars {
            let matched = manager.get_pattern(pattern);
            selected.extend(matched.into_iter().cloned());
        }
        selected
    };

    // Filter by source if specified
    if let Some(src) = source {
        let source_filter = match src.as_str() {
            "system" => envx_core::EnvVarSource::System,
            "user" => envx_core::EnvVarSource::User,
            "process" => envx_core::EnvVarSource::Process,
            "shell" => envx_core::EnvVarSource::Shell,
            _ => return Err(eyre!("Invalid source: {}", src)),
        };

        vars_to_export.retain(|v| v.source == source_filter);
    }

    if vars_to_export.is_empty() {
        println!("No variables to export.");
        return Ok(());
    }

    // Determine format
    let export_format = if let Some(fmt) = format {
        match fmt.as_str() {
            "env" => ExportFormat::DotEnv,
            "json" => ExportFormat::Json,
            "yaml" | "yml" => ExportFormat::Yaml,
            "txt" | "text" => ExportFormat::Text,
            "ps1" | "powershell" => ExportFormat::PowerShell,
            "sh" | "bash" => ExportFormat::Shell,
            _ => return Err(eyre!("Unsupported format: {}", fmt)),
        }
    } else {
        // Auto-detect from extension
        ExportFormat::from_extension(file)?
    };

    // Export
    let exporter = Exporter::new(vars_to_export, metadata);
    exporter.export_to_file(file, export_format)?;

    println!("Exported {} variables to '{}'", exporter.count(), file);

    Ok(())
}

fn handle_import(
    file: &str,
    vars: &[String],
    format: Option<String>,
    permanent: bool,
    prefix: Option<&String>,
    overwrite: bool,
    dry_run: bool,
) -> Result<()> {
    // Check if file exists
    if !Path::new(&file).exists() {
        return Err(eyre!("File not found: {}", file));
    }

    // Determine format
    let import_format = if let Some(fmt) = format {
        match fmt.as_str() {
            "env" => ImportFormat::DotEnv,
            "json" => ImportFormat::Json,
            "yaml" | "yml" => ImportFormat::Yaml,
            "txt" | "text" => ImportFormat::Text,
            _ => return Err(eyre!("Unsupported format: {}", fmt)),
        }
    } else {
        // Auto-detect from extension
        ImportFormat::from_extension(file)?
    };

    // Import variables
    let mut importer = Importer::new();
    importer.import_from_file(file, import_format)?;

    // Filter variables if patterns specified
    if !vars.is_empty() {
        importer.filter_by_patterns(vars);
    }

    // Add prefix if specified
    if let Some(pfx) = &prefix {
        importer.add_prefix(pfx);
    }

    // Get variables to import
    let import_vars = importer.get_variables();

    if import_vars.is_empty() {
        println!("No variables to import.");
        return Ok(());
    }

    // Check for conflicts
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    let mut conflicts = Vec::new();
    for (name, _) in &import_vars {
        if manager.get(name).is_some() {
            conflicts.push(name.clone());
        }
    }

    if !conflicts.is_empty() && !overwrite && !dry_run {
        println!("The following variables already exist:");
        for name in &conflicts {
            println!("  - {name}");
        }

        print!("Overwrite existing variables? [y/N]: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Import cancelled.");
            return Ok(());
        }
    }

    // Preview or apply changes
    if dry_run {
        println!("Would import {} variables:", import_vars.len());
        for (name, value) in &import_vars {
            let status = if conflicts.contains(name) {
                " [OVERWRITE]"
            } else {
                " [NEW]"
            };
            println!(
                "  {} = {}{}",
                name,
                if value.len() > 50 {
                    format!("{}...", &value[..50])
                } else {
                    value.clone()
                },
                status
            );
        }
        println!("\n(Dry run - no changes made)");
    } else {
        // Apply imports
        let mut imported = 0;
        let mut failed = 0;

        for (name, value) in import_vars {
            match manager.set(&name, &value, permanent) {
                Ok(()) => imported += 1,
                Err(e) => {
                    eprintln!("Failed to import {name}: {e}");
                    failed += 1;
                }
            }
        }

        println!("Imported {imported} variables");
        if failed > 0 {
            println!("Failed to import {failed} variables");
        }
    }

    Ok(())
}

fn handle_list_command(
    source: Option<&str>,
    query: Option<&str>,
    format: &str,
    sort: &str,
    names_only: bool,
    limit: Option<usize>,
    stats: bool,
) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    // Get filtered variables
    let mut vars = if let Some(q) = &query {
        manager.search(q)
    } else if let Some(src) = source {
        let source_filter = match src {
            "system" => envx_core::EnvVarSource::System,
            "user" => envx_core::EnvVarSource::User,
            "process" => envx_core::EnvVarSource::Process,
            "shell" => envx_core::EnvVarSource::Shell,
            _ => return Err(eyre!("Invalid source: {}", src)),
        };
        manager.filter_by_source(&source_filter)
    } else {
        manager.list()
    };

    // Sort variables
    match sort {
        "name" => vars.sort_by(|a, b| a.name.cmp(&b.name)),
        "value" => vars.sort_by(|a, b| a.value.cmp(&b.value)),
        "source" => vars.sort_by(|a, b| format!("{:?}", a.source).cmp(&format!("{:?}", b.source))),
        _ => {}
    }

    // Apply limit if specified
    let total_count = vars.len();
    if let Some(lim) = limit {
        vars.truncate(lim);
    }

    // Show statistics if requested
    if stats || (format == "table" && !names_only) {
        print_statistics(&manager, &vars, total_count, query, source);
    }

    // Handle names_only flag
    if names_only {
        for var in vars {
            println!("{}", var.name);
        }
        return Ok(());
    }

    // Format output
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&vars)?);
        }
        "simple" => {
            for var in vars {
                println!("{} = {}", style(&var.name).cyan(), var.value);
            }
        }
        "compact" => {
            for var in vars {
                let source_str = format_source_compact(&var.source);
                println!(
                    "{} {} = {}",
                    source_str,
                    style(&var.name).bright(),
                    style(truncate_value(&var.value, 60)).dim()
                );
            }
        }
        _ => {
            print_table(vars, limit.is_some());
        }
    }

    // Show limit notice
    if let Some(lim) = limit {
        if total_count > lim {
            println!(
                "\n{}",
                style(format!(
                    "Showing {lim} of {total_count} total variables. Use --limit to see more."
                ))
                .yellow()
            );
        }
    }

    Ok(())
}

/// Handle snapshot-related commands.
///
/// # Errors
///
/// This function will return an error if:
/// - The snapshot manager cannot be initialized
/// - Environment variable loading fails
/// - Snapshot operations fail (create, restore, delete, etc.)
/// - File I/O operations fail during snapshot operations
/// - User input cannot be read from stdin
/// - Invalid snapshot names or IDs are provided
pub fn handle_snapshot(args: SnapshotArgs) -> Result<()> {
    let snapshot_manager = SnapshotManager::new()?;
    let mut env_manager = EnvVarManager::new();
    env_manager.load_all()?;

    match args.command {
        SnapshotCommands::Create { name, description } => {
            let vars = env_manager.list().into_iter().cloned().collect();
            let snapshot = snapshot_manager.create(name, description, vars)?;
            println!("✅ Created snapshot: {} (ID: {})", snapshot.name, snapshot.id);
        }
        SnapshotCommands::List => {
            let snapshots = snapshot_manager.list()?;
            if snapshots.is_empty() {
                println!("No snapshots found.");
                return Ok(());
            }

            let mut table = Table::new();
            table.set_header(vec!["Name", "ID", "Created", "Variables", "Description"]);

            for snapshot in snapshots {
                table.add_row(vec![
                    snapshot.name,
                    snapshot.id[..8].to_string(),
                    snapshot.created_at.format("%Y-%m-%d %H:%M").to_string(),
                    snapshot.variables.len().to_string(),
                    snapshot.description.unwrap_or_default(),
                ]);
            }

            println!("{table}");
        }
        SnapshotCommands::Show { snapshot } => {
            let snap = snapshot_manager.get(&snapshot)?;
            println!("Snapshot: {}", snap.name);
            println!("ID: {}", snap.id);
            println!("Created: {}", snap.created_at.format("%Y-%m-%d %H:%M:%S"));
            println!("Description: {}", snap.description.unwrap_or_default());
            println!("Variables: {}", snap.variables.len());

            // Show first 10 variables
            println!("\nFirst 10 variables:");
            for (i, (name, var)) in snap.variables.iter().take(10).enumerate() {
                println!("  {}. {} = {}", i + 1, name, var.value);
            }

            if snap.variables.len() > 10 {
                println!("  ... and {} more", snap.variables.len() - 10);
            }
        }
        SnapshotCommands::Restore { snapshot, force } => {
            if !force {
                print!("⚠️  This will replace all current environment variables. Continue? [y/N] ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            snapshot_manager.restore(&snapshot, &mut env_manager)?;
            println!("✅ Restored from snapshot: {snapshot}");
        }
        SnapshotCommands::Delete { snapshot, force } => {
            if !force {
                print!("⚠️  Delete snapshot '{snapshot}'? [y/N] ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            snapshot_manager.delete(&snapshot)?;
            println!("✅ Deleted snapshot: {snapshot}");
        }
        SnapshotCommands::Diff { snapshot1, snapshot2 } => {
            let diff = snapshot_manager.diff(&snapshot1, &snapshot2)?;

            if diff.added.is_empty() && diff.removed.is_empty() && diff.modified.is_empty() {
                println!("No differences found between snapshots.");
                return Ok(());
            }

            if !diff.added.is_empty() {
                println!("➕ Added in {snapshot2}:");
                for (name, var) in &diff.added {
                    println!("   {} = {}", name, var.value);
                }
            }

            if !diff.removed.is_empty() {
                println!("\n➖ Removed in {snapshot2}:");
                for (name, var) in &diff.removed {
                    println!("   {} = {}", name, var.value);
                }
            }

            if !diff.modified.is_empty() {
                println!("\n🔄 Modified:");
                for (name, (old, new)) in &diff.modified {
                    println!("   {name}:");
                    println!("     Old: {}", old.value);
                    println!("     New: {}", new.value);
                }
            }
        }
    }

    Ok(())
}

/// Handle profile-related commands.
///
/// # Errors
///
/// This function will return an error if:
/// - The profile manager cannot be initialized
/// - Environment variable loading fails  
/// - Profile operations fail (create, switch, delete, etc.)
/// - File I/O operations fail during profile import/export
/// - User input cannot be read from stdin
/// - Invalid profile names are provided
/// - Profile data cannot be serialized/deserialized
pub fn handle_profile(args: ProfileArgs) -> Result<()> {
    let mut profile_manager = ProfileManager::new()?;
    let mut env_manager = EnvVarManager::new();
    env_manager.load_all()?;

    match args.command {
        ProfileCommands::Create { name, description } => {
            handle_profile_create(&mut profile_manager, &name, description)?;
        }
        ProfileCommands::List => {
            handle_profile_list(&profile_manager);
        }
        ProfileCommands::Show { name } => {
            handle_profile_show(&profile_manager, name)?;
        }
        ProfileCommands::Switch { name, apply } => {
            handle_profile_switch(&mut profile_manager, &mut env_manager, &name, apply)?;
        }
        ProfileCommands::Add {
            profile,
            name,
            value,
            override_system,
        } => {
            handle_profile_add(&mut profile_manager, &profile, &name, &value, override_system)?;
        }
        ProfileCommands::Remove { profile, name } => {
            handle_profile_remove(&mut profile_manager, &profile, &name)?;
        }
        ProfileCommands::Delete { name, force } => {
            handle_profile_delete(&mut profile_manager, &name, force)?;
        }
        ProfileCommands::Export { name, output } => {
            handle_profile_export(&profile_manager, &name, output)?;
        }
        ProfileCommands::Import { file, name, overwrite } => {
            handle_profile_import(&mut profile_manager, &file, name, overwrite)?;
        }
        ProfileCommands::Apply { name } => {
            handle_profile_apply(&mut profile_manager, &mut env_manager, &name)?;
        }
    }

    Ok(())
}

fn handle_profile_create(profile_manager: &mut ProfileManager, name: &str, description: Option<String>) -> Result<()> {
    profile_manager.create(name.to_string(), description)?;
    println!("✅ Created profile: {name}");
    Ok(())
}

fn handle_profile_list(profile_manager: &ProfileManager) {
    let profiles = profile_manager.list();
    if profiles.is_empty() {
        println!("No profiles found.");
    }

    let active = profile_manager.active().map(|p| &p.name);
    let mut table = Table::new();
    table.set_header(vec!["Name", "Variables", "Created", "Description", "Status"]);

    for profile in profiles {
        let status = if active == Some(&profile.name) {
            "● Active"
        } else {
            ""
        };

        table.add_row(vec![
            profile.name.clone(),
            profile.variables.len().to_string(),
            profile.created_at.format("%Y-%m-%d").to_string(),
            profile.description.clone().unwrap_or_default(),
            status.to_string(),
        ]);
    }

    println!("{table}");
}

fn handle_profile_show(profile_manager: &ProfileManager, name: Option<String>) -> Result<()> {
    let profile = if let Some(name) = name {
        profile_manager
            .get(&name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Profile '{}' not found", name))?
    } else {
        profile_manager
            .active()
            .ok_or_else(|| color_eyre::eyre::eyre!("No active profile"))?
    };

    println!("Profile: {}", profile.name);
    println!("Description: {}", profile.description.as_deref().unwrap_or(""));
    println!("Created: {}", profile.created_at.format("%Y-%m-%d %H:%M:%S"));
    println!("Updated: {}", profile.updated_at.format("%Y-%m-%d %H:%M:%S"));
    if let Some(parent) = &profile.parent {
        println!("Inherits from: {parent}");
    }
    println!("\nVariables:");

    for (name, var) in &profile.variables {
        let status = if var.enabled { "✓" } else { "✗" };
        let override_flag = if var.override_system { " [override]" } else { "" };
        println!("  {} {} = {}{}", status, name, var.value, override_flag);
    }
    Ok(())
}

fn handle_profile_switch(
    profile_manager: &mut ProfileManager,
    env_manager: &mut EnvVarManager,
    name: &str,
    apply: bool,
) -> Result<()> {
    profile_manager.switch(name)?;
    println!("✅ Switched to profile: {name}");

    if apply {
        profile_manager.apply(name, env_manager)?;
        println!("✅ Applied profile variables");
    }
    Ok(())
}

fn handle_profile_add(
    profile_manager: &mut ProfileManager,
    profile: &str,
    name: &str,
    value: &str,
    override_system: bool,
) -> Result<()> {
    let prof = profile_manager
        .get_mut(profile)
        .ok_or_else(|| color_eyre::eyre::eyre!("Profile '{}' not found", profile))?;

    prof.add_var(name.to_string(), value.to_string(), override_system);
    profile_manager.save()?;

    println!("✅ Added {name} to profile {profile}");
    Ok(())
}

fn handle_profile_remove(profile_manager: &mut ProfileManager, profile: &str, name: &str) -> Result<()> {
    let prof = profile_manager
        .get_mut(profile)
        .ok_or_else(|| color_eyre::eyre::eyre!("Profile '{}' not found", profile))?;

    prof.remove_var(name)
        .ok_or_else(|| color_eyre::eyre::eyre!("Variable '{}' not found in profile", name))?;

    profile_manager.save()?;
    println!("✅ Removed {name} from profile {profile}");
    Ok(())
}

fn handle_profile_delete(profile_manager: &mut ProfileManager, name: &str, force: bool) -> Result<()> {
    if !force {
        print!("⚠️  Delete profile '{name}'? [y/N] ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    profile_manager.delete(name)?;
    println!("✅ Deleted profile: {name}");
    Ok(())
}

fn handle_profile_export(profile_manager: &ProfileManager, name: &str, output: Option<String>) -> Result<()> {
    let json = profile_manager.export(name)?;

    if let Some(path) = output {
        std::fs::write(path, json)?;
        println!("✅ Exported profile to file");
    } else {
        println!("{json}");
    }
    Ok(())
}

fn handle_profile_import(
    profile_manager: &mut ProfileManager,
    file: &str,
    name: Option<String>,
    overwrite: bool,
) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let import_name = name.unwrap_or_else(|| "imported".to_string());

    profile_manager.import(import_name.clone(), &content, overwrite)?;
    println!("✅ Imported profile: {import_name}");
    Ok(())
}

fn handle_profile_apply(
    profile_manager: &mut ProfileManager,
    env_manager: &mut EnvVarManager,
    name: &str,
) -> Result<()> {
    profile_manager.apply(name, env_manager)?;
    println!("✅ Applied profile: {name}");
    Ok(())
}

fn print_statistics(
    manager: &EnvVarManager,
    filtered_vars: &[&envx_core::EnvVar],
    total_count: usize,
    query: Option<&str>,
    source: Option<&str>,
) {
    let _term = Term::stdout();

    // Count by source
    let system_count = manager.filter_by_source(&envx_core::EnvVarSource::System).len();
    let user_count = manager.filter_by_source(&envx_core::EnvVarSource::User).len();
    let process_count = manager.filter_by_source(&envx_core::EnvVarSource::Process).len();
    let shell_count = manager.filter_by_source(&envx_core::EnvVarSource::Shell).len();

    // Header
    println!("{}", style("═".repeat(60)).blue().bold());
    println!("{}", style("Environment Variables Summary").cyan().bold());
    println!("{}", style("═".repeat(60)).blue().bold());

    // Filter info
    if query.is_some() || source.is_some() {
        print!("  {} ", style("Filter:").yellow());
        if let Some(q) = query {
            print!("query='{}' ", style(q).green());
        }
        if let Some(s) = source {
            print!("source={} ", style(s).green());
        }
        println!();
        println!(
            "  {} {}/{} variables",
            style("Showing:").yellow(),
            style(filtered_vars.len()).green().bold(),
            total_count
        );
    } else {
        println!(
            "  {} {} variables",
            style("Total:").yellow(),
            style(total_count).green().bold()
        );
    }

    println!();
    println!("  {} By Source:", style("►").cyan());

    // Source breakdown with visual bars
    let max_count = system_count.max(user_count).max(process_count).max(shell_count);
    let bar_width = 30;

    print_source_bar("System", system_count, max_count, bar_width, "red");
    print_source_bar("User", user_count, max_count, bar_width, "yellow");
    print_source_bar("Process", process_count, max_count, bar_width, "green");
    print_source_bar("Shell", shell_count, max_count, bar_width, "cyan");

    println!("{}", style("─".repeat(60)).blue());
    println!();
}

fn print_source_bar(label: &str, count: usize, max: usize, width: usize, color: &str) {
    let filled = if max > 0 { (count * width / max).max(1) } else { 0 };

    let bar = "█".repeat(filled);
    let empty = "░".repeat(width - filled);

    let colored_bar = match color {
        "red" => style(bar).red(),
        "yellow" => style(bar).yellow(),
        "green" => style(bar).green(),
        "cyan" => style(bar).cyan(),
        _ => style(bar).white(),
    };

    println!(
        "    {:10} {} {}{} {}",
        style(label).bold(),
        colored_bar,
        style(empty).dim(),
        style(format!(" {count:4}")).bold(),
        style("vars").dim()
    );
}

fn print_table(vars: Vec<&envx_core::EnvVar>, _is_limited: bool) {
    if vars.is_empty() {
        println!("{}", style("No environment variables found.").yellow());
    }

    let mut table = Table::new();

    // Configure table style
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(120)
        .set_header(vec![
            Cell::new("Source").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Name").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Value").add_attribute(Attribute::Bold).fg(Color::Cyan),
        ]);

    // Add rows with colored source indicators
    for var in vars {
        let (source_str, source_color) = format_source(&var.source);
        let truncated_value = truncate_value(&var.value, 50);

        table.add_row(vec![
            Cell::new(source_str).fg(source_color),
            Cell::new(&var.name).fg(Color::White),
            Cell::new(truncated_value).fg(Color::Grey),
        ]);
    }

    println!("{table}");
}

fn format_source(source: &envx_core::EnvVarSource) -> (String, Color) {
    match source {
        envx_core::EnvVarSource::System => ("System".to_string(), Color::Red),
        envx_core::EnvVarSource::User => ("User".to_string(), Color::Yellow),
        envx_core::EnvVarSource::Process => ("Process".to_string(), Color::Green),
        envx_core::EnvVarSource::Shell => ("Shell".to_string(), Color::Cyan),
        envx_core::EnvVarSource::Application(app) => (format!("App:{app}"), Color::Magenta),
    }
}

fn format_source_compact(source: &envx_core::EnvVarSource) -> console::StyledObject<String> {
    match source {
        envx_core::EnvVarSource::System => style("[SYS]".to_string()).red().bold(),
        envx_core::EnvVarSource::User => style("[USR]".to_string()).yellow().bold(),
        envx_core::EnvVarSource::Process => style("[PRC]".to_string()).green().bold(),
        envx_core::EnvVarSource::Shell => style("[SHL]".to_string()).cyan().bold(),
        envx_core::EnvVarSource::Application(app) => style(format!("[{}]", &app[..3.min(app.len())].to_uppercase()))
            .magenta()
            .bold(),
    }
}

fn truncate_value(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_string()
    } else {
        format!("{}...", &value[..max_len - 3])
    }
}

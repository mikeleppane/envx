use crate::MonitorArgs;
use crate::ProfileArgs;
use crate::ProjectArgs;
use crate::RenameArgs;
use crate::SnapshotArgs;
use crate::WatchArgs;
use crate::handle_find_replace;
use crate::handle_list_command;
use crate::handle_path_command;
use crate::handle_profile;
use crate::handle_project;
use crate::handle_rename;
use crate::handle_replace;
use crate::handle_snapshot;
use crate::handle_watch;
use crate::monitor::handle_monitor;
use crate::replace::FindReplaceArgs;
use crate::replace::ReplaceArgs;
use clap::{Parser, Subcommand};
use color_eyre::Result;
use color_eyre::eyre::eyre;
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

    /// Manage project-specific configuration
    Project(ProjectArgs),

    /// Rename environment variables (supports wildcards)
    Rename(RenameArgs),

    /// Replace environment variable values
    Replace(ReplaceArgs),

    /// Find and replace text within environment variable values
    FindReplace(FindReplaceArgs),

    /// Watch files for changes and auto-sync
    Watch(WatchArgs),

    /// Monitor environment variable changes (read-only)
    Monitor(MonitorArgs),
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

        Commands::Project(args) => {
            handle_project(args)?;
        }

        Commands::Rename(args) => {
            handle_rename(&args)?;
        }

        Commands::Replace(args) => {
            handle_replace(&args)?;
        }

        Commands::FindReplace(args) => {
            handle_find_replace(&args)?;
        }

        Commands::Watch(args) => {
            handle_watch(&args)?;
        }

        Commands::Monitor(args) => {
            handle_monitor(args)?;
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

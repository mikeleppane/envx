use std::{io::Write, path::Path};

use crate::PathAction;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use envx_core::{EnvVarManager, PathManager};

/// Handles PATH command operations including add, remove, clean, dedupe, check, list, and move.
///
/// # Arguments
/// * `action` - The specific PATH action to perform, or None to list entries
/// * `check` - Whether to check for invalid entries when listing
/// * `var` - The environment variable name (typically "PATH")
/// * `permanent` - Whether to make changes permanent to the system
///
/// # Errors
/// Returns an error if:
/// - The specified environment variable is not found
/// - File system operations fail (creating directories, reading/writing)
/// - Invalid input is provided for move operations
/// - Environment variable operations fail
///
/// # Panics
/// Panics if `action` is `None` but the function logic expects it to be `Some`.
/// This should not happen in normal usage as the logic handles the `None` case before
/// calling `expect()`.
#[allow(clippy::too_many_lines)]
pub fn handle_path_command(action: Option<PathAction>, check: bool, var: &str, permanent: bool) -> Result<()> {
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

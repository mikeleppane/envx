use clap::Args;
use color_eyre::Result;
use comfy_table::{Table, presets::UTF8_FULL};
use envx_core::{EnvVarManager, env::split_wildcard_pattern};

#[derive(Args)]
pub struct ReplaceArgs {
    /// Variable name or pattern (supports wildcards with *)
    pub pattern: String,

    /// New value to set
    pub value: String,

    /// Dry run - show what would be replaced without making changes
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct FindReplaceArgs {
    /// Text to search for in values
    pub search: String,

    /// Text to replace with
    pub replacement: String,

    /// Only search in variables matching this pattern (supports wildcards)
    #[arg(short = 'p', long)]
    pub pattern: Option<String>,

    /// Dry run - show what would be replaced without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Handle replace command to replace environment variable values using patterns.
///
/// # Errors
///
/// This function will return an error if:
/// - Environment variable operations fail (loading, replacing)
/// - Pattern matching fails or produces invalid results
/// - File I/O operations fail when persisting changes
/// - Wildcard pattern parsing fails
pub fn handle_replace(args: &ReplaceArgs) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    if args.dry_run {
        // Show what would be replaced
        let preview = preview_replace(&manager, &args.pattern)?;

        if preview.is_empty() {
            println!("No variables match the pattern '{}'", args.pattern);
        } else {
            println!("Would update {} variable(s):", preview.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Variable", "Current Value", "New Value"]);

            for (name, current) in preview {
                table.add_row(vec![name, current, args.value.clone()]);
            }

            println!("{table}");
            println!("\nUse without --dry-run to apply changes");
        }
    } else {
        let replaced = manager.replace(&args.pattern, &args.value)?;

        if replaced.is_empty() {
            println!("No variables match the pattern '{}'", args.pattern);
        } else {
            println!("✅ Updated {} variable(s):", replaced.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Variable", "Old Value", "New Value"]);

            for (name, old, new) in &replaced {
                // Truncate long values for display
                let display_old = if old.len() > 50 {
                    format!("{}...", &old[..47])
                } else {
                    old.clone()
                };
                let display_new = if new.len() > 50 {
                    format!("{}...", &new[..47])
                } else {
                    new.clone()
                };
                table.add_row(vec![name.clone(), display_old, display_new]);
            }

            println!("{table}");

            #[cfg(windows)]
            println!("\n📝 Note: You may need to restart your terminal for changes to take effect");
        }
    }

    Ok(())
}

fn preview_replace(manager: &EnvVarManager, pattern: &str) -> Result<Vec<(String, String)>> {
    let mut preview = Vec::new();

    if pattern.contains('*') {
        let (prefix, suffix) = split_wildcard_pattern(pattern)?;

        for var in manager.list() {
            if var.name.starts_with(&prefix)
                && var.name.ends_with(&suffix)
                && var.name.len() >= prefix.len() + suffix.len()
            {
                preview.push((var.name.clone(), var.value.clone()));
            }
        }
    } else if let Some(var) = manager.get(pattern) {
        preview.push((var.name.clone(), var.value.clone()));
    }

    Ok(preview)
}

/// Handle find and replace operations within environment variable values.
///
/// # Errors
///
/// This function will return an error if:
/// - Environment variable operations fail (loading, updating)
/// - Pattern matching fails or produces invalid results
/// - Find and replace operations fail
/// - File I/O operations fail when persisting changes
/// - Wildcard pattern parsing fails
pub fn handle_find_replace(args: &FindReplaceArgs) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    if args.dry_run {
        // Show preview
        let preview = preview_find_replace(&manager, &args.search, &args.replacement, args.pattern.as_deref())?;

        if preview.is_empty() {
            println!("No variables contain '{}'", args.search);
        } else {
            println!("Would update {} variable(s):", preview.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Variable", "Current Value", "New Value"]);

            for (name, old, new) in preview {
                table.add_row(vec![name, old, new]);
            }

            println!("{table}");
            println!("\nUse without --dry-run to apply changes");
        }
    } else {
        let replaced = manager.find_replace(&args.search, &args.replacement, args.pattern.as_deref())?;

        if replaced.is_empty() {
            println!("No variables contain '{}'", args.search);
        } else {
            println!("✅ Updated {} variable(s):", replaced.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Variable", "Old Value", "New Value"]);

            for (name, old, new) in &replaced {
                // Truncate long values
                let display_old = if old.len() > 50 {
                    format!("{}...", &old[..47])
                } else {
                    old.clone()
                };
                let display_new = if new.len() > 50 {
                    format!("{}...", &new[..47])
                } else {
                    new.clone()
                };
                table.add_row(vec![name.clone(), display_old, display_new]);
            }

            println!("{table}");

            #[cfg(windows)]
            println!("\n📝 Note: You may need to restart your terminal for changes to take effect");
        }
    }

    Ok(())
}

fn preview_find_replace(
    manager: &EnvVarManager,
    search: &str,
    replacement: &str,
    pattern: Option<&str>,
) -> Result<Vec<(String, String, String)>> {
    let mut preview = Vec::new();

    for var in manager.list() {
        // Check if variable matches pattern (if specified)
        let matches_pattern = if let Some(pat) = pattern {
            if pat.contains('*') {
                let (prefix, suffix) = split_wildcard_pattern(pat)?;
                var.name.starts_with(&prefix)
                    && var.name.ends_with(&suffix)
                    && var.name.len() >= prefix.len() + suffix.len()
            } else {
                var.name == pat
            }
        } else {
            true
        };

        if matches_pattern && var.value.contains(search) {
            let new_value = var.value.replace(search, replacement);
            preview.push((var.name.clone(), var.value.clone(), new_value));
        }
    }

    Ok(preview)
}

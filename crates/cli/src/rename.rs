use clap::{Args, arg};
use color_eyre::Result;
use comfy_table::{Table, presets::UTF8_FULL};
use envx_core::{EnvVarManager, env::split_wildcard_pattern};

#[derive(Args)]
pub struct RenameArgs {
    /// Pattern to match (supports wildcards with *)
    pub pattern: String,

    /// New name or pattern
    pub replacement: String,

    /// Dry run - show what would be renamed without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Handle rename command to rename environment variables using patterns.
///
/// # Errors
///
/// This function will return an error if:
/// - Environment variable operations fail (loading, renaming)
/// - Pattern matching fails or produces invalid results
/// - Variable names conflict or are invalid
/// - File I/O operations fail when persisting changes
/// - User input cannot be read from stdin during confirmation
pub fn handle_rename(args: &RenameArgs) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    if args.dry_run {
        // Show what would be renamed
        let preview = preview_rename(&manager, &args.pattern, &args.replacement)?;

        if preview.is_empty() {
            println!("No variables match the pattern '{}'", args.pattern);
        } else {
            println!("Would rename {} variable(s):", preview.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Current Name", "New Name", "Value"]);

            for (old, new, value) in preview {
                table.add_row(vec![old, new, value]);
            }

            println!("{table}");
            println!("\nUse without --dry-run to apply changes");
        }
    } else {
        let renamed = manager.rename(&args.pattern, &args.replacement)?;

        if renamed.is_empty() {
            println!("No variables match the pattern '{}'", args.pattern);
        } else {
            println!("✅ Renamed {} variable(s):", renamed.len());

            let mut table = Table::new();
            table.load_preset(UTF8_FULL);
            table.set_header(vec!["Old Name", "New Name"]);

            for (old, new) in &renamed {
                table.add_row(vec![old.clone(), new.clone()]);
            }

            println!("{table}");

            #[cfg(windows)]
            println!("\n📝 Note: You may need to restart your terminal for changes to take effect");
        }
    }

    Ok(())
}

fn preview_rename(manager: &EnvVarManager, pattern: &str, replacement: &str) -> Result<Vec<(String, String, String)>> {
    let mut preview = Vec::new();

    if pattern.contains('*') {
        let (prefix, suffix) = split_wildcard_pattern(pattern)?;
        let (new_prefix, new_suffix) = split_wildcard_pattern(replacement)?;

        for var in manager.list() {
            if var.name.starts_with(&prefix)
                && var.name.ends_with(&suffix)
                && var.name.len() >= prefix.len() + suffix.len()
            {
                let middle = &var.name[prefix.len()..var.name.len() - suffix.len()];
                let new_name = format!("{new_prefix}{middle}{new_suffix}");
                preview.push((var.name.clone(), new_name, var.value.clone()));
            }
        }
    } else if let Some(var) = manager.get(pattern) {
        preview.push((var.name.clone(), replacement.to_string(), var.value.clone()));
    }

    Ok(preview)
}

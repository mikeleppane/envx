use clap::{Args, Subcommand};
use color_eyre::Result;
use comfy_table::Table;
use envx_core::{EnvVarManager, ProfileManager};

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

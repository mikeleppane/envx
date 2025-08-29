use clap::command;
use clap::{Args, Subcommand};
use color_eyre::Result;
use comfy_table::Table;
use envx_core::{EnvVarManager, ProfileManager, ProjectConfig, ProjectManager, RequiredVar, ValidationReport};

#[derive(Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommands,
}

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Initialize a new project configuration
    Init {
        /// Project name
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Apply project configuration
    Apply {
        /// Force apply even with validation errors
        #[arg(short, long)]
        force: bool,
    },
    /// Validate project configuration
    Check,
    /// Edit project configuration
    Edit,
    /// Show project information
    Info,
    /// Run a project script
    Run {
        /// Script name
        script: String,
    },
    /// Add a required variable
    Require {
        /// Variable name
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Validation pattern (regex)
        #[arg(short, long)]
        pattern: Option<String>,
        /// Example value
        #[arg(short, long)]
        example: Option<String>,
    },
}

/// Handle project-related commands.
///
/// # Errors
///
/// This function will return an error if:
/// - Project manager initialization fails
/// - Environment variable manager operations fail
/// - Project configuration file cannot be found, read, or written
/// - Project validation fails (when not using --force)
/// - Profile manager operations fail
/// - Script execution fails
/// - Required variable configuration cannot be updated
/// - File I/O operations fail during project operations
#[allow(clippy::too_many_lines)]
pub fn handle_project(args: ProjectArgs) -> Result<()> {
    match args.command {
        ProjectCommands::Init { name } => {
            let manager = ProjectManager::new()?;
            manager.init(name)?;
        }

        ProjectCommands::Apply { force } => {
            let mut project = ProjectManager::new()?;
            let mut env_manager = EnvVarManager::new();
            let mut profile_manager = ProfileManager::new()?;

            if let Some(project_dir) = project.find_and_load()? {
                println!("📁 Found project at: {}", project_dir.display());

                // Validate first
                let report = project.validate(&env_manager)?;

                if !report.success && !force {
                    print_validation_report(&report);
                    return Err(color_eyre::eyre::eyre!(
                        "Validation failed. Use --force to apply anyway."
                    ));
                }

                // Apply configuration
                project.apply(&mut env_manager, &mut profile_manager)?;
                println!("✅ Applied project configuration");

                if !report.warnings.is_empty() {
                    println!("\n⚠️  Warnings:");
                    for warning in &report.warnings {
                        println!("  - {}: {}", warning.var_name, warning.message);
                    }
                }
            } else {
                return Err(color_eyre::eyre::eyre!(
                    "No .envx/config.yaml found in current or parent directories"
                ));
            }
        }

        ProjectCommands::Check => {
            let mut project = ProjectManager::new()?;
            let env_manager = EnvVarManager::new();

            if project.find_and_load()?.is_some() {
                let report = project.validate(&env_manager)?;
                print_validation_report(&report);

                if !report.success {
                    std::process::exit(1);
                }
            } else {
                return Err(color_eyre::eyre::eyre!("No project configuration found"));
            }
        }

        ProjectCommands::Edit => {
            let _ = ProjectManager::new()?;
            let config_path = std::env::current_dir()?.join(".envx").join("config.yaml");

            if !config_path.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "No .envx/config.yaml found. Run 'envx init' first."
                ));
            }

            #[cfg(windows)]
            {
                std::process::Command::new("notepad").arg(&config_path).spawn()?;
            }

            #[cfg(unix)]
            {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
                std::process::Command::new(editor).arg(&config_path).spawn()?;
            }

            println!("📝 Opening config in editor...");
        }

        ProjectCommands::Info => {
            let mut project = ProjectManager::new()?;

            if let Some(project_dir) = project.find_and_load()? {
                // Load and display config
                let config_path = project_dir.join(".envx").join("config.yaml");
                let content = std::fs::read_to_string(&config_path)?;

                println!("📁 Project Directory: {}", project_dir.display());
                println!("\n📄 Configuration:");
                println!("{content}");
            } else {
                return Err(color_eyre::eyre::eyre!("No project configuration found"));
            }
        }

        ProjectCommands::Run { script } => {
            let mut project = ProjectManager::new()?;
            let mut env_manager = EnvVarManager::new();

            if project.find_and_load()?.is_some() {
                project.run_script(&script, &mut env_manager)?;
                println!("✅ Script '{script}' completed");
            } else {
                return Err(color_eyre::eyre::eyre!("No project configuration found"));
            }
        }

        ProjectCommands::Require {
            name,
            description,
            pattern,
            example,
        } => {
            let config_path = std::env::current_dir()?.join(".envx").join("config.yaml");

            if !config_path.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "No .envx/config.yaml found. Run 'envx init' first."
                ));
            }

            // Load, modify, and save config
            let mut config = ProjectConfig::load(&config_path)?;
            config.required.push(RequiredVar {
                name: name.clone(),
                description,
                pattern,
                example,
            });
            config.save(&config_path)?;

            println!("✅ Added required variable: {name}");
        }
    }

    Ok(())
}

fn print_validation_report(report: &ValidationReport) {
    if report.success {
        println!("✅ All required variables are set!");
        return;
    }

    if !report.missing.is_empty() {
        println!("❌ Missing required variables:");
        let mut table = Table::new();
        table.set_header(vec!["Variable", "Description", "Example"]);

        for var in &report.missing {
            table.add_row(vec![
                var.name.clone(),
                var.description.clone().unwrap_or_default(),
                var.example.clone().unwrap_or_default(),
            ]);
        }

        println!("{table}");
    }

    if !report.errors.is_empty() {
        println!("\n❌ Validation errors:");
        for error in &report.errors {
            println!("  - {}: {}", error.var_name, error.message);
        }
    }
}

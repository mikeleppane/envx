use std::path::PathBuf;

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
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Apply project configuration
    Apply {
        /// Force apply even with validation errors
        #[arg(short, long)]
        force: bool,
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Validate project configuration
    Check {
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Edit project configuration
    Edit {
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Show project information
    Info {
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Run a project script
    Run {
        /// Script name
        script: String,
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
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
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
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
        ProjectCommands::Init { name, file } => {
            let manager = ProjectManager::new()?;

            if let Some(custom_file) = file {
                manager.init_with_file(name, &custom_file)?;
                println!("✅ Created project configuration at: {}", custom_file.display());
            } else {
                manager.init(name)?;
            }
        }

        ProjectCommands::Apply { force, file } => {
            let mut project = ProjectManager::new()?;
            let mut env_manager = EnvVarManager::new();
            let mut profile_manager = ProfileManager::new()?;

            let loaded = if let Some(custom_file) = file {
                project.load_from_file(&custom_file)?;
                Some(custom_file.parent().unwrap_or(&PathBuf::from(".")).to_path_buf())
            } else {
                project.find_and_load()?
            };

            if let Some(project_dir) = loaded {
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
                return Err(color_eyre::eyre::eyre!("No configuration file found"));
            }
        }

        ProjectCommands::Check { file } => {
            let mut project = ProjectManager::new()?;
            let env_manager = EnvVarManager::new();

            let loaded = if let Some(custom_file) = file {
                project.load_from_file(&custom_file)?;
                true
            } else {
                project.find_and_load()?.is_some()
            };

            if loaded {
                let report = project.validate(&env_manager)?;
                print_validation_report(&report);

                if !report.success {
                    std::process::exit(1);
                }
            } else {
                return Err(color_eyre::eyre::eyre!("No project configuration found"));
            }
        }

        ProjectCommands::Edit { file } => {
            let config_path = if let Some(custom_file) = file {
                custom_file
            } else {
                std::env::current_dir()?.join(".envx").join("config.yaml")
            };

            if !config_path.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "Configuration file not found: {}. Run 'envx project init' first.",
                    config_path.display()
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

        ProjectCommands::Info { file } => {
            let mut project = ProjectManager::new()?;

            let (project_dir, config_path) = if let Some(custom_file) = file {
                project.load_from_file(&custom_file)?;
                (
                    custom_file.parent().unwrap_or(&PathBuf::from(".")).to_path_buf(),
                    custom_file,
                )
            } else if let Some(project_dir) = project.find_and_load()? {
                let config_path = project_dir.join(".envx").join("config.yaml");
                (project_dir, config_path)
            } else {
                return Err(color_eyre::eyre::eyre!("No project configuration found"));
            };

            let content = std::fs::read_to_string(&config_path)?;

            println!("📁 Project Directory: {}", project_dir.display());
            println!("📄 Configuration File: {}", config_path.display());
            println!("\n📄 Configuration:");
            println!("{content}");
        }

        ProjectCommands::Run { script, file } => {
            let mut project = ProjectManager::new()?;
            let mut env_manager = EnvVarManager::new();

            let loaded = if let Some(custom_file) = file {
                project.load_from_file(&custom_file)?;
                true
            } else {
                project.find_and_load()?.is_some()
            };

            if loaded {
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
            file,
        } => {
            let config_path = if let Some(custom_file) = file {
                custom_file
            } else {
                std::env::current_dir()?.join(".envx").join("config.yaml")
            };

            if !config_path.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "Configuration file not found: {}. Run 'envx project init' first.",
                    config_path.display()
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
            println!("📄 Updated file: {}", config_path.display());
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

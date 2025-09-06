#![allow(clippy::format_push_string)]
use color_eyre::Result;
use color_eyre::eyre::eyre;
use dialoguer::{Confirm, Input, MultiSelect, Select, theme::ColorfulTheme};
use std::{
    fs,
    path::{Path, PathBuf},
};

use ahash::AHashMap as HashMap;
use colored::Colorize;
use glob::glob;
use serde::{Deserialize, Serialize};

use crate::{ProfileManager, ProjectConfig, RequiredVar, ValidationRules as ConfigValidationRules};

// Custom error type for ESC handling
#[derive(Debug)]
struct EscPressed;

impl std::fmt::Display for EscPressed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "User pressed ESC")
    }
}

impl std::error::Error for EscPressed {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WizardConfig {
    pub skip_system_check: bool,
    pub auto_detect_project: bool,
    pub default_profiles: Vec<String>,
    pub template_path: Option<PathBuf>,
    pub selected_vars: Vec<SelectedVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedVariable {
    pub name: String,
    pub value: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectType {
    pub name: String,
    pub category: ProjectCategory,
    pub suggested_vars: Vec<SuggestedVariable>,
    pub suggested_profiles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectCategory {
    WebApp,
    Python,
    Rust,
    Docker,
    Microservices,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedVariable {
    pub name: String,
    pub description: String,
    pub example: String,
    pub required: bool,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub config_path: PathBuf,
    pub git_hooks: bool,
    pub ci_integration: bool,
    pub shared_profiles: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    pub require_all_defined: bool,
    pub validate_urls: bool,
    pub validate_numbers: bool,
    pub warn_missing: bool,
    pub strict_mode: bool,
    pub custom_patterns: HashMap<String, String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrations {
    pub shell_aliases: bool,
    pub auto_completion: bool,
    pub vscode_extension: bool,
    pub git_hooks: bool,
    pub docker_integration: bool,
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub shell: String,
    pub terminal: String,
    pub home_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl SystemInfo {
    /// Detects system information including OS, shell, terminal, and directories.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The home directory cannot be determined
    /// - The config directory cannot be determined
    pub fn detect() -> Result<Self> {
        let os = if cfg!(windows) {
            "Windows".to_string()
        } else if cfg!(target_os = "macos") {
            "macOS".to_string()
        } else {
            "Linux".to_string()
        };

        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "PowerShell".to_string()
            } else {
                "bash".to_string()
            }
        });

        let terminal = std::env::var("TERM_PROGRAM")
            .or_else(|_| std::env::var("TERMINAL"))
            .unwrap_or_else(|_| "Unknown".to_string());

        let home_dir = dirs::home_dir().ok_or_else(|| eyre!("Could not find home directory"))?;
        let config_dir = dirs::config_dir().ok_or_else(|| eyre!("Could not find config directory"))?;

        Ok(Self {
            os,
            shell,
            terminal,
            home_dir,
            config_dir,
        })
    }
}

#[derive(Default)]
pub struct SetupWizard {
    theme: ColorfulTheme,
    config: WizardConfig,
}

#[derive(Debug, Clone)]
pub struct SetupResult {
    pub project_type: ProjectType,
    pub profiles: Vec<String>,
    pub profile_configs: HashMap<String, HashMap<String, String>>,
    pub team_config: Option<TeamConfig>,
    pub validation_rules: ValidationRules,
    pub imported_files: Vec<PathBuf>,
    pub create_env_files: bool,
    pub selected_vars: Vec<SelectedVariable>,
}

impl SetupWizard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the setup wizard and returns the configuration result.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - System detection fails
    /// - User input validation fails
    /// - File I/O operations fail during configuration
    /// - Profile creation fails
    /// - Configuration file creation fails
    pub fn run(&mut self) -> Result<SetupResult> {
        // Wrap the entire wizard in error handling for ESC
        match self.run_wizard() {
            Ok(result) => Ok(result),
            Err(e) => {
                if e.downcast_ref::<EscPressed>().is_some() {
                    Self::show_goodbye();
                    std::process::exit(0);
                } else {
                    Err(e)
                }
            }
        }
    }

    fn run_wizard(&mut self) -> Result<SetupResult> {
        // Step 1: Welcome
        Self::show_welcome()?;

        // Step 2: Detect system
        let system_info = Self::detect_system()?;
        Self::show_system_info(&system_info);

        // Step 3: Project type
        let project_type = self.select_project_type()?;

        // Step 4: Import existing files
        let imported_files = if let Some(existing_files) = self.scan_existing_files()? {
            self.import_existing(existing_files)?
        } else {
            Vec::new()
        };

        // Step 5: Configure environment variables with values
        let selected_vars = self.configure_variables(&project_type)?;

        // Step 6: Create profiles with actual configurations
        let (profiles, profile_configs) = self.create_and_configure_profiles(&project_type, &selected_vars)?;

        // Step 7: Ask if user wants to create .env files
        let create_env_files = self.ask_create_env_files()?;

        // Step 8: Team setup
        let team_config = if self.ask_team_setup()? {
            Some(self.configure_team_features()?)
        } else {
            None
        };

        // Step 9: Validation rules
        let validation_rules = self.configure_validation(&project_type)?;

        // Step 10: Review and apply
        let result = SetupResult {
            project_type: project_type.clone(),
            profiles,
            profile_configs,
            team_config,
            validation_rules,
            imported_files,
            create_env_files,
            selected_vars,
        };

        self.review_and_apply(&result)?;

        // Step 11: Check if all required variables are set
        Self::check_required_variables(&result);

        Ok(result)
    }

    fn show_goodbye() {
        println!("\n{}", "━".repeat(65).bright_black());
        println!(
            "\n{} {}",
            "👋".bright_yellow(),
            "Setup cancelled. No worries!".bright_cyan().bold()
        );
        println!(
            "\n{}",
            "You can run 'envx init' anytime to start the setup wizard again.".bright_white()
        );
        println!("{}", "Your project files remain unchanged.".bright_white());
        println!("\n{}", "━".repeat(65).bright_black());
        println!("\n{}", "Happy coding! 🚀".bright_magenta());
    }

    #[allow(clippy::too_many_lines)]
    fn show_welcome() -> Result<()> {
        // Clear screen for a fresh start
        print!("\x1B[2J\x1B[1;1H");

        // Colorful ASCII art logo
        println!(
            "{}",
            "╭─────────────────────────────────────────────────────────────╮".bright_cyan()
        );
        println!(
            "{}",
            "│                                                             │".bright_cyan()
        );
        println!(
            "{}",
            "│  ███████╗███╗   ██╗██╗   ██╗██╗  ██╗                        │".bright_cyan()
        );
        println!(
            "{}",
            "│  ██╔════╝████╗  ██║██║   ██║╚██╗██╔╝                        │".bright_cyan()
        );
        println!(
            "{}",
            "│  █████╗  ██╔██╗ ██║██║   ██║ ╚███╔╝                         │".bright_cyan()
        );
        println!(
            "{}",
            "│  ██╔══╝  ██║╚██╗██║╚██╗ ██╔╝ ██╔██╗                         │".bright_cyan()
        );
        println!(
            "{}",
            "│  ███████╗██║ ╚████║ ╚████╔╝  ██╔╝ ██╗                       │".bright_cyan()
        );
        println!(
            "{}",
            "│  ╚══════╝╚═╝  ╚═══╝  ╚═══╝   ╚═╝  ╚═╝                       │".bright_cyan()
        );
        println!(
            "{}",
            "│                                                             │".bright_cyan()
        );
        println!(
            "{}",
            format!(
                "│           Environment Variable Manager v{:<8}            │",
                env!("CARGO_PKG_VERSION")
            )
            .bright_cyan()
        );
        println!(
            "{}",
            "╰─────────────────────────────────────────────────────────────╯".bright_cyan()
        );

        println!(
            "\n{} {} {}",
            "✨".bright_yellow(),
            "Welcome to envx!".bright_white().bold(),
            "Your intelligent environment variable companion".bright_blue()
        );

        println!("\n{}", "━".repeat(65).bright_black());

        // Feature highlights with icons
        println!("\n{}", "This setup wizard will help you:".bright_white());

        let features = vec![
            (
                "📋",
                "Define environment variables",
                "Set up your project's environment",
            ),
            (
                "🚀",
                "Create profiles",
                "Configure dev, test, and production environments",
            ),
            ("📦", "Import existing files", "Seamlessly migrate from .env files"),
            ("📝", "Generate .env files", "Create .env files for each profile"),
            ("👥", "Enable team features", "Share configurations with your team"),
        ];

        for (icon, title, desc) in features {
            println!(
                "  {} {} {}",
                icon,
                format!("{title:<22}").bright_green().bold(),
                format!("─ {desc}").bright_black()
            );
        }

        println!("\n{}", "━".repeat(65).bright_black());

        // Updated estimated time
        println!(
            "\n{} {} {}",
            "⏱️ ".bright_blue(),
            "Estimated time:".bright_white(),
            "1-3 minutes".bright_yellow().bold()
        );

        // Note about ESC
        println!(
            "\n{} {}",
            "💡".bright_yellow(),
            "Tip: Press ESC at any time to exit the wizard".bright_white().italic()
        );

        // Interactive prompt with better styling
        println!(
            "\n{}",
            "Let's create the perfect setup for your project! 🎯".bright_magenta()
        );

        let continue_prompt = format!(
            "\n{} {} {} {} {}",
            "Type".bright_black(),
            "[y (yes)]".bright_green().bold(),
            "to begin your journey,".bright_black(),
            "[n (no)]".bright_red().bold(),
            "to skip, or [ESC] to exit".bright_black()
        );

        // Add a subtle animation effect
        print!("\n{}", "Initializing".bright_cyan());
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            print!("{}", ".".bright_cyan());
            std::io::Write::flush(&mut std::io::stdout())?;
        }
        println!(" {}", "Ready!".bright_green().bold());

        println!("{continue_prompt}");

        // Custom theme for this specific prompt
        let welcome_theme = ColorfulTheme {
            prompt_style: dialoguer::console::Style::new().cyan().bold(),
            ..ColorfulTheme::default()
        };

        let result = Confirm::with_theme(&welcome_theme)
            .with_prompt("")
            .default(true)
            .show_default(false)
            .wait_for_newline(true)
            .interact_opt()?;

        match result {
            Some(true) => {
                // Clear and show a motivational message
                println!(
                    "\n{} {}",
                    "🎉".bright_yellow(),
                    "Great choice! Let's build something amazing together."
                        .bright_green()
                        .bold()
                );
                std::thread::sleep(std::time::Duration::from_millis(1000));
                Ok(())
            }
            Some(false) => {
                println!(
                    "\n{} {}",
                    "👋".bright_yellow(),
                    "No problem! You can run 'envx init' anytime to set up.".bright_blue()
                );
                std::process::exit(0);
            }
            None => Err(EscPressed.into()),
        }
    }

    fn detect_system() -> Result<SystemInfo> {
        println!("\n🔍 Detecting your system...");

        let info = SystemInfo::detect()?;
        Ok(info)
    }

    fn show_system_info(info: &SystemInfo) {
        println!("✓ OS: {}", info.os);
        println!("✓ Shell: {}", info.shell);
        println!("✓ Terminal: {}", info.terminal);
        println!("✓ Envx Version: {}\n", env!("CARGO_PKG_VERSION"));
    }

    fn ask_team_setup(&self) -> Result<bool> {
        match Confirm::with_theme(&self.theme)
            .with_prompt("Are you working in a team?")
            .default(false)
            .interact_opt()?
        {
            Some(value) => Ok(value),
            None => Err(EscPressed.into()),
        }
    }

    fn ask_create_env_files(&self) -> Result<bool> {
        match Confirm::with_theme(&self.theme)
            .with_prompt("\nWould you like to create .env files for your profiles?")
            .default(true)
            .interact_opt()?
        {
            Some(value) => Ok(value),
            None => Err(EscPressed.into()),
        }
    }

    /// Prompts the user to select a project type from predefined options.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the selection (ESC key)
    /// - Custom project type creation fails
    pub fn select_project_type(&self) -> Result<ProjectType> {
        let options = vec![
            "Web Application (Node.js, React, etc.)",
            "Python Application",
            "Rust Application",
            "Docker/Container-based",
            "Multi-service/Microservices",
            "Other/Custom",
        ];

        let Some(selection) = Select::with_theme(&self.theme)
            .with_prompt("What type of project are you working on?")
            .items(&options)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        let project_type = match selection {
            0 => Self::create_web_app_type(),
            1 => Self::create_python_type(),
            2 => Self::create_rust_type(),
            3 => Self::create_docker_type(),
            4 => Self::create_microservices_type(),
            _ => self.create_custom_type()?,
        };

        Ok(project_type)
    }

    fn configure_variables(&mut self, project_type: &ProjectType) -> Result<Vec<SelectedVariable>> {
        let mut selected_vars = Vec::new();

        // First, handle predefined variables if any
        if !project_type.suggested_vars.is_empty() {
            println!("\n📋 Let's configure variables for your {} project:", project_type.name);

            let options: Vec<String> = project_type
                .suggested_vars
                .iter()
                .map(|var| {
                    let required_marker = if var.required { " (required)" } else { "" };
                    format!("{} - {}{}", var.name, var.description, required_marker)
                })
                .collect();

            let defaults: Vec<bool> = project_type.suggested_vars.iter().map(|var| var.required).collect();

            let Some(selections) = MultiSelect::with_theme(&self.theme)
                .with_prompt("Select variables to configure (Space to toggle, Enter to continue)")
                .items(&options)
                .defaults(&defaults)
                .interact_opt()?
            else {
                return Err(EscPressed.into());
            };

            // Configure values for selected variables
            if !selections.is_empty() {
                println!("\n🔧 Configure variable values:");
                for &idx in &selections {
                    let var = &project_type.suggested_vars[idx];

                    let value = Input::<String>::with_theme(&self.theme)
                        .with_prompt(format!("{} ({})", var.name, var.description))
                        .default(var.example.clone())
                        .interact()?;

                    selected_vars.push(SelectedVariable {
                        name: var.name.clone(),
                        value,
                        description: var.description.clone(),
                        required: var.required,
                    });
                }
            }
        }

        // Always ask about custom environment variables
        println!("\n➕ Custom Environment Variables");
        let Some(add_custom) = Confirm::with_theme(&self.theme)
            .with_prompt("Would you like to add custom environment variables?")
            .default(true)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        if add_custom {
            loop {
                println!("\n📝 Add a custom variable:");

                let var_name = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Variable name (or press Enter to finish)")
                    .allow_empty(true)
                    .interact()?;

                if var_name.is_empty() {
                    break;
                }

                let description = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Description")
                    .default(format!("{var_name} configuration"))
                    .interact()?;

                let value = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Value")
                    .default("your-value-here".to_string())
                    .interact()?;

                let Some(required) = Confirm::with_theme(&self.theme)
                    .with_prompt("Is this variable required?")
                    .default(false)
                    .interact_opt()?
                else {
                    return Err(EscPressed.into());
                };

                selected_vars.push(SelectedVariable {
                    name: var_name,
                    value,
                    description,
                    required,
                });

                let Some(add_more) = Confirm::with_theme(&self.theme)
                    .with_prompt("Add another custom variable?")
                    .default(true)
                    .interact_opt()?
                else {
                    return Err(EscPressed.into());
                };

                if !add_more {
                    break;
                }
            }
        }

        self.config.selected_vars.clone_from(&selected_vars);
        Ok(selected_vars)
    }

    fn create_and_configure_profiles(
        &self,
        project_type: &ProjectType,
        selected_vars: &[SelectedVariable],
    ) -> Result<(Vec<String>, HashMap<String, HashMap<String, String>>)> {
        println!("\n📁 Let's create environment profiles:");

        let mut profiles = Vec::new();
        let mut profile_configs = HashMap::new();

        // Show suggested profiles
        let suggested = &project_type.suggested_profiles;
        let mut options: Vec<String> = suggested
            .iter()
            .map(|p| {
                format!(
                    "{} ({})",
                    p,
                    match p.as_str() {
                        "development" => "local development",
                        "testing" => "running tests",
                        "staging" => "pre-production",
                        "production" => "live environment",
                        _ => "custom",
                    }
                )
            })
            .collect();

        options.push("Add custom profile".to_string());

        let defaults: Vec<bool> = vec![true, false, false, false]; // Default to dev only

        let Some(selections) = MultiSelect::with_theme(&self.theme)
            .with_prompt("Select profiles to create")
            .items(&options)
            .defaults(&defaults)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        // Process selections
        for &idx in &selections {
            if idx < suggested.len() {
                profiles.push(suggested[idx].clone());
            } else if idx == options.len() - 1 {
                // Add custom profile
                let custom_name = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Enter custom profile name")
                    .interact()?;

                if !custom_name.is_empty() {
                    profiles.push(custom_name);
                }
            }
        }

        // Allow adding more custom profiles
        loop {
            let Some(add_more) = Confirm::with_theme(&self.theme)
                .with_prompt("Add another custom profile?")
                .default(false)
                .interact_opt()?
            else {
                return Err(EscPressed.into());
            };

            if !add_more {
                break;
            }

            let custom_name = Input::<String>::with_theme(&self.theme)
                .with_prompt("Enter profile name")
                .interact()?;

            if !custom_name.is_empty() && !profiles.contains(&custom_name) {
                profiles.push(custom_name);
            }
        }

        // Configure each profile
        for profile in &profiles {
            println!("\n⚙️  Configuring '{profile}' profile:");
            let mut profile_config = HashMap::new();

            // Add selected variables to each profile with profile-specific values
            for var in selected_vars {
                let default_value = Self::get_profile_default_value(profile, &var.name, &var.value);

                let value = Input::<String>::with_theme(&self.theme)
                    .with_prompt(format!("  {}", var.name))
                    .default(default_value)
                    .interact()?;

                profile_config.insert(var.name.clone(), value);
            }

            profile_configs.insert(profile.clone(), profile_config);
        }

        Ok((profiles, profile_configs))
    }

    fn get_profile_default_value(profile: &str, var_name: &str, base_value: &str) -> String {
        match (profile, var_name) {
            ("development", "NODE_ENV") => "development".to_string(),
            ("testing", "NODE_ENV") => "test".to_string(),
            ("staging", "NODE_ENV") => "staging".to_string(),
            ("production", "NODE_ENV") => "production".to_string(),

            ("development", "DATABASE_URL") => base_value.replace("myapp", "myapp_dev"),
            ("testing", "DATABASE_URL") => base_value.replace("myapp", "myapp_test"),
            ("staging", "DATABASE_URL") => base_value.replace("myapp", "myapp_staging"),

            ("development", "LOG_LEVEL") => "debug".to_string(),
            ("testing", "LOG_LEVEL") => "error".to_string(),
            ("production", "LOG_LEVEL") => "info".to_string(),

            ("development", "DEBUG") => "true".to_string(),
            (_, "DEBUG") => "false".to_string(),

            _ => base_value.to_string(),
        }
    }

    /// Scans for existing environment files in the current directory.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File system operations fail during scanning
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the operation (ESC key)
    pub fn scan_existing_files(&self) -> Result<Option<Vec<PathBuf>>> {
        println!("\n🔍 Scanning for existing environment files...");

        let patterns = vec![".env", ".env.*", "docker-compose.yml", "docker-compose.yaml"];
        let mut found_files = Vec::new();

        for pattern in patterns {
            if let Ok(paths) = glob(pattern) {
                for path in paths.flatten() {
                    found_files.push(path);
                }
            }
        }

        if found_files.is_empty() {
            return Ok(None);
        }

        println!("Found existing environment files:");
        for (i, file) in found_files.iter().enumerate() {
            let var_count = Self::count_env_vars(file).unwrap_or(0);
            println!(
                "  {} {} ({} variables)",
                if i == 0 { "✓" } else { " " },
                file.display(),
                var_count
            );
        }

        let Some(import) = Confirm::with_theme(&self.theme)
            .with_prompt("\nWould you like to import these?")
            .default(true)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        if import { Ok(Some(found_files)) } else { Ok(None) }
    }

    fn count_env_vars(path: &Path) -> Result<usize> {
        let content = fs::read_to_string(path)?;
        let count = content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .filter(|line| line.contains('='))
            .count();
        Ok(count)
    }

    /// Imports selected existing environment files based on user choice.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the operation (ESC key)
    pub fn import_existing(&self, files: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
        let options: Vec<&str> = vec!["Import all", "Select files to import", "Skip import"];

        let Some(selection) = Select::with_theme(&self.theme)
            .with_prompt("Import option")
            .items(&options)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        match selection {
            0 => Ok(files),
            1 => {
                let file_names: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();

                let Some(selections) = MultiSelect::with_theme(&self.theme)
                    .with_prompt("Select files to import")
                    .items(&file_names)
                    .interact_opt()?
                else {
                    return Err(EscPressed.into());
                };

                Ok(selections.into_iter().map(|i| files[i].clone()).collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Configures team collaboration features for the project.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the operation (ESC key)
    /// - Repository root cannot be found when creating config path
    pub fn configure_team_features(&self) -> Result<TeamConfig> {
        println!("\n👥 Team Collaboration Setup:");

        let Some(create_config) = Confirm::with_theme(&self.theme)
            .with_prompt("Create .envx/config.yaml for team?")
            .default(true)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        let git_hooks = false;

        let ci_integration = false;

        let Some(shared_profiles) = Confirm::with_theme(&self.theme)
            .with_prompt("Enable shared profiles?")
            .default(true)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        let config_path = if create_config {
            let repo_root = Self::find_repo_root().unwrap_or_else(|_| PathBuf::from("."));
            repo_root.join(".envx").join("config.yaml")
        } else {
            PathBuf::from(".envx/config.yaml")
        };

        Ok(TeamConfig {
            config_path,
            git_hooks,
            ci_integration,
            shared_profiles,
        })
    }

    fn find_repo_root() -> Result<PathBuf> {
        let current = std::env::current_dir()?;
        let mut dir = current.as_path();

        loop {
            if dir.join(".git").exists() {
                return Ok(dir.to_path_buf());
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => return Err(eyre!("No git repository found")),
            }
        }
    }

    /// Configures validation rules for environment variables based on user preferences.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the operation (ESC key)
    /// - Custom pattern configuration fails
    pub fn configure_validation(&self, project_type: &ProjectType) -> Result<ValidationRules> {
        println!("\n✅ Configure Validation Rules:");

        let options = vec![
            "Require all variables in .envx/config.yaml",
            "Validate URLs are properly formatted",
            "Check numeric values are in valid ranges",
            "Warn about missing required variables",
            "Strict mode (fail on any validation error)",
        ];

        let defaults = vec![true, true, true, true, false]; // All except strict mode

        let Some(selections) = MultiSelect::with_theme(&self.theme)
            .with_prompt("Select validation rules")
            .items(&options)
            .defaults(&defaults)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        let rules = ValidationRules {
            require_all_defined: selections.contains(&0),
            validate_urls: selections.contains(&1),
            validate_numbers: selections.contains(&2),
            warn_missing: selections.contains(&3),
            strict_mode: selections.contains(&4),
            custom_patterns: self.get_custom_patterns(project_type)?,
        };

        Ok(rules)
    }

    fn get_custom_patterns(&self, project_type: &ProjectType) -> Result<HashMap<String, String>> {
        let mut patterns = HashMap::new();

        match &project_type.category {
            ProjectCategory::WebApp => {
                patterns.insert("*_URL".to_string(), r"^https?://.*".to_string());
                patterns.insert("*_PORT".to_string(), r"^[0-9]{1,5}$".to_string());
            }
            ProjectCategory::Docker => {
                patterns.insert("*_IMAGE".to_string(), r"^[a-z0-9\-_/:.]+$".to_string());
            }
            _ => {}
        }

        let Some(add_custom) = Confirm::with_theme(&self.theme)
            .with_prompt("\nAdd custom validation pattern?")
            .default(false)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        if add_custom {
            let pattern_name = Input::<String>::with_theme(&self.theme)
                .with_prompt("Pattern name (e.g., *_EMAIL)")
                .interact()?;

            let pattern_regex = Input::<String>::with_theme(&self.theme)
                .with_prompt("Regex pattern")
                .interact()?;

            patterns.insert(pattern_name, pattern_regex);
        }

        Ok(patterns)
    }

    /// Reviews the setup configuration with the user and applies it if confirmed.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User interaction fails (e.g., terminal issues)
    /// - User cancels the operation (ESC key)
    /// - Configuration application fails
    /// - File I/O operations fail during setup
    pub fn review_and_apply(&self, result: &SetupResult) -> Result<()> {
        println!("\n📋 Setup Summary:");
        println!("{}", "━".repeat(50));
        println!("Project Type:     {}", result.project_type.name);
        println!("Profiles:         {}", result.profiles.join(", "));
        println!("Variables:        {} configured", result.selected_vars.len());
        println!(
            "Create .env files: {}",
            if result.create_env_files { "Yes" } else { "No" }
        );
        println!(
            "Team Setup:       {}",
            if result.team_config.is_some() {
                "Enabled"
            } else {
                "Disabled"
            }
        );

        if !result.imported_files.is_empty() {
            println!("Imported Files:   {}", result.imported_files.len());
        }

        println!("{}", "━".repeat(50));

        let Some(confirm) = Confirm::with_theme(&self.theme)
            .with_prompt("\nReady to apply configuration?")
            .default(true)
            .interact_opt()?
        else {
            return Err(EscPressed.into());
        };

        if !confirm {
            return Err(eyre!("Setup cancelled by user"));
        }

        // Apply the configuration
        self.apply_configuration(result)?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn apply_configuration(&self, result: &SetupResult) -> Result<()> {
        println!("\n🚀 Applying configuration...");

        // Create project config
        if let Some(team_config) = &result.team_config {
            Self::create_project_config(result, &team_config.config_path)?;
            println!("✓ Created project configuration");
        }

        // Import files
        for file in &result.imported_files {
            println!("✓ Imported {}", file.display());
            // Actual import logic would go here
        }

        // Create profiles in ProfileManager
        let mut profile_manager = ProfileManager::new()?;

        // Check for existing profiles and handle conflicts
        let mut profile_mappings: HashMap<String, String> = HashMap::new();
        for profile_name in &result.profiles {
            profile_mappings.insert(profile_name.clone(), profile_name.clone());
        }

        for (profile_name, _) in &result.profile_configs {
            if profile_manager.get(profile_name).is_some() {
                println!("\n⚠️  Profile '{profile_name}' already exists!");

                let options = vec![
                    format!("Rename new profile (current: {})", profile_name),
                    format!("Delete existing '{}' profile and replace", profile_name),
                    "Skip this profile".to_string(),
                ];

                let Some(choice) = Select::with_theme(&self.theme)
                    .with_prompt("How would you like to proceed?")
                    .items(&options)
                    .interact_opt()?
                else {
                    return Err(EscPressed.into());
                };

                match choice {
                    0 => {
                        // Rename new profile
                        loop {
                            let new_name = Input::<String>::with_theme(&self.theme)
                                .with_prompt("Enter new profile name")
                                .default(format!("{profile_name}_new"))
                                .interact()?;

                            if new_name.is_empty() {
                                println!("Profile name cannot be empty!");
                                continue;
                            }

                            if profile_manager.get(&new_name).is_none() {
                                profile_mappings.insert(profile_name.clone(), new_name);
                                break;
                            }
                            println!("Profile '{new_name}' also exists! Please choose another name.");
                        }
                    }
                    1 => {
                        // Delete existing profile
                        let Some(confirm_delete) = Confirm::with_theme(&self.theme)
                            .with_prompt(format!(
                                "Are you sure you want to delete the existing '{profile_name}' profile?"
                            ))
                            .default(false)
                            .interact_opt()?
                        else {
                            return Err(EscPressed.into());
                        };

                        if confirm_delete {
                            profile_manager.delete(profile_name)?;
                            println!("✓ Deleted existing profile: {profile_name}");
                        } else {
                            println!("Skipping profile: {profile_name}");
                            profile_mappings.remove(profile_name);
                        }
                    }
                    2 => {
                        // Skip this profile
                        println!("Skipping profile: {profile_name}");
                        profile_mappings.remove(profile_name);
                    }
                    _ => unreachable!(),
                }
            }
        }

        // Create profiles with resolved names
        for (original_name, actual_name) in &profile_mappings {
            if let Some(profile_vars) = result.profile_configs.get(original_name) {
                // Create the profile
                profile_manager.create(actual_name.clone(), Some(format!("{actual_name} environment")))?;

                // Add variables to the profile
                if let Some(profile) = profile_manager.get_mut(actual_name) {
                    for (var_name, var_value) in profile_vars {
                        profile.add_var(var_name.clone(), var_value.clone(), false);
                    }
                }

                if original_name == actual_name {
                    println!("✓ Created profile: {actual_name}");
                } else {
                    println!("✓ Created profile: {actual_name} (renamed from {original_name})");
                }
            }
        }

        // Set the first profile as active (typically "development")
        // Use the mapped name in case it was renamed
        if let Some(first_profile) = result.profiles.first() {
            if let Some(actual_name) = profile_mappings.get(first_profile) {
                profile_manager.switch(actual_name)?;
                println!("✓ Set active profile: {actual_name}");
            }
        }

        // Create .env files if requested
        if result.create_env_files {
            Self::create_env_files_with_mappings(result, &profile_mappings)?;
        }

        // Set environment variables in the current session
        for var in &result.selected_vars {
            unsafe { std::env::set_var(&var.name, &var.value) };
            println!("✓ Set {} in current session", var.name);
        }

        Ok(())
    }

    fn create_env_files_with_mappings(result: &SetupResult, mappings: &HashMap<String, String>) -> Result<()> {
        println!("\n📝 Creating .env files...");

        for (original_name, config) in &result.profile_configs {
            if let Some(actual_name) = mappings.get(original_name) {
                let filename = if actual_name == "development" {
                    ".env".to_string()
                } else {
                    format!(".env.{actual_name}")
                };

                let mut content = String::new();
                content.push_str(&format!("# Environment variables for {actual_name} profile\n"));
                if original_name != actual_name {
                    content.push_str(&format!("# (originally configured as {original_name})\n"));
                }
                content.push_str(&format!(
                    "# Generated by envx on {}\n\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                ));

                for (key, value) in config {
                    content.push_str(&format!("{key}={value}\n"));
                }

                fs::write(&filename, content)?;
                println!("✓ Created {filename}");
            }
        }

        Ok(())
    }

    fn check_required_variables(result: &SetupResult) {
        println!("\n🔍 Checking required environment variables...");

        let mut missing_vars = Vec::new();
        let mut set_vars = Vec::new();

        for var in &result.selected_vars {
            if var.required {
                match std::env::var(&var.name) {
                    Ok(value) => {
                        if value == var.value {
                            set_vars.push(&var.name);
                        } else {
                            missing_vars.push(&var.name);
                        }
                    }
                    Err(_) => missing_vars.push(&var.name),
                }
            }
        }

        if !set_vars.is_empty() {
            println!("\n✅ Successfully set in current session:");
            for var in set_vars {
                println!("   ✓ {var}");
            }
        }

        if !missing_vars.is_empty() {
            println!("\n⚠️  The following required variables need a terminal restart to take effect:");
            for var in missing_vars {
                println!("   • {var}");
            }

            println!("\n💡 To apply these variables:");
            println!("   1. Close and restart your terminal");
            println!("   2. Run 'envx list' to verify they are set");
            println!("   3. Or source the .env file: source .env");
        } else if result.selected_vars.iter().any(|v| v.required) {
            println!("\n✅ All required variables are set!");
        }

        println!("\n✅ Setup complete! Here's what to do next:");
        println!("\n  1. Run `envx list` to see your environment variables");
        println!("  2. Run `envx tui` to launch the interactive interface");
        println!("  3. Run `envx profile list` to see available profiles");
        println!("  4. Run `envx profile set <name>` to switch profiles");

        if result.team_config.is_some() {
            println!("  5. Commit .envx/config.yaml to share with your team");
        }
    }

    fn create_project_config(result: &SetupResult, path: &Path) -> Result<()> {
        // Create directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let config = ProjectConfig {
            name: Some(result.project_type.name.to_lowercase().replace(' ', "-")),
            description: Some(format!("{} project", result.project_type.name)),
            required: result
                .selected_vars
                .iter()
                .filter(|v| v.required)
                .map(|v| RequiredVar {
                    name: v.name.clone(),
                    description: Some(v.description.clone()),
                    pattern: None,
                    example: Some(v.value.clone()),
                })
                .collect(),
            defaults: result
                .selected_vars
                .iter()
                .map(|v| (v.name.clone(), v.value.clone()))
                .collect(),
            auto_load: vec![".env".to_string(), ".env.local".to_string()],
            profile: result.profiles.first().cloned(),
            scripts: HashMap::new(),
            validation: ConfigValidationRules {
                warn_unused: result.validation_rules.warn_missing,
                strict_names: result.validation_rules.strict_mode,
                patterns: result.validation_rules.custom_patterns.clone(),
            },
            inherit: true,
        };

        let yaml = serde_yaml::to_string(&config)?;
        fs::write(path, yaml)?;

        Ok(())
    }

    // ... rest of the project type creation methods remain the same ...
    fn create_web_app_type() -> ProjectType {
        ProjectType {
            name: "Web Application".to_string(),
            category: ProjectCategory::WebApp,
            suggested_vars: vec![
                SuggestedVariable {
                    name: "NODE_ENV".to_string(),
                    description: "Application environment".to_string(),
                    example: "development".to_string(),
                    required: true,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "PORT".to_string(),
                    description: "Server port".to_string(),
                    example: "3000".to_string(),
                    required: true,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "DATABASE_URL".to_string(),
                    description: "Database connection string".to_string(),
                    example: "postgresql://localhost:5432/myapp".to_string(),
                    required: true,
                    sensitive: true,
                },
                SuggestedVariable {
                    name: "JWT_SECRET".to_string(),
                    description: "JWT signing secret".to_string(),
                    example: "your-secret-key".to_string(),
                    required: false,
                    sensitive: true,
                },
                SuggestedVariable {
                    name: "API_KEY".to_string(),
                    description: "External API key".to_string(),
                    example: "your-api-key".to_string(),
                    required: false,
                    sensitive: true,
                },
            ],
            suggested_profiles: vec![
                "development".to_string(),
                "testing".to_string(),
                "production".to_string(),
            ],
        }
    }

    fn create_python_type() -> ProjectType {
        ProjectType {
            name: "Python Application".to_string(),
            category: ProjectCategory::Python,
            suggested_vars: vec![
                SuggestedVariable {
                    name: "PYTHONPATH".to_string(),
                    description: "Python module search path".to_string(),
                    example: "./src".to_string(),
                    required: false,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "DATABASE_URL".to_string(),
                    description: "Database connection string".to_string(),
                    example: "postgresql://localhost:5432/myapp".to_string(),
                    required: true,
                    sensitive: true,
                },
                SuggestedVariable {
                    name: "SECRET_KEY".to_string(),
                    description: "Django/Flask secret key".to_string(),
                    example: "your-secret-key".to_string(),
                    required: true,
                    sensitive: true,
                },
                SuggestedVariable {
                    name: "DEBUG".to_string(),
                    description: "Debug mode flag".to_string(),
                    example: "True".to_string(),
                    required: false,
                    sensitive: false,
                },
            ],
            suggested_profiles: vec![
                "development".to_string(),
                "testing".to_string(),
                "production".to_string(),
            ],
        }
    }

    fn create_rust_type() -> ProjectType {
        ProjectType {
            name: "Rust Application".to_string(),
            category: ProjectCategory::Rust,
            suggested_vars: vec![
                SuggestedVariable {
                    name: "RUST_LOG".to_string(),
                    description: "Rust logging level".to_string(),
                    example: "info".to_string(),
                    required: false,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "DATABASE_URL".to_string(),
                    description: "Database connection string".to_string(),
                    example: "postgresql://localhost:5432/myapp".to_string(),
                    required: true,
                    sensitive: true,
                },
                SuggestedVariable {
                    name: "SERVER_PORT".to_string(),
                    description: "Server port".to_string(),
                    example: "8080".to_string(),
                    required: true,
                    sensitive: false,
                },
            ],
            suggested_profiles: vec!["development".to_string(), "release".to_string()],
        }
    }

    fn create_docker_type() -> ProjectType {
        ProjectType {
            name: "Docker Application".to_string(),
            category: ProjectCategory::Docker,
            suggested_vars: vec![
                SuggestedVariable {
                    name: "COMPOSE_PROJECT_NAME".to_string(),
                    description: "Docker Compose project name".to_string(),
                    example: "myapp".to_string(),
                    required: true,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "DOCKER_REGISTRY".to_string(),
                    description: "Docker registry URL".to_string(),
                    example: "docker.io".to_string(),
                    required: false,
                    sensitive: false,
                },
            ],
            suggested_profiles: vec!["local".to_string(), "staging".to_string(), "production".to_string()],
        }
    }

    fn create_microservices_type() -> ProjectType {
        ProjectType {
            name: "Microservices".to_string(),
            category: ProjectCategory::Microservices,
            suggested_vars: vec![
                SuggestedVariable {
                    name: "SERVICE_DISCOVERY_URL".to_string(),
                    description: "Service discovery endpoint".to_string(),
                    example: "http://consul:8500".to_string(),
                    required: true,
                    sensitive: false,
                },
                SuggestedVariable {
                    name: "KAFKA_BROKERS".to_string(),
                    description: "Kafka broker addresses".to_string(),
                    example: "kafka1:9092,kafka2:9092".to_string(),
                    required: false,
                    sensitive: false,
                },
            ],
            suggested_profiles: vec!["local".to_string(), "kubernetes".to_string(), "production".to_string()],
        }
    }

    fn create_custom_type(&self) -> Result<ProjectType> {
        let value = Input::<String>::with_theme(&self.theme)
            .with_prompt("Enter project type name")
            .default("Custom Project".to_string())
            .interact()?;
        let name = value;

        // For custom projects, we'll skip the predefined variables
        // and let the user add all variables as custom
        Ok(ProjectType {
            name,
            category: ProjectCategory::Custom,
            suggested_vars: Vec::new(), // Empty, so user can add all as custom
            suggested_profiles: vec!["development".to_string(), "production".to_string()],
        })
    }
}

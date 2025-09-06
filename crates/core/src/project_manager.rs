use crate::project_config::ProjectConfig;
use crate::{EnvVarManager, ProfileManager, ValidationRules};
use ahash::AHashMap as HashMap;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ProjectManager {
    config_dir: PathBuf,
    config: Option<ProjectConfig>,
    current_dir: PathBuf,
}

impl ProjectManager {
    /// Create a new `ProjectManager` instance
    ///
    /// # Errors
    ///
    /// This function will return an error if getting the current directory fails.
    pub fn new() -> Result<Self> {
        Ok(Self {
            config_dir: PathBuf::from(".envx"),
            config: None,
            current_dir: std::env::current_dir()?,
        })
    }

    /// Initialize a new project configuration
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Creating the .envx directory fails
    /// - Saving the configuration file fails
    /// - Writing the .gitignore file fails
    pub fn init(&self, name: Option<String>) -> Result<()> {
        // Create .envx directory
        fs::create_dir_all(&self.config_dir)?;

        // Create config.yaml
        let config = ProjectConfig::new(name);
        let config_path = self.config_dir.join("config.yaml");
        config.save(&config_path)?;

        // Create .gitignore for .envx directory
        let gitignore_path = self.config_dir.join(".gitignore");
        fs::write(gitignore_path, "local/\n*.local.yaml\n")?;

        println!("✅ Initialized envx project configuration");
        println!("📁 Created .envx/config.yaml");

        Ok(())
    }

    /// Initialize a new project with a custom configuration file
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Creating parent directories fails
    /// - Saving the configuration file fails
    pub fn init_with_file(&self, name: Option<String>, file_path: &Path) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let project_name = name
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            })
            .unwrap_or_else(|| "my-project".to_string());

        let config = ProjectConfig {
            name: Some(project_name.clone()),
            description: Some(format!("{project_name} environment configuration")),
            required: vec![],
            defaults: HashMap::new(),
            auto_load: vec![".env".to_string()],
            profile: None,
            scripts: HashMap::new(),
            validation: ValidationRules::default(),
            inherit: true,
        };

        config.save(file_path)?;
        println!("✅ Initialized project '{}' at {}", project_name, file_path.display());

        Ok(())
    }

    /// Find and load project configuration
    ///
    /// # Errors
    ///
    /// This function will return an error if loading the project configuration file fails.
    pub fn find_and_load(&mut self) -> Result<Option<PathBuf>> {
        let mut current = self.current_dir.clone();

        loop {
            let config_path = current.join(".envx").join("config.yaml");
            if config_path.exists() {
                self.config = Some(ProjectConfig::load(&config_path)?);
                return Ok(Some(current));
            }

            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    /// Apply project configuration
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - No project configuration is loaded
    /// - Profile application fails
    /// - Loading environment files fails
    /// - Setting environment variables fails
    pub fn apply(&self, manager: &mut EnvVarManager, profile_manager: &mut ProfileManager) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("No project configuration loaded"))?;

        // Apply profile if specified
        if let Some(profile_name) = &config.profile {
            profile_manager.apply(profile_name, manager)?;
        }

        // Load auto-load files
        for file in &config.auto_load {
            let file_path = self.current_dir.join(file);
            if file_path.exists() {
                Self::load_env_file(&file_path, manager)?;
            }
        }

        // Apply defaults (only if variable not already set)
        for (name, value) in &config.defaults {
            if manager.get(name).is_none() {
                manager.set(name, value, true)?;
            }
        }

        Ok(())
    }

    /// Load configuration from a specific file
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The configuration file does not exist
    /// - Loading the project configuration file fails
    pub fn load_from_file(&mut self, file_path: &Path) -> Result<()> {
        if !file_path.exists() {
            return Err(eyre!("Configuration file not found: {}", file_path.display()));
        }

        self.config = Some(ProjectConfig::load(file_path)?);
        self.config_dir = file_path.to_path_buf();

        Ok(())
    }

    /// Validate required variables
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - No project configuration is loaded
    /// - Regex compilation fails for pattern validation
    pub fn validate(&self, manager: &EnvVarManager) -> Result<ValidationReport> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("No project configuration loaded"))?;

        let mut report = ValidationReport::default();

        // Check required variables
        for required in &config.required {
            match manager.get(&required.name) {
                Some(var) => {
                    // Validate pattern if specified
                    if let Some(pattern) = &required.pattern {
                        let re = Regex::new(pattern)?;
                        if !re.is_match(&var.value) {
                            report.errors.push(ValidationError {
                                var_name: required.name.clone(),
                                error_type: ErrorType::PatternMismatch,
                                message: format!("Value does not match pattern: {pattern}"),
                            });
                        }
                    }
                    report.found.push(required.name.clone());
                }
                None => {
                    report.missing.push(MissingVar {
                        name: required.name.clone(),
                        description: required.description.clone(),
                        example: required.example.clone(),
                    });
                }
            }
        }

        // Check validation rules
        if config.validation.strict_names {
            for var in manager.list() {
                if !is_valid_var_name(&var.name) {
                    report.warnings.push(ValidationWarning {
                        var_name: var.name.clone(),
                        message: "Invalid variable name format".to_string(),
                    });
                }
            }
        }

        report.success = report.errors.is_empty() && report.missing.is_empty();
        Ok(report)
    }

    /// Run a project script
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - No project configuration is loaded
    /// - The specified script is not found in the configuration
    /// - Setting environment variables fails
    /// - The script execution fails
    pub fn run_script(&self, script_name: &str, manager: &mut EnvVarManager) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("No project configuration loaded"))?;

        let script = config
            .scripts
            .get(script_name)
            .ok_or_else(|| color_eyre::eyre::eyre!("Script '{}' not found", script_name))?;

        // Apply script-specific environment variables
        for (name, value) in &script.env {
            manager.set(name, value, false)?;
        }

        // Execute the script
        #[cfg(unix)]
        {
            std::process::Command::new("sh").arg("-c").arg(&script.run).status()?;
        }

        #[cfg(windows)]
        {
            std::process::Command::new("cmd").arg("/C").arg(&script.run).status()?;
        }

        Ok(())
    }

    fn load_env_file(path: &Path, manager: &mut EnvVarManager) -> Result<()> {
        let content = fs::read_to_string(path)?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                manager.set(key, value, true)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub success: bool,
    pub missing: Vec<MissingVar>,
    pub found: Vec<String>,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

#[derive(Debug)]
pub struct MissingVar {
    pub name: String,
    pub description: Option<String>,
    pub example: Option<String>,
}

#[derive(Debug)]
pub struct ValidationError {
    pub var_name: String,
    pub error_type: ErrorType,
    pub message: String,
}

#[derive(Debug)]
pub enum ErrorType {
    PatternMismatch,
    InvalidValue,
}

#[derive(Debug)]
pub struct ValidationWarning {
    pub var_name: String,
    pub message: String,
}

fn is_valid_var_name(name: &str) -> bool {
    // Unix/Windows compatible variable name
    let re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    re.is_match(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_config::{RequiredVar, Script};
    use ahash::AHashMap as HashMap;
    use tempfile::TempDir;

    fn create_test_project_manager() -> (ProjectManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let current_dir = temp_dir.path().to_path_buf();

        let manager = ProjectManager {
            config_dir: current_dir.join(".envx"),
            config: None,
            current_dir: current_dir.clone(),
        };

        (manager, temp_dir)
    }

    fn create_test_env_manager() -> EnvVarManager {
        let mut manager = EnvVarManager::new();
        manager.set("EXISTING_VAR", "existing_value", false).unwrap();
        manager
    }

    fn create_test_profile_manager() -> ProfileManager {
        ProfileManager::new().unwrap()
    }

    fn create_test_config() -> ProjectConfig {
        let mut config = ProjectConfig::new(Some("test-project".to_string()));

        // Add required variables
        config.required.push(RequiredVar {
            name: "DATABASE_URL".to_string(),
            description: Some("Database connection string".to_string()),
            pattern: Some(r"^(postgresql|mysql)://.*".to_string()),
            example: Some("postgresql://localhost/mydb".to_string()),
        });

        config.required.push(RequiredVar {
            name: "API_KEY".to_string(),
            description: Some("API authentication key".to_string()),
            pattern: None,
            example: None,
        });

        // Add defaults
        config
            .defaults
            .insert("NODE_ENV".to_string(), "development".to_string());
        config.defaults.insert("PORT".to_string(), "3000".to_string());

        // Add scripts
        let mut script_env = HashMap::new();
        script_env.insert("NODE_ENV".to_string(), "test".to_string());

        config.scripts.insert(
            "test".to_string(),
            Script {
                description: Some("Run tests".to_string()),
                run: "echo Running tests".to_string(),
                env: script_env,
            },
        );

        config
    }

    #[test]
    fn test_project_manager_new() {
        let result = ProjectManager::new();
        assert!(result.is_ok());

        let manager = result.unwrap();
        assert_eq!(manager.config_dir, PathBuf::from(".envx"));
        assert!(manager.config.is_none());
    }

    #[test]
    fn test_init_creates_structure() {
        let (manager, temp_dir) = create_test_project_manager();

        let result = manager.init(Some("test-project".to_string()));
        assert!(result.is_ok());

        // Verify .envx directory exists
        let envx_dir = temp_dir.path().join(".envx");
        assert!(envx_dir.exists());
        assert!(envx_dir.is_dir());

        // Verify config.yaml exists
        let config_path = envx_dir.join("config.yaml");
        assert!(config_path.exists());

        // Verify .gitignore exists
        let gitignore_path = envx_dir.join(".gitignore");
        assert!(gitignore_path.exists());

        // Verify gitignore content
        let gitignore_content = fs::read_to_string(gitignore_path).unwrap();
        assert!(gitignore_content.contains("local/"));
        assert!(gitignore_content.contains("*.local.yaml"));
    }

    #[test]
    fn test_init_creates_valid_config() {
        let (manager, temp_dir) = create_test_project_manager();

        manager.init(Some("my-app".to_string())).unwrap();

        let config_path = temp_dir.path().join(".envx").join("config.yaml");
        let config = ProjectConfig::load(&config_path).unwrap();

        assert_eq!(config.name, Some("my-app".to_string()));
        assert!(config.required.is_empty());
        assert!(config.defaults.is_empty());
        assert_eq!(config.auto_load, vec![".env".to_string()]);
    }

    #[test]
    fn test_find_and_load_in_current_dir() {
        let (mut manager, temp_dir) = create_test_project_manager();

        // Create config in current dir
        manager.init(Some("test".to_string())).unwrap();

        let result = manager.find_and_load();
        assert!(result.is_ok());

        let found_path = result.unwrap();
        assert!(found_path.is_some());
        assert_eq!(found_path.unwrap(), temp_dir.path());
        assert!(manager.config.is_some());
    }

    #[test]
    fn test_find_and_load_in_parent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let parent_dir = temp_dir.path();
        let child_dir = parent_dir.join("subdir");
        fs::create_dir(&child_dir).unwrap();

        // Create config in parent
        let parent_manager = ProjectManager {
            config_dir: parent_dir.join(".envx"),
            config: None,
            current_dir: parent_dir.to_path_buf(),
        };
        parent_manager.init(Some("parent-project".to_string())).unwrap();

        // Try to find from child
        let mut child_manager = ProjectManager {
            config_dir: PathBuf::from(".envx"),
            config: None,
            current_dir: child_dir,
        };

        let result = child_manager.find_and_load();
        assert!(result.is_ok());

        let found_path = result.unwrap();
        assert!(found_path.is_some());
        assert_eq!(found_path.unwrap(), parent_dir);
        assert!(child_manager.config.is_some());
    }

    #[test]
    fn test_find_and_load_not_found() {
        let (mut manager, _temp) = create_test_project_manager();

        let result = manager.find_and_load();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        assert!(manager.config.is_none());
    }

    #[test]
    fn test_apply_loads_env_files() {
        let (mut manager, temp_dir) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();
        let mut profile_manager = create_test_profile_manager();

        // Create .env file
        let env_content = "TEST_VAR=test_value\nANOTHER_VAR=another_value";
        fs::write(temp_dir.path().join(".env"), env_content).unwrap();

        // Create config with auto_load
        let mut config = create_test_config();
        config.auto_load = vec![".env".to_string()];
        manager.config = Some(config);

        let result = manager.apply(&mut env_manager, &mut profile_manager);
        assert!(result.is_ok());

        assert_eq!(env_manager.get("TEST_VAR").unwrap().value, "test_value");
        assert_eq!(env_manager.get("ANOTHER_VAR").unwrap().value, "another_value");
    }

    #[test]
    fn test_apply_sets_defaults() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();
        let mut profile_manager = create_test_profile_manager();

        manager.config = Some(create_test_config());

        let result = manager.apply(&mut env_manager, &mut profile_manager);
        assert!(result.is_ok());

        assert_eq!(env_manager.get("NODE_ENV").unwrap().value, "development");
        assert_eq!(env_manager.get("PORT").unwrap().value, "3000");
    }

    #[test]
    fn test_apply_doesnt_override_existing() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();
        let mut profile_manager = create_test_profile_manager();

        // Set a variable that's also in defaults
        env_manager.set("NODE_ENV", "production", false).unwrap();

        manager.config = Some(create_test_config());

        manager.apply(&mut env_manager, &mut profile_manager).unwrap();

        // Should not override existing value
        assert_eq!(env_manager.get("NODE_ENV").unwrap().value, "production");
    }

    #[test]
    fn test_apply_no_config_error() {
        let (manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();
        let mut profile_manager = create_test_profile_manager();

        let result = manager.apply(&mut env_manager, &mut profile_manager);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No project configuration loaded")
        );
    }

    #[test]
    fn test_validate_all_present_and_valid() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        // Set required variables
        env_manager
            .set("DATABASE_URL", "postgresql://localhost/mydb", false)
            .unwrap();
        env_manager.set("API_KEY", "secret-key", false).unwrap();

        manager.config = Some(create_test_config());

        let report = manager.validate(&env_manager).unwrap();
        assert!(report.success);
        assert!(report.missing.is_empty());
        assert!(report.errors.is_empty());
        assert_eq!(report.found.len(), 2);
    }

    #[test]
    fn test_validate_missing_variables() {
        let (mut manager, _temp) = create_test_project_manager();
        let env_manager = create_test_env_manager();

        manager.config = Some(create_test_config());

        let report = manager.validate(&env_manager).unwrap();
        assert!(!report.success);
        assert_eq!(report.missing.len(), 2);

        let missing_names: Vec<&str> = report.missing.iter().map(|m| m.name.as_str()).collect();
        assert!(missing_names.contains(&"DATABASE_URL"));
        assert!(missing_names.contains(&"API_KEY"));
    }

    #[test]
    fn test_validate_pattern_mismatch() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        // Set with invalid pattern
        env_manager.set("DATABASE_URL", "invalid-url", false).unwrap();
        env_manager.set("API_KEY", "valid-key", false).unwrap();

        manager.config = Some(create_test_config());

        let report = manager.validate(&env_manager).unwrap();
        assert!(!report.success);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].var_name, "DATABASE_URL");
        assert!(matches!(report.errors[0].error_type, ErrorType::PatternMismatch));
    }

    #[test]
    fn test_validate_strict_names() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        // Set variable with invalid name
        env_manager.vars.insert(
            "invalid-name".to_string(),
            crate::EnvVar {
                name: "invalid-name".to_string(),
                value: "value".to_string(),
                source: crate::EnvVarSource::User,
                modified: chrono::Utc::now(),
                original_value: None,
            },
        );

        let mut config = create_test_config();
        config.validation.strict_names = true;
        manager.config = Some(config);

        let report = manager.validate(&env_manager).unwrap();
        assert!(!report.warnings.is_empty());
        assert!(report.warnings.iter().any(|w| w.var_name == "invalid-name"));
    }

    #[test]
    fn test_run_script_success() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        manager.config = Some(create_test_config());

        let result = manager.run_script("test", &mut env_manager);
        assert!(result.is_ok());

        // Verify script environment was applied
        assert_eq!(env_manager.get("NODE_ENV").unwrap().value, "test");
    }

    #[test]
    fn test_run_script_not_found() {
        let (mut manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        manager.config = Some(create_test_config());

        let result = manager.run_script("nonexistent", &mut env_manager);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Script 'nonexistent' not found")
        );
    }

    #[test]
    fn test_run_script_no_config() {
        let (manager, _temp) = create_test_project_manager();
        let mut env_manager = create_test_env_manager();

        let result = manager.run_script("test", &mut env_manager);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No project configuration loaded")
        );
    }

    #[test]
    fn test_load_env_file_basic() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");
        let mut env_manager = create_test_env_manager();

        let content = r#"
# Comment line
VAR1=value1
VAR2="quoted value"
VAR3='single quoted'
EMPTY_LINE_ABOVE=yes

# Another comment
VAR_WITH_SPACES = spaced value
"#;
        fs::write(&env_path, content).unwrap();

        ProjectManager::load_env_file(&env_path, &mut env_manager).unwrap();

        assert_eq!(env_manager.get("VAR1").unwrap().value, "value1");
        assert_eq!(env_manager.get("VAR2").unwrap().value, "quoted value");
        assert_eq!(env_manager.get("VAR3").unwrap().value, "single quoted");
        assert_eq!(env_manager.get("EMPTY_LINE_ABOVE").unwrap().value, "yes");
        assert_eq!(env_manager.get("VAR_WITH_SPACES").unwrap().value, "spaced value");
    }

    #[test]
    fn test_load_env_file_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");
        let mut env_manager = create_test_env_manager();

        let content = r"
# Edge cases
EMPTY_VALUE=
EQUALS_IN_VALUE=key=value=more
URL=https://example.com/path?query=value
MULTILINE_ATTEMPT=line1\nline2
SPECIAL_CHARS=!@#$%^&*()
";
        fs::write(&env_path, content).unwrap();

        ProjectManager::load_env_file(&env_path, &mut env_manager).unwrap();

        assert_eq!(env_manager.get("EMPTY_VALUE").unwrap().value, "");
        assert_eq!(env_manager.get("EQUALS_IN_VALUE").unwrap().value, "key=value=more");
        assert_eq!(
            env_manager.get("URL").unwrap().value,
            "https://example.com/path?query=value"
        );
        assert_eq!(env_manager.get("MULTILINE_ATTEMPT").unwrap().value, "line1\\nline2");
        assert_eq!(env_manager.get("SPECIAL_CHARS").unwrap().value, "!@#$%^&*()");
    }

    #[test]
    fn test_load_env_file_not_found() {
        let mut env_manager = create_test_env_manager();
        let result = ProjectManager::load_env_file(Path::new("/nonexistent/.env"), &mut env_manager);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid_var_name() {
        // Valid names
        assert!(is_valid_var_name("VAR"));
        assert!(is_valid_var_name("VAR_NAME"));
        assert!(is_valid_var_name("_PRIVATE"));
        assert!(is_valid_var_name("var123"));
        assert!(is_valid_var_name("V"));
        assert!(is_valid_var_name("VERY_LONG_VARIABLE_NAME_WITH_UNDERSCORES"));

        // Invalid names
        assert!(!is_valid_var_name("123VAR")); // Starts with number
        assert!(!is_valid_var_name("VAR-NAME")); // Contains dash
        assert!(!is_valid_var_name("VAR NAME")); // Contains space
        assert!(!is_valid_var_name("VAR.NAME")); // Contains dot
        assert!(!is_valid_var_name("")); // Empty
        assert!(!is_valid_var_name("VAR$")); // Contains special char
        assert!(!is_valid_var_name("@VAR")); // Starts with special char
    }

    #[test]
    fn test_validation_report_success_calculation() {
        let mut report = ValidationReport::default();
        // Default should be false because Default trait initializes bool as false
        assert!(!report.success);

        // Manually set success based on empty errors and missing
        report.success = report.errors.is_empty() && report.missing.is_empty();
        assert!(report.success); // Now it should be true since both are empty

        // Add missing variable
        report.missing.push(MissingVar {
            name: "VAR".to_string(),
            description: None,
            example: None,
        });
        report.success = report.errors.is_empty() && report.missing.is_empty();
        assert!(!report.success);

        // Clear missing
        report.missing.clear();
        report.success = report.errors.is_empty() && report.missing.is_empty();
        assert!(report.success);

        // Add error
        report.errors.push(ValidationError {
            var_name: "VAR".to_string(),
            error_type: ErrorType::PatternMismatch,
            message: "error".to_string(),
        });
        report.success = report.errors.is_empty() && report.missing.is_empty();
        assert!(!report.success);

        // Clear everything
        report.errors.clear();
        report.missing.clear();
        report.success = report.errors.is_empty() && report.missing.is_empty();
        assert!(report.success);
    }
}

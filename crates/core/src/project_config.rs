use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name
    pub name: Option<String>,

    /// Project description
    pub description: Option<String>,

    /// Required environment variables
    pub required: Vec<RequiredVar>,

    /// Default values for variables
    pub defaults: HashMap<String, String>,

    /// Files to auto-load (in order)
    pub auto_load: Vec<String>,

    /// Profile to activate
    pub profile: Option<String>,

    /// Scripts to run
    pub scripts: HashMap<String, Script>,

    /// Validation rules
    pub validation: ValidationRules,

    /// Inherit from parent directories
    pub inherit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredVar {
    pub name: String,
    pub description: Option<String>,
    pub pattern: Option<String>, // Regex pattern
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub description: Option<String>,
    pub run: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationRules {
    /// Warn about unused variables
    pub warn_unused: bool,

    /// Error on invalid variable names
    pub strict_names: bool,

    /// Custom validation patterns
    pub patterns: HashMap<String, String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            required: Vec::new(),
            defaults: HashMap::new(),
            auto_load: vec![".env".to_string()],
            profile: None,
            scripts: HashMap::new(),
            validation: ValidationRules::default(),
            inherit: true,
        }
    }
}

impl ProjectConfig {
    #[must_use]
    pub fn new(name: Option<String>) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    pub fn add_required(&mut self, name: String, description: Option<String>) {
        self.required.push(RequiredVar {
            name,
            description,
            pattern: None,
            example: None,
        });
    }

    /// Saves the project configuration to a YAML file.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration cannot be serialized to YAML
    /// - The file cannot be written to disk (e.g., permission denied, disk full)
    pub fn save(&self, path: &Path) -> Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    /// Loads a project configuration from a YAML file.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read (e.g., file not found, permission denied)
    /// - The file content is not valid UTF-8
    /// - The YAML content cannot be parsed into a valid `ProjectConfig`
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

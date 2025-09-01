use clap::Args;
use color_eyre::Result;
use color_eyre::eyre::Context;
use color_eyre::eyre::eyre;
use envx_core::ProjectConfig;
use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct DocsArgs {
    /// Output file path (outputs to stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Custom title for the documentation
    #[arg(long, default_value = "Environment Variables")]
    pub title: String,

    /// Include only required variables
    #[arg(long)]
    pub required_only: bool,
}

/// Handles the documentation generation command.
///
/// # Errors
///
/// Returns an error if:
/// - The .envx/config.yaml file does not exist
/// - The project configuration cannot be loaded
/// - The output file cannot be written to
/// - Markdown generation fails
pub fn handle_docs(args: DocsArgs) -> Result<()> {
    // Check if .envx/config.yaml exists
    let config_path = Path::new(".envx").join("config.yaml");

    if !config_path.exists() {
        return Err(eyre!(
            "No .envx/config.yaml found in the current directory.\n\
            Please run 'envx project init' to initialize a project first."
        ));
    }

    // Load project configuration
    let config =
        ProjectConfig::load(&config_path).context("Failed to load project configuration from .envx/config.yaml")?;

    // Generate markdown documentation
    let markdown = generate_markdown(&config, &args).context("Failed to generate markdown documentation")?;

    // Output to file or stdout
    if let Some(output_path) = args.output {
        fs::write(&output_path, markdown)
            .with_context(|| format!("Failed to write documentation to '{}'", output_path.display()))?;
        println!("✅ Documentation generated: {}", output_path.display());
    } else {
        print!("{markdown}");
    }

    Ok(())
}

fn generate_markdown(config: &ProjectConfig, args: &DocsArgs) -> Result<String> {
    let mut output = String::new();

    // Title
    writeln!(&mut output, "# {}", args.title)?;
    writeln!(&mut output)?;

    // Collect all variables
    let mut all_vars: HashMap<String, (String, String, String, bool)> = HashMap::new();

    // 1. Add required variables from config
    for req_var in &config.required {
        all_vars.insert(
            req_var.name.clone(),
            (
                req_var
                    .description
                    .clone()
                    .unwrap_or_else(|| "_No description_".to_string()),
                req_var
                    .example
                    .clone()
                    .map_or_else(|| "_None_".to_string(), |e| mask_sensitive_value(&req_var.name, &e)),
                config
                    .defaults
                    .get(&req_var.name)
                    .map_or_else(|| "_None_".to_string(), |d| mask_sensitive_value(&req_var.name, d)),
                true, // is_required
            ),
        );
    }

    // 2. Add defaults from config (that aren't already in required)
    for (name, default_value) in &config.defaults {
        all_vars.entry(name.clone()).or_insert((
            "_No description_".to_string(),
            mask_sensitive_value(name, default_value),
            mask_sensitive_value(name, default_value),
            false, // is_required
        ));
    }

    // 3. Parse auto-loaded .env files to find more variables
    for file_path in &config.auto_load {
        if let Ok(env_vars) = parse_env_file(file_path) {
            for (name, value) in env_vars {
                // Only add if not already documented
                all_vars.entry(name.clone()).or_insert((
                    "_No description_".to_string(),
                    mask_sensitive_value(&name, &value),
                    "_None_".to_string(),
                    false, // is_required
                ));
            }
        }
    }

    // Convert to sorted vec for output
    let mut sorted_vars: Vec<(String, String, String, String, bool)> = all_vars
        .into_iter()
        .map(|(name, (desc, example, default, is_required))| (name, desc, example, default, is_required))
        .collect();

    // Filter if required_only
    if args.required_only {
        sorted_vars.retain(|(_, _, _, _, is_required)| *is_required);
    }

    // Sort by name
    sorted_vars.sort_by(|a, b| a.0.cmp(&b.0));

    // Generate table
    writeln!(&mut output, "| Variable | Description | Example | Default |")?;
    writeln!(&mut output, "|----------|-------------|---------|---------|")?;

    for (name, description, example, default, is_required) in sorted_vars {
        let var_name = if is_required { format!("**{name}**") } else { name };

        writeln!(
            &mut output,
            "| {var_name} | {description} | `{example}` | `{default}` |"
        )?;
    }

    Ok(output)
}

fn parse_env_file(path: &str) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    if !Path::new(path).exists() {
        return Ok(vars);
    }

    let content = fs::read_to_string(path)?;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE format
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            vars.insert(key.to_string(), value.to_string());
        }
    }

    Ok(vars)
}

fn mask_sensitive_value(name: &str, value: &str) -> String {
    let sensitive_patterns = [
        "KEY",
        "SECRET",
        "PASSWORD",
        "TOKEN",
        "PRIVATE",
        "CREDENTIAL",
        "AUTH",
        "CERT",
        "CERTIFICATE",
    ];

    let name_upper = name.to_uppercase();
    if sensitive_patterns.iter().any(|pattern| name_upper.contains(pattern)) {
        if value.len() > 4 {
            format!("{}****", &value[..4])
        } else {
            "****".to_string()
        }
    } else {
        value.to_string()
    }
}

// Add this at the end of the file

#[cfg(test)]
mod tests {
    use super::*;
    use envx_core::{ProjectConfig, RequiredVar, project_config::ValidationRules};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_config() -> ProjectConfig {
        ProjectConfig {
            name: Some("test-project".to_string()),
            description: Some("Test project description".to_string()),
            required: vec![
                RequiredVar {
                    name: "DATABASE_URL".to_string(),
                    description: Some("PostgreSQL connection string".to_string()),
                    example: Some("postgresql://user:pass@localhost:5432/dbname".to_string()),
                    pattern: None,
                },
                RequiredVar {
                    name: "API_KEY".to_string(),
                    description: Some("API key for external service".to_string()),
                    example: Some("sk-1234567890abcdef".to_string()),
                    pattern: None,
                },
                RequiredVar {
                    name: "JWT_SECRET".to_string(),
                    description: None,
                    example: None,
                    pattern: None,
                },
            ],
            defaults: HashMap::from([
                ("NODE_ENV".to_string(), "development".to_string()),
                ("PORT".to_string(), "3000".to_string()),
                ("API_KEY".to_string(), "default-api-key".to_string()),
                ("SECRET_TOKEN".to_string(), "secret123456".to_string()),
            ]),
            auto_load: vec![".env".to_string(), ".env.local".to_string()],
            profile: None,
            scripts: HashMap::new(),
            validation: ValidationRules::default(),
            inherit: true,
        }
    }

    #[test]
    fn test_mask_sensitive_value() {
        // Test sensitive patterns
        assert_eq!(mask_sensitive_value("API_KEY", "sk-1234567890"), "sk-1****");
        assert_eq!(mask_sensitive_value("SECRET", "mysecret"), "myse****");
        assert_eq!(mask_sensitive_value("PASSWORD", "pass123"), "pass****");
        assert_eq!(mask_sensitive_value("AUTH_TOKEN", "token"), "toke****");
        assert_eq!(mask_sensitive_value("PRIVATE_KEY", "key"), "****");
        assert_eq!(mask_sensitive_value("DB_PASSWORD", "dbpass"), "dbpa****");
        assert_eq!(mask_sensitive_value("CERTIFICATE", "cert123"), "cert****");

        // Test non-sensitive values
        assert_eq!(mask_sensitive_value("PORT", "3000"), "3000");
        assert_eq!(mask_sensitive_value("NODE_ENV", "production"), "production");
        assert_eq!(
            mask_sensitive_value("DATABASE_URL", "postgres://localhost"),
            "postgres://localhost"
        );

        // Test edge cases
        assert_eq!(mask_sensitive_value("KEY", ""), "****");
        assert_eq!(mask_sensitive_value("TOKEN", "abc"), "****");
        assert_eq!(mask_sensitive_value("MIXED_SECRET_VAR", "value"), "valu****"); // # spellchecker:disable-line
    }

    #[test]
    fn test_parse_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join(".env");

        // Create test .env file
        let content = r#"
# Comment line
DATABASE_URL=postgres://localhost:5432/mydb
API_KEY=test-api-key
PORT=3000

# Another comment
EMPTY_VALUE=
QUOTED_VALUE="quoted value"
SINGLE_QUOTED='single quoted'
SPACES_AROUND = value with spaces 
        "#;
        fs::write(&env_file, content).unwrap();

        let result = parse_env_file(env_file.to_str().unwrap()).unwrap();

        assert_eq!(
            result.get("DATABASE_URL"),
            Some(&"postgres://localhost:5432/mydb".to_string())
        );
        assert_eq!(result.get("API_KEY"), Some(&"test-api-key".to_string()));
        assert_eq!(result.get("PORT"), Some(&"3000".to_string()));
        assert_eq!(result.get("EMPTY_VALUE"), Some(&String::new()));
        assert_eq!(result.get("QUOTED_VALUE"), Some(&"quoted value".to_string()));
        assert_eq!(result.get("SINGLE_QUOTED"), Some(&"single quoted".to_string()));
        assert_eq!(result.get("SPACES_AROUND"), Some(&"value with spaces".to_string()));

        // Comments should not be parsed
        assert!(!result.contains_key("# Comment line"));
        assert!(!result.contains_key("# Another comment"));
    }

    #[test]
    fn test_parse_env_file_nonexistent() {
        let result = parse_env_file("nonexistent.env").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_env_file_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join(".env");

        // Test edge cases
        let content = r"
# Empty lines and various formats
KEY1=value1

KEY2 = value2
KEY3= value3
KEY4 =value4

# No equals sign
INVALID_LINE

# Multiple equals signs
KEY5=value=with=equals

# Unicode
UNICODE_KEY=值
KEY_UNICODE=hello世界

# Special characters
SPECIAL!@#$%^&*()=value
        ";
        fs::write(&env_file, content).unwrap();

        let result = parse_env_file(env_file.to_str().unwrap()).unwrap();

        assert_eq!(result.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(result.get("KEY2"), Some(&"value2".to_string()));
        assert_eq!(result.get("KEY3"), Some(&"value3".to_string()));
        assert_eq!(result.get("KEY4"), Some(&"value4".to_string()));
        assert_eq!(result.get("KEY5"), Some(&"value=with=equals".to_string()));
        assert_eq!(result.get("UNICODE_KEY"), Some(&"值".to_string()));
        assert_eq!(result.get("KEY_UNICODE"), Some(&"hello世界".to_string()));
        assert_eq!(result.get("SPECIAL!@#$%^&*()"), Some(&"value".to_string()));

        // Invalid line should not be parsed
        assert!(!result.contains_key("INVALID_LINE"));
    }

    #[test]
    fn test_generate_markdown_basic() {
        let config = create_test_config();
        let args = DocsArgs {
            output: None,
            title: "Test Environment Variables".to_string(),
            required_only: false,
        };

        let markdown = generate_markdown(&config, &args).unwrap();

        // Check title
        assert!(markdown.contains("# Test Environment Variables"));

        // Check table header
        assert!(markdown.contains("| Variable | Description | Example | Default |"));
        assert!(markdown.contains("|----------|-------------|---------|---------|"));

        // Check required variables are bold
        assert!(markdown.contains("| **DATABASE_URL** |"));
        assert!(markdown.contains("| **API_KEY** |"));
        assert!(markdown.contains("| **JWT_SECRET** |"));

        // Check descriptions
        assert!(markdown.contains("PostgreSQL connection string"));
        assert!(markdown.contains("API key for external service"));

        // Check examples
        assert!(markdown.contains("`postgresql://user:pass@localhost:5432/dbname`"));

        // Check sensitive values are masked
        assert!(markdown.contains("`sk-1****`")); // API_KEY example
        assert!(markdown.contains("`defa****`")); // API_KEY default
        assert!(markdown.contains("`secr****`")); // SECRET_TOKEN

        // Check non-sensitive defaults
        assert!(markdown.contains("| NODE_ENV |"));
        assert!(markdown.contains("`development`"));
        assert!(markdown.contains("| PORT |"));
        assert!(markdown.contains("`3000`"));
    }

    #[test]
    fn test_generate_markdown_required_only() {
        let config = create_test_config();
        let args = DocsArgs {
            output: None,
            title: "Environment Variables".to_string(),
            required_only: true,
        };

        let markdown = generate_markdown(&config, &args).unwrap();

        // Should contain required variables
        assert!(markdown.contains("**DATABASE_URL**"));
        assert!(markdown.contains("**API_KEY**"));
        assert!(markdown.contains("**JWT_SECRET**"));

        // Should NOT contain optional variables
        assert!(!markdown.contains("| NODE_ENV |"));
        assert!(!markdown.contains("| PORT |"));
        assert!(!markdown.contains("| SECRET_TOKEN |"));
    }

    #[test]
    fn test_generate_markdown_with_env_files() {
        let temp_dir = TempDir::new().unwrap();
        let env_file = temp_dir.path().join(".env");

        // Create .env file with additional variables
        let content = r"
REDIS_URL=redis://localhost:6379
CACHE_PASSWORD=cachepass123
LOG_LEVEL=debug
NEW_VAR=new_value
        ";
        fs::write(&env_file, content).unwrap();

        // Create config with auto_load pointing to our test file
        let mut config = create_test_config();
        config.auto_load = vec![env_file.to_str().unwrap().to_string()];

        let args = DocsArgs {
            output: None,
            title: "Environment Variables".to_string(),
            required_only: false,
        };

        let markdown = generate_markdown(&config, &args).unwrap();

        // Should include variables from .env file
        assert!(markdown.contains("| REDIS_URL |"));
        assert!(markdown.contains("`redis://localhost:6379`"));

        // Password should be masked
        assert!(markdown.contains("| CACHE_PASSWORD |"));
        assert!(markdown.contains("`cach****`")); // # spellchecker:disable-line

        // Regular variables should not be masked
        assert!(markdown.contains("| LOG_LEVEL |"));
        assert!(markdown.contains("`debug`"));
        assert!(markdown.contains("| NEW_VAR |"));
        assert!(markdown.contains("`new_value`"));
    }

    #[test]
    fn test_generate_markdown_sorting() {
        let config = ProjectConfig {
            name: None,
            description: None,
            required: vec![
                RequiredVar {
                    name: "ZEBRA".to_string(),
                    description: None,
                    example: None,
                    pattern: None,
                },
                RequiredVar {
                    name: "APPLE".to_string(),
                    description: None,
                    example: None,
                    pattern: None,
                },
            ],
            defaults: HashMap::from([
                ("BANANA".to_string(), "yellow".to_string()),
                ("MANGO".to_string(), "orange".to_string()),
            ]),
            auto_load: vec![],
            profile: None,
            scripts: HashMap::new(),
            validation: ValidationRules::default(),
            inherit: true,
        };

        let args = DocsArgs {
            output: None,
            title: "Test".to_string(),
            required_only: false,
        };

        let markdown = generate_markdown(&config, &args).unwrap();

        // Extract variable names from markdown to check order
        let lines: Vec<&str> = markdown.lines().collect();
        let var_lines: Vec<&str> = lines
            .iter()
            .filter(|line| line.starts_with("| ") && !line.contains("Variable") && !line.contains("----"))
            .copied()
            .collect();

        // Variables should be in alphabetical order
        assert!(var_lines[0].contains("APPLE"));
        assert!(var_lines[1].contains("BANANA"));
        assert!(var_lines[2].contains("MANGO"));
        assert!(var_lines[3].contains("ZEBRA"));
    }

    fn handle_docs_with_config(args: DocsArgs, config: &ProjectConfig) -> Result<()> {
        // Generate markdown documentation
        let markdown = generate_markdown(config, &args)?;

        // Output to file or stdout
        if let Some(output_path) = args.output {
            fs::write(&output_path, markdown)?;
            println!("✅ Documentation generated: {}", output_path.display());
        } else {
            print!("{markdown}");
        }

        Ok(())
    }

    #[test]
    fn test_handle_docs_stdout() {
        let config = create_test_config();

        let args = DocsArgs {
            output: None,
            title: "Test".to_string(),
            required_only: false,
        };

        // Use the test helper function that doesn't load from disk
        let result = handle_docs_with_config(args, &config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_docs_file_output() {
        let temp_dir = TempDir::new().unwrap();
        let output_file = temp_dir.path().join("output.md");

        let config = create_test_config();

        let args = DocsArgs {
            output: Some(output_file.clone()),
            title: "Test Output".to_string(),
            required_only: false,
        };

        let result = handle_docs_with_config(args, &config);

        assert!(result.is_ok());
        assert!(output_file.exists());

        let content = fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("# Test Output"));
        assert!(content.contains("**API_KEY**"));
        assert!(content.contains("PORT"));
    }

    #[test]
    fn test_markdown_content_structure() {
        let config = create_test_config();
        let args = DocsArgs {
            output: None,
            title: "My Variables".to_string(),
            required_only: false,
        };

        let markdown = generate_markdown(&config, &args).unwrap();
        let lines: Vec<&str> = markdown.lines().collect();

        // Check structure
        assert_eq!(lines[0], "# My Variables");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "| Variable | Description | Example | Default |");
        assert_eq!(lines[3], "|----------|-------------|---------|---------|");

        // Count table rows (excluding header and separator)
        let table_rows = lines.iter().skip(4).filter(|line| line.starts_with('|')).count();

        // Should have rows for all required vars + defaults
        assert!(table_rows >= 4); // At least API_KEY, DATABASE_URL, JWT_SECRET, NODE_ENV, PORT, SECRET_TOKEN
    }
}

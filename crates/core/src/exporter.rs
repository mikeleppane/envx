use crate::EnvVar;
use color_eyre::Result;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    DotEnv,
    Json,
    Yaml,
    Text,
    PowerShell,
    Shell,
}

impl ExportFormat {
    /// Determines the export format from a file path's extension.
    ///
    /// # Errors
    ///
    /// Currently this function never returns an error and always succeeds,
    /// defaulting to `Text` format for unknown extensions.
    pub fn from_extension(path: &str) -> Result<Self> {
        let ext = Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("");

        match ext.to_lowercase().as_str() {
            "env" => Ok(Self::DotEnv),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            "txt" | "text" => Ok(Self::Text),
            "ps1" => Ok(Self::PowerShell),
            "sh" | "bash" => Ok(Self::Shell),
            _ => {
                // Check if filename is .env or similar
                let filename = Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or("");

                if filename.starts_with('.') && filename.contains("env") {
                    Ok(Self::DotEnv)
                } else {
                    Ok(Self::Text) // Default to text format
                }
            }
        }
    }
}

pub struct Exporter {
    variables: Vec<EnvVar>,
    include_metadata: bool,
}

impl Exporter {
    #[must_use]
    pub const fn new(variables: Vec<EnvVar>, include_metadata: bool) -> Self {
        Self {
            variables,
            include_metadata,
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.variables.len()
    }

    /// Exports environment variables to a file in the specified format.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be created or written to due to filesystem permissions or disk space issues
    /// - JSON serialization fails when using JSON format
    /// - YAML formatting fails when using YAML format
    pub fn export_to_file(&self, path: &str, format: ExportFormat) -> Result<()> {
        let content = match format {
            ExportFormat::DotEnv => self.to_dotenv(),
            ExportFormat::Json => self.to_json()?,
            ExportFormat::Yaml => self.to_yaml(),
            ExportFormat::Text => self.to_text(),
            ExportFormat::PowerShell => self.to_powershell(),
            ExportFormat::Shell => self.to_shell(),
        };

        fs::write(path, content)?;
        Ok(())
    }

    fn to_dotenv(&self) -> String {
        let mut lines = Vec::new();

        if self.include_metadata {
            lines.push("# Environment variables exported by envx".to_string());
            lines.push(format!(
                "# Date: {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            ));
            lines.push(format!("# Count: {}", self.variables.len()));
            lines.push(String::new());
        }

        for var in &self.variables {
            if self.include_metadata {
                lines.push(format!(
                    "# Source: {:?}, Modified: {}",
                    var.source,
                    var.modified.format("%Y-%m-%d %H:%M:%S")
                ));
            }

            // For .env format, we need to handle escaping more carefully
            // Only escape actual escape sequences, not all backslashes
            let needs_quotes = var.value.contains(' ')
                || var.value.contains('=')
                || var.value.contains('#')
                || var.value.contains('"')
                || var.value.contains('\'')
                || var.value.contains('\n')
                || var.value.contains('\r')
                || var.value.contains('\t');

            if needs_quotes {
                // In quoted strings, only escape quotes and actual escape sequences
                let escaped_value = var
                    .value
                    .replace('"', "\\\"") // Escape quotes
                    .replace('\n', "\\n") // Escape newlines
                    .replace('\r', "\\r") // Escape carriage returns
                    .replace('\t', "\\t"); // Escape tabs
                // Don't escape backslashes in paths!

                lines.push(format!("{}=\"{}\"", var.name, escaped_value));
            } else {
                // For unquoted values, we might need different escaping
                // But for simple values, just use as-is
                lines.push(format!("{}={}", var.name, var.value));
            }
        }

        lines.join("\n")
    }

    fn to_json(&self) -> Result<String> {
        if self.include_metadata {
            // Export with full metadata
            let export_data = serde_json::json!({
                "exported_at": chrono::Utc::now(),
                "count": self.variables.len(),
                "variables": self.variables
            });
            Ok(serde_json::to_string_pretty(&export_data)?)
        } else {
            // Export as simple key-value pairs
            let mut map = serde_json::Map::new();
            for var in &self.variables {
                map.insert(var.name.clone(), serde_json::Value::String(var.value.clone()));
            }
            Ok(serde_json::to_string_pretty(&map)?)
        }
    }

    fn to_yaml(&self) -> String {
        let mut lines = Vec::new();

        if self.include_metadata {
            lines.push("# Environment variables exported by envx".to_string());
            lines.push(format!(
                "# Date: {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            ));
            lines.push("---".to_string());
        }

        for var in &self.variables {
            if self.include_metadata {
                lines.push(format!("# Source: {:?}", var.source));
            }

            // For YAML, we need to quote values that contain special YAML characters
            // but we should NOT escape backslashes in paths
            let value = if var.value.contains(':')
                || var.value.contains('#')
                || var.value.contains('"')
                || var.value.contains('\'')
                || var.value.contains('\n')
                || var.value.contains('\r')
                || var.value.contains('\t')
                || var.value.starts_with(' ')
                || var.value.ends_with(' ')
                || var.value.starts_with('-')
                || var.value.starts_with('*')
                || var.value.starts_with('&')
                || var.value.starts_with('!')
                || var.value.starts_with('[')
                || var.value.starts_with('{')
                || var.value.starts_with('>')
                || var.value.starts_with('|')
            {
                // In YAML quoted strings, only escape quotes and control characters
                let escaped = var
                    .value
                    .replace('"', "\\\"") // Escape quotes
                    .replace('\n', "\\n") // Escape newlines
                    .replace('\r', "\\r") // Escape carriage returns
                    .replace('\t', "\\t"); // Escape tabs
                // Don't escape backslashes!

                format!("\"{escaped}\"")
            } else {
                var.value.clone()
            };

            lines.push(format!("{}: {}", var.name, value));
        }

        lines.join("\n")
    }

    fn to_text(&self) -> String {
        let mut lines = Vec::new();

        if self.include_metadata {
            lines.push("# Environment Variables Export".to_string());
            lines.push(format!("# Generated: {}", chrono::Utc::now()));
            lines.push(format!("# Total: {} variables", self.variables.len()));
            lines.push("#".repeat(50));
            lines.push(String::new());
        }

        for var in &self.variables {
            if self.include_metadata {
                lines.push(format!("# Name: {}", var.name));
                lines.push(format!("# Source: {:?}", var.source));
                lines.push(format!("# Modified: {}", var.modified));
            }
            lines.push(format!("{}={}", var.name, var.value));
            if self.include_metadata {
                lines.push(String::new());
            }
        }

        lines.join("\n")
    }

    fn to_powershell(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# PowerShell Environment Variables Script".to_string());
        lines.push(format!("# Generated by envx - {}", chrono::Utc::now()));
        lines.push(String::new());

        for var in &self.variables {
            if self.include_metadata {
                lines.push(format!("# {} ({:?})", var.name, var.source));
            }

            // Escape PowerShell special characters
            let escaped_value = var.value.replace('`', "``").replace('"', "`\"");
            lines.push(format!("$env:{} = \"{}\"", var.name, escaped_value));
        }

        lines.join("\n")
    }

    fn to_shell(&self) -> String {
        let mut lines = Vec::new();

        lines.push("#!/bin/bash".to_string());
        lines.push("# Shell Environment Variables Script".to_string());
        lines.push(format!("# Generated by envx - {}", chrono::Utc::now()));
        lines.push(String::new());

        for var in &self.variables {
            if self.include_metadata {
                lines.push(format!("# {} ({:?})", var.name, var.source));
            }

            // Escape shell special characters
            let escaped_value = var
                .value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
                .replace('`', "\\`");

            lines.push(format!("export {}=\"{}\"", var.name, escaped_value));
        }

        lines.join("\n")
    }
}

// ...existing code...

#[cfg(test)]
mod tests {
    #![allow(clippy::cognitive_complexity)]
    use super::*;
    use crate::EnvVar;
    use crate::EnvVarSource as VarSource;
    use chrono::{DateTime, Utc};
    use std::fs;
    use tempfile::NamedTempFile;

    // Helper function to create test environment variables
    fn create_test_vars() -> Vec<EnvVar> {
        vec![
            EnvVar {
                name: "SIMPLE_VAR".to_string(),
                value: "simple_value".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "PATH_VAR".to_string(),
                value: "C:\\Program Files\\App;C:\\Windows\\System32".to_string(),
                source: VarSource::System,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "QUOTED_VAR".to_string(),
                value: "value with \"quotes\" and 'single quotes'".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "SPECIAL_CHARS".to_string(),
                value: "line1\nline2\ttab\\backslash".to_string(),
                source: VarSource::Process,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "EMPTY_VAR".to_string(),
                value: String::new(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "UNICODE_VAR".to_string(),
                value: "Hello 世界 🌍".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ]
    }

    #[test]
    fn test_export_format_from_extension() {
        assert!(matches!(
            ExportFormat::from_extension("file.env").unwrap(),
            ExportFormat::DotEnv
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.ENV").unwrap(),
            ExportFormat::DotEnv
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.json").unwrap(),
            ExportFormat::Json
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.JSON").unwrap(),
            ExportFormat::Json
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.yaml").unwrap(),
            ExportFormat::Yaml
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.yml").unwrap(),
            ExportFormat::Yaml
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.txt").unwrap(),
            ExportFormat::Text
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.text").unwrap(),
            ExportFormat::Text
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.ps1").unwrap(),
            ExportFormat::PowerShell
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.sh").unwrap(),
            ExportFormat::Shell
        ));
        assert!(matches!(
            ExportFormat::from_extension("file.bash").unwrap(),
            ExportFormat::Shell
        ));

        // Special case for .env files
        assert!(matches!(
            ExportFormat::from_extension(".env").unwrap(),
            ExportFormat::DotEnv
        ));
        assert!(matches!(
            ExportFormat::from_extension(".env.local").unwrap(),
            ExportFormat::DotEnv
        ));
        assert!(matches!(
            ExportFormat::from_extension(".env.production").unwrap(),
            ExportFormat::DotEnv
        ));

        // Default to Text for unknown extensions
        assert!(matches!(
            ExportFormat::from_extension("file.xyz").unwrap(),
            ExportFormat::Text
        ));
        assert!(matches!(
            ExportFormat::from_extension("file").unwrap(),
            ExportFormat::Text
        ));
    }

    #[test]
    fn test_exporter_new() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars.clone(), true);

        assert_eq!(exporter.count(), vars.len());
    }

    #[test]
    fn test_to_dotenv_without_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        let output = exporter.to_dotenv();

        // Verify basic variables
        assert!(output.contains("SIMPLE_VAR=simple_value"));

        // Verify quoted values (contains spaces or special chars)
        // Windows paths should NOT have escaped backslashes in quoted strings
        assert!(output.contains("PATH_VAR=\"C:\\Program Files\\App;C:\\Windows\\System32\""));
        assert!(output.contains("QUOTED_VAR=\"value with \\\"quotes\\\" and 'single quotes'\""));

        // Verify escaped characters - only actual control characters are escaped
        assert!(output.contains("SPECIAL_CHARS=\"line1\\nline2\\ttab\\backslash\""));

        // Verify empty value
        assert!(output.contains("EMPTY_VAR="));

        // Verify no metadata comments
        assert!(!output.contains("# Environment variables exported by envx"));
        assert!(!output.contains("# Source:"));
    }

    #[test]
    fn test_to_dotenv_with_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, true);

        let output = exporter.to_dotenv();

        // Verify metadata is included
        assert!(output.contains("# Environment variables exported by envx"));
        assert!(output.contains("# Date:"));
        assert!(output.contains("# Count: 6"));
        assert!(output.contains("# Source:"));
        assert!(output.contains("Modified:"));
    }

    #[test]
    fn test_to_dotenv_edge_cases() {
        let vars = vec![
            EnvVar {
                name: "HASH_VALUE".to_string(),
                value: "value#with#hashes".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "EQUALS_VALUE".to_string(),
                value: "key=value=pairs".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "SPACES_AROUND".to_string(),
                value: "  spaces at start and end  ".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ];

        let exporter = Exporter::new(vars, false);
        let output = exporter.to_dotenv();

        // Values with # or = should be quoted
        assert!(output.contains("HASH_VALUE=\"value#with#hashes\""));
        assert!(output.contains("EQUALS_VALUE=\"key=value=pairs\""));
        assert!(output.contains("SPACES_AROUND=\"  spaces at start and end  \""));
    }

    #[test]
    fn test_to_json_without_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        let output = exporter.to_json().unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Should be a simple object with key-value pairs
        assert!(json.is_object());
        assert_eq!(json["SIMPLE_VAR"], "simple_value");
        assert_eq!(json["PATH_VAR"], "C:\\Program Files\\App;C:\\Windows\\System32");
        assert_eq!(json["QUOTED_VAR"], "value with \"quotes\" and 'single quotes'");
        assert_eq!(json["SPECIAL_CHARS"], "line1\nline2\ttab\\backslash");
        assert_eq!(json["EMPTY_VAR"], "");
        assert_eq!(json["UNICODE_VAR"], "Hello 世界 🌍");
    }

    #[test]
    fn test_to_json_with_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, true);

        let output = exporter.to_json().unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Should have metadata structure
        assert!(json.is_object());
        assert!(json["exported_at"].is_string());
        assert_eq!(json["count"], 6);
        assert!(json["variables"].is_array());

        let variables = json["variables"].as_array().unwrap();
        assert_eq!(variables.len(), 6);

        // Check first variable has all fields
        let first_var = &variables[0];
        assert!(first_var["name"].is_string());
        assert!(first_var["value"].is_string());
        assert!(first_var["source"].is_string());
        assert!(first_var["modified"].is_string());
    }

    #[test]
    fn test_to_yaml_without_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        let output = exporter.to_yaml();

        // Verify basic YAML format
        assert!(output.contains("SIMPLE_VAR: simple_value"));
        assert!(output.contains("EMPTY_VAR: "));

        // Values with colons should be quoted
        assert!(output.contains("PATH_VAR: \"C:\\Program Files\\App;C:\\Windows\\System32\""));

        // Values with quotes should be escaped and quoted
        assert!(output.contains("QUOTED_VAR: \"value with \\\"quotes\\\" and 'single quotes'\""));

        // No metadata
        assert!(!output.contains("# Environment variables exported by envx"));
    }

    #[test]
    fn test_to_yaml_with_metadata() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, true);

        let output = exporter.to_yaml();

        // Verify metadata
        assert!(output.contains("# Environment variables exported by envx"));
        assert!(output.contains("# Date:"));
        assert!(output.contains("---"));
        assert!(output.contains("# Source:"));
    }

    #[test]
    fn test_to_yaml_special_cases() {
        let vars = vec![
            EnvVar {
                name: "URL".to_string(),
                value: "https://example.com:8080/path".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "COMMENT".to_string(),
                value: "value # with comment".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "LEADING_SPACE".to_string(),
                value: "  value".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "TRAILING_SPACE".to_string(),
                value: "value  ".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ];

        let exporter = Exporter::new(vars, false);
        let output = exporter.to_yaml();

        // All these should be quoted due to special characters
        assert!(output.contains("URL: \"https://example.com:8080/path\""));
        assert!(output.contains("COMMENT: \"value # with comment\""));
        assert!(output.contains("LEADING_SPACE: \"  value\""));
        assert!(output.contains("TRAILING_SPACE: \"value  \""));
    }

    #[test]
    fn test_to_text() {
        let vars = create_test_vars();

        // Without metadata
        let exporter = Exporter::new(vars.clone(), false);
        let output = exporter.to_text();

        assert!(output.contains("SIMPLE_VAR=simple_value"));
        assert!(output.contains("PATH_VAR=C:\\Program Files\\App;C:\\Windows\\System32"));
        assert!(!output.contains("# Environment Variables Export"));

        // With metadata
        let exporter = Exporter::new(vars, true);
        let output = exporter.to_text();

        assert!(output.contains("# Environment Variables Export"));
        assert!(output.contains("# Generated:"));
        assert!(output.contains("# Total: 6 variables"));
        assert!(output.contains("# Name: SIMPLE_VAR"));
        assert!(output.contains("# Source:"));
        assert!(output.contains("# Modified:"));
    }

    #[test]
    fn test_to_powershell() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        let output = exporter.to_powershell();

        // Verify PowerShell header
        assert!(output.contains("# PowerShell Environment Variables Script"));
        assert!(output.contains("# Generated by envx"));

        // Verify PowerShell format
        assert!(output.contains("$env:SIMPLE_VAR = \"simple_value\""));
        assert!(output.contains("$env:PATH_VAR = \"C:\\Program Files\\App;C:\\Windows\\System32\""));

        // Verify escaped characters
        assert!(output.contains("$env:QUOTED_VAR = \"value with `\"quotes`\" and 'single quotes'\""));
        assert!(output.contains("$env:SPECIAL_CHARS = \"line1\nline2\ttab\\backslash\""));
    }

    #[test]
    fn test_to_powershell_escaping() {
        let vars = vec![
            EnvVar {
                name: "BACKTICK".to_string(),
                value: "value`with`backticks".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "DOLLAR".to_string(),
                value: "$variable $test".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ];

        let exporter = Exporter::new(vars, false);
        let output = exporter.to_powershell();

        // Backticks should be escaped
        assert!(output.contains("$env:BACKTICK = \"value``with``backticks\""));
        // Dollar signs are not escaped in PowerShell strings with double quotes
        assert!(output.contains("$env:DOLLAR = \"$variable $test\""));
    }

    #[test]
    fn test_to_shell() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        let output = exporter.to_shell();

        // Verify shell header
        assert!(output.contains("#!/bin/bash"));
        assert!(output.contains("# Shell Environment Variables Script"));
        assert!(output.contains("# Generated by envx"));

        // Verify shell format
        assert!(output.contains("export SIMPLE_VAR=\"simple_value\""));
        assert!(output.contains("export PATH_VAR=\"C:\\\\Program Files\\\\App;C:\\\\Windows\\\\System32\""));

        // Verify escaped characters
        assert!(output.contains("export QUOTED_VAR=\"value with \\\"quotes\\\" and 'single quotes'\""));
        assert!(output.contains("export SPECIAL_CHARS=\"line1\nline2\ttab\\\\backslash\""));
    }

    #[test]
    fn test_to_shell_escaping() {
        let vars = vec![
            EnvVar {
                name: "DOLLAR".to_string(),
                value: "$HOME/path".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "BACKTICK".to_string(),
                value: "`command`".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "BACKSLASH".to_string(),
                value: "path\\to\\file".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ];

        let exporter = Exporter::new(vars, false);
        let output = exporter.to_shell();

        // Shell special characters should be escaped
        assert!(output.contains("export DOLLAR=\"\\$HOME/path\""));
        assert!(output.contains("export BACKTICK=\"\\`command\\`\""));
        assert!(output.contains("export BACKSLASH=\"path\\\\to\\\\file\""));
    }

    #[test]
    fn test_export_to_file() {
        let vars = create_test_vars();
        let exporter = Exporter::new(vars, false);

        // Test exporting to different formats
        let formats = vec![
            (ExportFormat::DotEnv, ".env"),
            (ExportFormat::Json, ".json"),
            (ExportFormat::Yaml, ".yaml"),
            (ExportFormat::Text, ".txt"),
            (ExportFormat::PowerShell, ".ps1"),
            (ExportFormat::Shell, ".sh"),
        ];

        for (format, ext) in formats {
            let temp_file = NamedTempFile::with_suffix(ext).unwrap();
            let path = temp_file.path().to_str().unwrap();

            exporter.export_to_file(path, format).unwrap();

            // Verify file was created and has content
            let content = fs::read_to_string(path).unwrap();
            assert!(!content.is_empty());
            assert!(content.contains("SIMPLE_VAR"));
        }
    }

    #[test]
    fn test_empty_export() {
        let exporter = Exporter::new(vec![], true);

        assert_eq!(exporter.count(), 0);

        // Test all formats with empty variables
        let dotenv = exporter.to_dotenv();
        assert!(dotenv.contains("# Count: 0"));

        let json = exporter.to_json().unwrap();
        assert!(json.contains("\"count\": 0"));
        assert!(json.contains("\"variables\": []"));

        let yaml = exporter.to_yaml();
        assert!(yaml.contains("---"));

        let text = exporter.to_text();
        assert!(text.contains("# Total: 0 variables"));

        let ps = exporter.to_powershell();
        assert!(ps.contains("# PowerShell Environment Variables Script"));

        let sh = exporter.to_shell();
        assert!(sh.contains("#!/bin/bash"));
    }

    #[test]
    fn test_variable_name_edge_cases() {
        let vars = vec![
            EnvVar {
                name: "SIMPLE-NAME-WITH-DASHES".to_string(),
                value: "value1".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "NAME.WITH.DOTS".to_string(),
                value: "value2".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "_UNDERSCORE_START".to_string(),
                value: "value3".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
            EnvVar {
                name: "123_NUMBER_START".to_string(),
                value: "value4".to_string(),
                source: VarSource::User,
                modified: Utc::now(),
                original_value: None,
            },
        ];

        let exporter = Exporter::new(vars, false);

        // All formats should handle these names correctly
        let dotenv = exporter.to_dotenv();
        assert!(dotenv.contains("SIMPLE-NAME-WITH-DASHES=value1"));
        assert!(dotenv.contains("NAME.WITH.DOTS=value2"));

        let json = exporter.to_json().unwrap();
        assert!(json.contains("\"SIMPLE-NAME-WITH-DASHES\": \"value1\""));

        let yaml = exporter.to_yaml();
        assert!(yaml.contains("SIMPLE-NAME-WITH-DASHES: value1"));

        let ps = exporter.to_powershell();
        assert!(ps.contains("$env:SIMPLE-NAME-WITH-DASHES = \"value1\""));

        let sh = exporter.to_shell();
        assert!(sh.contains("export SIMPLE-NAME-WITH-DASHES=\"value1\""));
    }

    #[test]
    fn test_very_long_values() {
        let long_value = "a".repeat(1000);
        let vars = vec![EnvVar {
            name: "LONG_VALUE".to_string(),
            value: long_value.clone(),
            source: VarSource::User,
            modified: Utc::now(),
            original_value: None,
        }];

        let exporter = Exporter::new(vars, false);

        // All formats should handle long values
        let dotenv = exporter.to_dotenv();
        assert!(dotenv.contains(&format!("LONG_VALUE={long_value}")));

        let json = exporter.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["LONG_VALUE"].as_str().unwrap().len(), 1000);
    }

    #[test]
    fn test_metadata_consistency() {
        let fixed_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let vars = vec![EnvVar {
            name: "TEST_VAR".to_string(),
            value: "test_value".to_string(),
            source: VarSource::System,
            modified: fixed_time,
            original_value: None,
        }];

        let exporter = Exporter::new(vars, true);

        // Check that metadata is formatted consistently
        let dotenv = exporter.to_dotenv();
        assert!(dotenv.contains("# Source: System"));
        assert!(dotenv.contains("2024-01-01 12:00:00"));

        let text = exporter.to_text();
        assert!(text.contains("# Source: System"));

        let ps = exporter.to_powershell();
        assert!(ps.contains("# TEST_VAR (System)"));

        let sh = exporter.to_shell();
        assert!(sh.contains("# TEST_VAR (System)"));
    }
}

use ahash::AHashMap as HashMap;
use color_eyre::Result;
use regex::Regex;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum ImportFormat {
    DotEnv,
    Json,
    Yaml,
    Text,
}

impl ImportFormat {
    /// Determines the import format based on file extension.
    ///
    /// # Errors
    ///
    /// This function currently never returns an error, but uses `Result` for future extensibility.
    pub fn from_extension(path: &str) -> Result<Self> {
        let ext = Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("");

        match ext.to_lowercase().as_str() {
            "env" => Ok(Self::DotEnv),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            "txt" | "text" => Ok(Self::Text),
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

#[derive(Debug, Clone, Default)]
pub struct Importer {
    variables: HashMap<String, String>,
}

impl Importer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Imports environment variables from a file in the specified format.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read (file not found, permission denied, etc.)
    /// - The file content cannot be parsed in the specified format (e.g., invalid JSON syntax)
    pub fn import_from_file(&mut self, path: &str, format: ImportFormat) -> Result<()> {
        let content = fs::read_to_string(path)?;

        match format {
            ImportFormat::DotEnv => self.parse_dotenv(&content),
            ImportFormat::Json => self.parse_json(&content)?,
            ImportFormat::Yaml => self.parse_yaml(&content),
            ImportFormat::Text => self.parse_text(&content),
        }

        Ok(())
    }

    #[must_use]
    pub fn get_variables(&self) -> Vec<(String, String)> {
        self.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    pub fn filter_by_patterns(&mut self, patterns: &[String]) {
        let mut matched = HashMap::new();

        for pattern in patterns {
            let regex_pattern = if pattern.contains('*') || pattern.contains('?') {
                wildcard_to_regex(pattern)
            } else {
                format!("^{}$", regex::escape(pattern))
            };

            if let Ok(re) = Regex::new(&regex_pattern) {
                for (key, value) in &self.variables {
                    if re.is_match(key) {
                        matched.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        self.variables = matched;
    }

    pub fn add_prefix(&mut self, prefix: &str) {
        let mut prefixed = HashMap::new();

        for (key, value) in self.variables.drain() {
            prefixed.insert(format!("{prefix}{key}"), value);
        }

        self.variables = prefixed;
    }

    fn parse_dotenv(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=VALUE or KEY="VALUE"
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();

                // Validate key
                if key.is_empty() || key.contains(' ') {
                    continue;
                }

                // Process value
                let processed_value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    // Remove quotes and unescape
                    let unquoted = &value[1..value.len() - 1];

                    // Process escape sequences properly
                    Self::unescape_string(unquoted)
                } else {
                    // Remove inline comments (but not # in values)
                    if let Some(comment_pos) = value.find(" #") {
                        value[..comment_pos].trim().to_string()
                    } else {
                        value.to_string()
                    }
                };

                self.variables.insert(key.to_string(), processed_value);
            }
        }
    }

    fn unescape_string(input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.peek() {
                    Some('\\') => {
                        result.push('\\');
                        chars.next(); // consume the second backslash
                    }
                    Some('n') => {
                        result.push('\n');
                        chars.next(); // consume the 'n'
                    }
                    Some('r') => {
                        result.push('\r');
                        chars.next(); // consume the 'r'
                    }
                    Some('t') => {
                        result.push('\t');
                        chars.next(); // consume the 't'
                    }
                    Some('"') => {
                        result.push('"');
                        chars.next(); // consume the quote
                    }
                    Some('\'') => {
                        result.push('\'');
                        chars.next(); // consume the single quote
                    }
                    _ => {
                        // Unknown escape sequence, keep the backslash
                        result.push('\\');
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn parse_json(&mut self, content: &str) -> Result<()> {
        let parsed: serde_json::Value = serde_json::from_str(content)?;

        // Handle both simple object and structured format
        if let Some(obj) = parsed.as_object() {
            // Check if it's a structured export with metadata
            if obj.contains_key("variables") {
                if let Some(vars) = obj["variables"].as_array() {
                    for var in vars {
                        if let (Some(name), Some(value)) = (
                            var.get("name").and_then(|v| v.as_str()),
                            var.get("value").and_then(|v| v.as_str()),
                        ) {
                            self.variables.insert(name.to_string(), value.to_string());
                        }
                    }
                }
            } else {
                // Simple key-value format
                for (key, value) in obj {
                    if let Some(val_str) = value.as_str() {
                        self.variables.insert(key.clone(), val_str.to_string());
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_yaml(&mut self, content: &str) {
        // Simple YAML parser for key: value pairs
        let mut skip_remaining = false;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Stop processing after document separator
            if line == "---" {
                skip_remaining = true;
                continue;
            }

            // Skip all content after document separator
            if skip_remaining {
                continue;
            }

            // Parse key: value
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();

                // Remove quotes if present
                let processed_value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    value[1..value.len() - 1].to_string()
                } else {
                    value.to_string()
                };

                self.variables.insert(key.to_string(), processed_value);
            }
        }
    }

    fn parse_text(&mut self, content: &str) {
        // Same as dotenv but more lenient
        self.parse_dotenv(content);
    }
}

fn wildcard_to_regex(pattern: &str) -> String {
    let mut regex = String::new();
    regex.push('^');

    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }

    regex.push('$');
    regex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper function to create a temporary file with content
    fn create_temp_file(content: &str, extension: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        if !extension.is_empty() {
            file = NamedTempFile::with_suffix(extension).unwrap();
        }
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_import_format_from_extension() {
        assert!(matches!(
            ImportFormat::from_extension("file.env").unwrap(),
            ImportFormat::DotEnv
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.ENV").unwrap(),
            ImportFormat::DotEnv
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.json").unwrap(),
            ImportFormat::Json
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.JSON").unwrap(),
            ImportFormat::Json
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.yaml").unwrap(),
            ImportFormat::Yaml
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.yml").unwrap(),
            ImportFormat::Yaml
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.txt").unwrap(),
            ImportFormat::Text
        ));
        assert!(matches!(
            ImportFormat::from_extension("file.text").unwrap(),
            ImportFormat::Text
        ));

        // Default to Text for unknown extensions
        assert!(matches!(
            ImportFormat::from_extension("file.xyz").unwrap(),
            ImportFormat::Text
        ));
        assert!(matches!(
            ImportFormat::from_extension("file").unwrap(),
            ImportFormat::Text
        ));

        // Special case for .env files
        assert!(matches!(
            ImportFormat::from_extension(".env").unwrap(),
            ImportFormat::DotEnv
        ));
        assert!(matches!(
            ImportFormat::from_extension(".env.local").unwrap(),
            ImportFormat::DotEnv
        ));
        assert!(matches!(
            ImportFormat::from_extension(".env.production").unwrap(),
            ImportFormat::DotEnv
        ));
    }

    #[test]
    fn test_parse_dotenv_basic() {
        let mut importer = Importer::new();
        let content = r#"
# This is a comment
KEY1=value1
KEY2=value2
KEY3=value with spaces

# Another comment
KEY4="quoted value"
KEY5='single quoted'
"#;

        importer.parse_dotenv(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("KEY2").unwrap(), "value2");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value with spaces");
        assert_eq!(vars_map.get("KEY4").unwrap(), "quoted value");
        assert_eq!(vars_map.get("KEY5").unwrap(), "single quoted");
    }

    #[test]
    fn test_parse_dotenv_with_escapes() {
        let mut importer = Importer::new();
        let content = r#"
ESCAPED="line1\nline2\ttab"
BACKSLASH="path\\to\\file"
DOUBLE_BACKSLASH="path\\\\to\\\\file"
QUOTE="He said \"hello\""
"#;

        importer.parse_dotenv(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("ESCAPED").unwrap(), "line1\nline2\ttab");
        assert_eq!(vars_map.get("BACKSLASH").unwrap(), "path\\to\\file");
        assert_eq!(vars_map.get("DOUBLE_BACKSLASH").unwrap(), "path\\\\to\\\\file");
        assert_eq!(vars_map.get("QUOTE").unwrap(), "He said \"hello\"");
    }

    #[test]
    fn test_parse_dotenv_inline_comments() {
        let mut importer = Importer::new();
        let content = r#"
KEY1=value1 # This is an inline comment
KEY2=value#notacomment
KEY3=value # comment
KEY4="value # not a comment in quotes"
"#;

        importer.parse_dotenv(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("KEY2").unwrap(), "value#notacomment");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value");
        assert_eq!(vars_map.get("KEY4").unwrap(), "value # not a comment in quotes");
    }

    #[test]
    fn test_parse_dotenv_edge_cases() {
        let mut importer = Importer::new();
        let content = r"
# Empty value
EMPTY=
# No spaces around equals
COMPACT=value
# Spaces in key should be ignored
INVALID KEY=value
# Key with spaces is invalid
KEY WITH SPACES=value
# Just equals sign
=value
# No equals sign
NOEQUALS
# Multiple equals signs
KEY=value=with=equals
# Unicode values
UNICODE=こんにちは
# Special characters
SPECIAL=!@#$%^&*()
";

        importer.parse_dotenv(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("EMPTY").unwrap(), "");
        assert_eq!(vars_map.get("COMPACT").unwrap(), "value");
        assert!(!vars_map.contains_key("INVALID KEY"));
        assert!(!vars_map.contains_key("KEY WITH SPACES"));
        assert!(!vars_map.contains_key("NOEQUALS"));
        assert_eq!(vars_map.get("KEY").unwrap(), "value=with=equals");
        assert_eq!(vars_map.get("UNICODE").unwrap(), "こんにちは");
        assert_eq!(vars_map.get("SPECIAL").unwrap(), "!@#$%^&*()");
    }

    #[test]
    fn test_parse_json_simple() {
        let mut importer = Importer::new();
        let content = r#"{
            "KEY1": "value1",
            "KEY2": "value2",
            "KEY3": "value with spaces"
        }"#;

        importer.parse_json(content).unwrap();
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("KEY2").unwrap(), "value2");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value with spaces");
    }

    #[test]
    fn test_parse_json_structured() {
        let mut importer = Importer::new();
        let content = r#"{
            "exported_at": "2024-01-01T00:00:00Z",
            "count": 3,
            "variables": [
                {"name": "KEY1", "value": "value1"},
                {"name": "KEY2", "value": "value2"},
                {"name": "KEY3", "value": "value3"}
            ]
        }"#;

        importer.parse_json(content).unwrap();
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.len(), 3);
        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("KEY2").unwrap(), "value2");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value3");
    }

    #[test]
    fn test_parse_json_invalid() {
        let mut importer = Importer::new();
        let content = "not valid json";

        assert!(importer.parse_json(content).is_err());
    }

    #[test]
    fn test_parse_json_non_string_values() {
        let mut importer = Importer::new();
        let content = r#"{
            "STRING": "value",
            "NUMBER": 42,
            "BOOLEAN": true,
            "NULL": null,
            "ARRAY": [1, 2, 3],
            "OBJECT": {"nested": "value"}
        }"#;

        importer.parse_json(content).unwrap();
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        // Only string values should be imported
        assert_eq!(vars_map.len(), 1);
        assert_eq!(vars_map.get("STRING").unwrap(), "value");
    }

    #[test]
    fn test_parse_yaml_basic() {
        let mut importer = Importer::new();
        let content = r"
# YAML comment
KEY1: value1
KEY2: value2
KEY3: value with spaces
---
KEY4: after document marker
";

        importer.parse_yaml(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("KEY2").unwrap(), "value2");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value with spaces");
        assert!(!vars_map.contains_key("KEY4")); // After --- should be ignored
    }

    #[test]
    fn test_parse_yaml_quoted() {
        let mut importer = Importer::new();
        let content = r#"
KEY1: "quoted value"
KEY2: 'single quoted'
KEY3: "value: with colon"
KEY4: unquoted: with colon
"#;

        importer.parse_yaml(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("KEY1").unwrap(), "quoted value");
        assert_eq!(vars_map.get("KEY2").unwrap(), "single quoted");
        assert_eq!(vars_map.get("KEY3").unwrap(), "value: with colon");
        assert_eq!(vars_map.get("KEY4").unwrap(), "unquoted: with colon");
    }

    #[test]
    fn test_parse_yaml_edge_cases() {
        let mut importer = Importer::new();
        let content = r"
# Empty value
EMPTY:
EMPTY2: 
# No space after colon
COMPACT:value
# Multiple colons
URL: http://example.com:8080
# Special characters
SPECIAL: !@#$%^&*()
";

        importer.parse_yaml(content);
        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.get("EMPTY").unwrap(), "");
        assert_eq!(vars_map.get("EMPTY2").unwrap(), "");
        assert_eq!(vars_map.get("COMPACT").unwrap(), "value");
        assert_eq!(vars_map.get("URL").unwrap(), "http://example.com:8080");
        assert_eq!(vars_map.get("SPECIAL").unwrap(), "!@#$%^&*()");
    }

    #[test]
    fn test_import_from_file_dotenv() {
        let content = "KEY1=value1\nKEY2=value2";
        let file = create_temp_file(content, ".env");

        let mut importer = Importer::new();
        importer
            .import_from_file(file.path().to_str().unwrap(), ImportFormat::DotEnv)
            .unwrap();

        let vars = importer.get_variables();
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_import_from_file_auto_detect() {
        // Test .env file
        let env_file = create_temp_file("KEY=value", ".env");
        let mut importer = Importer::new();
        let format = ImportFormat::from_extension(env_file.path().to_str().unwrap()).unwrap();
        importer
            .import_from_file(env_file.path().to_str().unwrap(), format)
            .unwrap();
        assert_eq!(importer.get_variables().len(), 1);

        // Test .json file
        let json_file = create_temp_file(r#"{"KEY": "value"}"#, ".json");
        let mut importer = Importer::new();
        let format = ImportFormat::from_extension(json_file.path().to_str().unwrap()).unwrap();
        importer
            .import_from_file(json_file.path().to_str().unwrap(), format)
            .unwrap();
        assert_eq!(importer.get_variables().len(), 1);
    }

    #[test]
    fn test_filter_by_patterns_exact() {
        let mut importer = Importer::new();
        importer.variables.insert("KEY1".to_string(), "value1".to_string());
        importer.variables.insert("KEY2".to_string(), "value2".to_string());
        importer.variables.insert("OTHER".to_string(), "value3".to_string());

        importer.filter_by_patterns(&["KEY1".to_string(), "KEY2".to_string()]);

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.len(), 2);
        assert!(vars_map.contains_key("KEY1"));
        assert!(vars_map.contains_key("KEY2"));
        assert!(!vars_map.contains_key("OTHER"));
    }

    #[test]
    fn test_filter_by_patterns_wildcard() {
        let mut importer = Importer::new();
        importer.variables.insert("API_KEY".to_string(), "value1".to_string());
        importer
            .variables
            .insert("API_SECRET".to_string(), "value2".to_string());
        importer
            .variables
            .insert("DATABASE_URL".to_string(), "value3".to_string());
        importer.variables.insert("OTHER".to_string(), "value4".to_string());

        importer.filter_by_patterns(&["API_*".to_string()]);

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.len(), 2);
        assert!(vars_map.contains_key("API_KEY"));
        assert!(vars_map.contains_key("API_SECRET"));
        assert!(!vars_map.contains_key("DATABASE_URL"));
    }

    #[test]
    fn test_filter_by_patterns_question_mark() {
        let mut importer = Importer::new();
        importer.variables.insert("KEY1".to_string(), "value1".to_string());
        importer.variables.insert("KEY2".to_string(), "value2".to_string());
        importer.variables.insert("KEY10".to_string(), "value3".to_string());
        importer.variables.insert("OTHER".to_string(), "value4".to_string());

        importer.filter_by_patterns(&["KEY?".to_string()]);

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.len(), 2);
        assert!(vars_map.contains_key("KEY1"));
        assert!(vars_map.contains_key("KEY2"));
        assert!(!vars_map.contains_key("KEY10")); // ? matches exactly one character
    }

    #[test]
    fn test_filter_by_patterns_multiple() {
        let mut importer = Importer::new();
        importer.variables.insert("API_KEY".to_string(), "value1".to_string());
        importer.variables.insert("DB_HOST".to_string(), "value2".to_string());
        importer.variables.insert("DB_PORT".to_string(), "value3".to_string());
        importer.variables.insert("OTHER".to_string(), "value4".to_string());

        importer.filter_by_patterns(&["API_*".to_string(), "DB_*".to_string()]);

        let vars = importer.get_variables();
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn test_add_prefix() {
        let mut importer = Importer::new();
        importer.variables.insert("KEY1".to_string(), "value1".to_string());
        importer.variables.insert("KEY2".to_string(), "value2".to_string());

        importer.add_prefix("PREFIX_");

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert_eq!(vars_map.len(), 2);
        assert!(vars_map.contains_key("PREFIX_KEY1"));
        assert!(vars_map.contains_key("PREFIX_KEY2"));
        assert!(!vars_map.contains_key("KEY1"));
        assert!(!vars_map.contains_key("KEY2"));
        assert_eq!(vars_map.get("PREFIX_KEY1").unwrap(), "value1");
        assert_eq!(vars_map.get("PREFIX_KEY2").unwrap(), "value2");
    }

    #[test]
    fn test_add_prefix_empty() {
        let mut importer = Importer::new();
        importer.variables.insert("KEY1".to_string(), "value1".to_string());

        importer.add_prefix("");

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert!(vars_map.contains_key("KEY1"));
        assert_eq!(vars_map.get("KEY1").unwrap(), "value1");
    }

    #[test]
    fn test_wildcard_to_regex() {
        // Test asterisk wildcard
        let regex = wildcard_to_regex("API_*");
        assert_eq!(regex, "^API_.*$");

        // Test question mark wildcard
        let regex = wildcard_to_regex("KEY?");
        assert_eq!(regex, "^KEY.$");

        // Test special regex characters are escaped
        let regex = wildcard_to_regex("KEY.VALUE");
        assert_eq!(regex, "^KEY\\.VALUE$");

        let regex = wildcard_to_regex("KEY[1]");
        assert_eq!(regex, "^KEY\\[1\\]$");

        // Test combination
        let regex = wildcard_to_regex("*_KEY_?");
        assert_eq!(regex, "^.*_KEY_.$");
    }

    #[test]
    fn test_complex_import_workflow() {
        // Create a complex .env file
        let content = r#"
# Database configuration
DB_HOST=localhost
DB_PORT=5432
DB_USER=admin
DB_PASSWORD="secret password"

# API configuration
API_KEY=abc123
API_SECRET=xyz789
API_URL=https://api.example.com

# Feature flags
FEATURE_NEW_UI=true
FEATURE_BETA=false

# Paths
APP_PATH=/usr/local/app
LOG_PATH=/var/log/app
"#;

        let file = create_temp_file(content, ".env");

        let mut importer = Importer::new();
        importer
            .import_from_file(file.path().to_str().unwrap(), ImportFormat::DotEnv)
            .unwrap();

        // Test initial import
        assert_eq!(importer.get_variables().len(), 11);

        // Filter to only DB variables
        importer.filter_by_patterns(&["DB_*".to_string()]);
        assert_eq!(importer.get_variables().len(), 4);

        // Add prefix
        importer.add_prefix("TEST_");

        let vars = importer.get_variables();
        let vars_map: HashMap<_, _> = vars.into_iter().collect();

        assert!(vars_map.contains_key("TEST_DB_HOST"));
        assert!(vars_map.contains_key("TEST_DB_PORT"));
        assert!(vars_map.contains_key("TEST_DB_USER"));
        assert!(vars_map.contains_key("TEST_DB_PASSWORD"));
        assert_eq!(vars_map.get("TEST_DB_PASSWORD").unwrap(), "secret password");
    }

    #[test]
    fn test_parse_text_format() {
        let mut importer = Importer::new();
        // Text format should behave like dotenv
        let content = "KEY1=value1\nKEY2=value2";

        importer.parse_text(content);
        let vars = importer.get_variables();

        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_empty_content() {
        let mut importer = Importer::new();

        importer.parse_dotenv("");
        assert_eq!(importer.get_variables().len(), 0);

        importer.parse_json("{}").unwrap();
        assert_eq!(importer.get_variables().len(), 0);

        importer.parse_yaml("");
        assert_eq!(importer.get_variables().len(), 0);
    }

    #[test]
    fn test_file_not_found() {
        let mut importer = Importer::new();
        let result = importer.import_from_file("/non/existent/file.env", ImportFormat::DotEnv);
        assert!(result.is_err());
    }
}

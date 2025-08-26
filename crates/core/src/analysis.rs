use crate::EnvVar;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct Analyzer {
    vars: Vec<EnvVar>,
}

impl Analyzer {
    #[must_use]
    pub const fn new(vars: Vec<EnvVar>) -> Self {
        Self { vars }
    }

    pub fn find_duplicates(&self) -> HashMap<String, Vec<&EnvVar>> {
        let mut duplicates = HashMap::new();
        let mut seen = HashMap::new();

        for var in &self.vars {
            seen.entry(var.name.to_uppercase()).or_insert_with(Vec::new).push(var);
        }

        for (name, vars) in seen {
            if vars.len() > 1 {
                duplicates.insert(name, vars);
            }
        }

        duplicates
    }

    #[must_use]
    pub fn validate_all(&self) -> HashMap<String, ValidationResult> {
        let mut results = HashMap::new();

        for var in &self.vars {
            let mut errors = Vec::new();
            let mut warnings = Vec::new();

            // Check for common issues
            if var.name.is_empty() {
                errors.push("Variable name is empty".to_string());
            }

            if var.name.contains(' ') {
                errors.push("Variable name contains spaces".to_string());
            }

            if var.name.starts_with(|c: char| c.is_numeric()) {
                errors.push("Variable name starts with a number".to_string());
            }

            // Check for PATH-like variables
            if var.name.to_uppercase().ends_with("PATH") {
                let path_analyzer = PathAnalyzer::new(&var.value);
                let path_result = path_analyzer.analyze();
                errors.extend(path_result.errors);
                warnings.extend(path_result.warnings);
            }

            let valid = errors.is_empty();
            results.insert(
                var.name.clone(),
                ValidationResult {
                    valid,
                    errors,
                    warnings,
                },
            );
        }

        results
    }

    #[must_use]
    pub fn find_unused(&self) -> Vec<&EnvVar> {
        // This is a simplified version - real implementation would check running processes
        self.vars
            .iter()
            .filter(|v| {
                // Check for common unused patterns
                v.name.starts_with("OLD_")
                    || v.name.starts_with("BACKUP_")
                    || v.name.ends_with("_OLD")
                    || v.name.ends_with("_BACKUP")
            })
            .collect()
    }

    #[must_use]
    pub fn analyze_dependencies(&self) -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();

        for var in &self.vars {
            let mut var_deps = Vec::new();

            // Check if this variable references other variables
            for other in &self.vars {
                if var.name != other.name && !other.name.is_empty() {
                    // Windows style: %VAR_NAME%
                    let pattern_windows = format!("%{}%", other.name);
                    // Unix style with braces: ${VAR_NAME}
                    let pattern_unix_braces = format!("${{{}}}", other.name);

                    if var.value.contains(&pattern_windows) || var.value.contains(&pattern_unix_braces) {
                        var_deps.push(other.name.clone());
                    } else {
                        // For $VAR_NAME pattern, we need to be more careful
                        // to avoid matching just $ at the end of string
                        let unix_pattern = format!("${}", other.name);
                        // Check if followed by a non-alphanumeric character or end of string
                        if let Some(pos) = var.value.find(&unix_pattern) {
                            let next_pos = pos + unix_pattern.len();
                            if next_pos == var.value.len()
                                || !var
                                    .value
                                    .chars()
                                    .nth(next_pos)
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                            {
                                var_deps.push(other.name.clone());
                            }
                        }
                    }
                }
            }

            // Remove duplicates
            var_deps.sort();
            var_deps.dedup();

            if !var_deps.is_empty() {
                deps.insert(var.name.clone(), var_deps);
            }
        }

        deps
    }
}

pub struct PathAnalyzer {
    paths: Vec<String>,
}

impl PathAnalyzer {
    #[must_use]
    pub fn new(path_value: &str) -> Self {
        let separator = if cfg!(windows) { ';' } else { ':' };
        let paths = path_value
            .split(separator)
            .map(std::string::ToString::to_string)
            .collect();

        Self { paths }
    }

    #[must_use]
    pub fn analyze(&self) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut seen = HashSet::new();

        for path_str in &self.paths {
            if path_str.is_empty() {
                warnings.push("Empty path entry found".to_string());
                continue;
            }

            // Check for duplicates
            if !seen.insert(path_str.to_lowercase()) {
                warnings.push(format!("Duplicate path entry: {path_str}"));
            }

            // Check if path exists
            let path = Path::new(path_str);
            if !path.exists() {
                errors.push(format!("Path does not exist: {path_str}"));
            } else if !path.is_dir() {
                errors.push(format!("Path is not a directory: {path_str}"));
            }

            // Check for common issues
            if path_str.contains("..") {
                warnings.push(format!("Path contains relative parent reference: {path_str}"));
            }

            #[cfg(windows)]
            {
                if path_str.contains('/') {
                    warnings.push(format!("Path uses Unix-style separators on Windows: {path_str}"));
                }
            }

            #[cfg(unix)]
            {
                if path_str.contains('\\') {
                    warnings.push(format!("Path uses Windows-style separators on Unix: {path_str}"));
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    #[must_use]
    pub fn get_duplicates(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut duplicates = Vec::new();

        for path in &self.paths {
            let normalized = path.to_lowercase();
            if !seen.insert(normalized.clone()) {
                duplicates.push(path.clone());
            }
        }

        duplicates
    }

    #[must_use]
    pub fn get_invalid(&self) -> Vec<String> {
        self.paths.iter().filter(|p| !Path::new(p).exists()).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvVar, EnvVarSource};
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    // Helper function to create test environment variables
    fn create_test_var(name: &str, value: &str) -> EnvVar {
        EnvVar {
            name: name.to_string(),
            value: value.to_string(),
            source: EnvVarSource::User,
            modified: Utc::now(),
            original_value: None,
        }
    }

    // Helper to create test variables with different sources
    fn create_test_vars() -> Vec<EnvVar> {
        vec![
            create_test_var("PATH", "/usr/bin:/usr/local/bin"),
            create_test_var("HOME", "/home/user"),
            create_test_var("JAVA_HOME", "/usr/lib/jvm/java-11"),
            create_test_var("PYTHON_PATH", "/usr/bin/python"),
            create_test_var("OLD_PATH", "/old/path"),
            create_test_var("BACKUP_HOME", "/backup/home"),
            create_test_var("API_KEY", "secret123"),
            create_test_var("DATABASE_URL", "postgres://localhost:5432/db"),
            create_test_var("APP_CONFIG", "${HOME}/config:${JAVA_HOME}/conf"),
            create_test_var("FULL_PATH", "%PATH%;%JAVA_HOME%\\bin"),
        ]
    }

    #[test]
    fn test_analyzer_new() {
        let vars = create_test_vars();
        let analyzer = Analyzer::new(vars.clone());
        assert_eq!(analyzer.vars.len(), vars.len());
    }

    #[test]
    fn test_find_duplicates_no_duplicates() {
        let vars = create_test_vars();
        let analyzer = Analyzer::new(vars);

        let duplicates = analyzer.find_duplicates();
        assert!(duplicates.is_empty());
    }

    #[test]
    fn test_find_duplicates_with_case_variations() {
        let vars = vec![
            create_test_var("PATH", "/usr/bin"),
            create_test_var("Path", "/usr/local/bin"),
            create_test_var("path", "/bin"),
            create_test_var("HOME", "/home/user"),
            create_test_var("home", "/home/user2"),
        ];

        let analyzer = Analyzer::new(vars);
        let duplicates = analyzer.find_duplicates();

        assert_eq!(duplicates.len(), 2); // PATH and HOME groups
        assert_eq!(duplicates.get("PATH").unwrap().len(), 3);
        assert_eq!(duplicates.get("HOME").unwrap().len(), 2);
    }

    #[test]
    fn test_validate_all_valid_variables() {
        let vars = vec![
            create_test_var("VALID_VAR", "value"),
            create_test_var("ANOTHER_VAR", "another value"),
            create_test_var("_UNDERSCORE_START", "value"),
        ];

        let analyzer = Analyzer::new(vars);
        let results = analyzer.validate_all();

        for (_, result) in results {
            assert!(result.valid);
            assert!(result.errors.is_empty());
        }
    }

    #[test]
    fn test_validate_all_invalid_names() {
        let vars = vec![
            create_test_var("", "empty name"),
            create_test_var("SPACE IN NAME", "value"),
            create_test_var("123_STARTS_WITH_NUMBER", "value"),
            create_test_var("VALID_NAME", "value"),
        ];

        let analyzer = Analyzer::new(vars);
        let results = analyzer.validate_all();

        // Empty name
        let empty_result = results.get("").unwrap();
        assert!(!empty_result.valid);
        assert!(empty_result.errors.iter().any(|e| e.contains("empty")));

        // Space in name
        let space_result = results.get("SPACE IN NAME").unwrap();
        assert!(!space_result.valid);
        assert!(space_result.errors.iter().any(|e| e.contains("spaces")));

        // Starts with number
        let number_result = results.get("123_STARTS_WITH_NUMBER").unwrap();
        assert!(!number_result.valid);
        assert!(number_result.errors.iter().any(|e| e.contains("number")));

        // Valid name
        let valid_result = results.get("VALID_NAME").unwrap();
        assert!(valid_result.valid);
    }

    #[test]
    fn test_validate_path_variables() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let valid_path = temp_dir.path().to_str().unwrap();
        let invalid_path = "/nonexistent/path/that/does/not/exist";

        let separator = if cfg!(windows) { ";" } else { ":" };
        let path_value = format!("{valid_path}{separator}{invalid_path}");

        let vars = vec![
            create_test_var("CUSTOM_PATH", &path_value),
            create_test_var("EMPTY_PATH", &format!("{valid_path}{separator}")),
        ];

        let analyzer = Analyzer::new(vars);
        let results = analyzer.validate_all();

        // CUSTOM_PATH should have errors about non-existent path
        let custom_result = results.get("CUSTOM_PATH").unwrap();
        assert!(!custom_result.valid);
        assert!(custom_result.errors.iter().any(|e| e.contains("does not exist")));

        // EMPTY_PATH should have warning about empty entry
        let empty_result = results.get("EMPTY_PATH").unwrap();
        assert!(empty_result.warnings.iter().any(|w| w.contains("Empty path entry")));
    }

    #[test]
    fn test_find_unused() {
        let vars = vec![
            create_test_var("ACTIVE_VAR", "value"),
            create_test_var("OLD_CONFIG", "old value"),
            create_test_var("BACKUP_PATH", "backup"),
            create_test_var("DATA_OLD", "old data"),
            create_test_var("CONFIG_BACKUP", "backup config"),
            create_test_var("CURRENT_VAR", "current"),
        ];

        let analyzer = Analyzer::new(vars);
        let unused = analyzer.find_unused();

        assert_eq!(unused.len(), 4);
        let unused_names: Vec<&str> = unused.iter().map(|v| v.name.as_str()).collect();
        assert!(unused_names.contains(&"OLD_CONFIG"));
        assert!(unused_names.contains(&"BACKUP_PATH"));
        assert!(unused_names.contains(&"DATA_OLD"));
        assert!(unused_names.contains(&"CONFIG_BACKUP"));
        assert!(!unused_names.contains(&"ACTIVE_VAR"));
        assert!(!unused_names.contains(&"CURRENT_VAR"));
    }

    #[test]
    fn test_analyze_dependencies_no_deps() {
        let vars = vec![
            create_test_var("VAR1", "value1"),
            create_test_var("VAR2", "value2"),
            create_test_var("VAR3", "value3"),
        ];

        let analyzer = Analyzer::new(vars);
        let deps = analyzer.analyze_dependencies();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_analyze_dependencies_with_references() {
        let vars = vec![
            create_test_var("HOME", "/home/user"),
            create_test_var("JAVA_HOME", "/usr/lib/jvm/java"),
            create_test_var("CONFIG_PATH", "${HOME}/config"),
            create_test_var("JAVA_BIN", "${JAVA_HOME}/bin"),
            create_test_var("FULL_PATH", "${HOME}/bin:${JAVA_HOME}/bin"),
            create_test_var("WINDOWS_PATH", "%HOME%;%JAVA_HOME%"),
        ];

        let analyzer = Analyzer::new(vars);
        let deps = analyzer.analyze_dependencies();

        // CONFIG_PATH depends on HOME
        assert!(deps.contains_key("CONFIG_PATH"));
        assert_eq!(deps.get("CONFIG_PATH").unwrap(), &vec!["HOME".to_string()]);

        // JAVA_BIN depends on JAVA_HOME
        assert!(deps.contains_key("JAVA_BIN"));
        assert_eq!(deps.get("JAVA_BIN").unwrap(), &vec!["JAVA_HOME".to_string()]);

        // FULL_PATH depends on both HOME and JAVA_HOME
        assert!(deps.contains_key("FULL_PATH"));
        let full_path_deps = deps.get("FULL_PATH").unwrap();
        assert_eq!(full_path_deps.len(), 2);
        assert!(full_path_deps.contains(&"HOME".to_string()));
        assert!(full_path_deps.contains(&"JAVA_HOME".to_string()));

        // WINDOWS_PATH also has dependencies
        assert!(deps.contains_key("WINDOWS_PATH"));
        let windows_deps = deps.get("WINDOWS_PATH").unwrap();
        assert_eq!(windows_deps.len(), 2);
    }

    #[test]
    fn test_path_analyzer_new() {
        let path_value = if cfg!(windows) {
            "C:\\Windows;C:\\Program Files;C:\\Users"
        } else {
            "/usr/bin:/usr/local/bin:/home/user/bin"
        };

        let analyzer = PathAnalyzer::new(path_value);
        assert_eq!(analyzer.paths.len(), 3);
    }

    #[test]
    fn test_path_analyzer_empty_entries() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let path_value = format!("/path1{separator}{separator}/path2");

        let analyzer = PathAnalyzer::new(&path_value);
        let result = analyzer.analyze();

        assert!(result.warnings.iter().any(|w| w.contains("Empty path entry")));
    }

    #[test]
    fn test_path_analyzer_duplicate_detection() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let path_value = format!("/path1{separator}/path2{separator}/path1{separator}/PATH1");

        let analyzer = PathAnalyzer::new(&path_value);
        let result = analyzer.analyze();

        assert!(result.warnings.iter().any(|w| w.contains("Duplicate")));

        let duplicates = analyzer.get_duplicates();
        assert!(!duplicates.is_empty());
    }

    #[test]
    fn test_path_analyzer_invalid_paths() {
        let path_value = if cfg!(windows) {
            "C:\\NonExistent;C:\\AlsoNonExistent"
        } else {
            "/nonexistent:/also/nonexistent"
        };

        let analyzer = PathAnalyzer::new(path_value);
        let result = analyzer.analyze();

        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("does not exist")));

        let invalid = analyzer.get_invalid();
        assert_eq!(invalid.len(), 2);
    }

    #[test]
    fn test_path_analyzer_relative_paths() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let path_value = format!("/absolute/path{separator}../relative/path");

        let analyzer = PathAnalyzer::new(&path_value);
        let result = analyzer.analyze();

        assert!(result.warnings.iter().any(|w| w.contains("relative parent reference")));
    }

    #[test]
    #[cfg(windows)]
    fn test_path_analyzer_wrong_separators_windows() {
        let path_value = "C:\\Windows;/unix/style/path";

        let analyzer = PathAnalyzer::new(path_value);
        let result = analyzer.analyze();

        assert!(result.warnings.iter().any(|w| w.contains("Unix-style separators")));
    }

    #[test]
    #[cfg(unix)]
    fn test_path_analyzer_wrong_separators_unix() {
        let path_value = "/usr/bin:C:\\Windows\\Style\\Path";

        let analyzer = PathAnalyzer::new(path_value);
        let result = analyzer.analyze();

        assert!(result.warnings.iter().any(|w| w.contains("Windows-style separators")));
    }

    #[test]
    fn test_path_analyzer_file_not_directory() {
        // Create a temporary file (not directory)
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.txt");
        fs::write(&temp_file, "test").unwrap();

        let path_value = temp_file.to_str().unwrap();
        let analyzer = PathAnalyzer::new(path_value);
        let result = analyzer.analyze();

        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("not a directory")));
    }

    #[test]
    fn test_complex_validation_scenario() {
        let temp_dir = TempDir::new().unwrap();
        let valid_path = temp_dir.path().to_str().unwrap();
        let separator = if cfg!(windows) { ";" } else { ":" };

        let vars = vec![
            create_test_var("", "empty name"),
            create_test_var("SPACE NAME", "value"),
            create_test_var("123START", "value"),
            create_test_var("VALID_PATH", valid_path),
            create_test_var("INVALID_PATH", "/nonexistent"),
            create_test_var("MIXED_PATH", &format!("{valid_path}{separator}/nonexistent")),
            create_test_var("OLD_VAR", "old value"),
            create_test_var("REF_VAR", "${VALID_PATH}/subdir"),
        ];

        let analyzer = Analyzer::new(vars);

        // Test validation
        let validation_results = analyzer.validate_all();
        assert!(!validation_results.get("").unwrap().valid);
        assert!(!validation_results.get("SPACE NAME").unwrap().valid);
        assert!(!validation_results.get("123START").unwrap().valid);
        assert!(validation_results.get("VALID_PATH").unwrap().valid);
        assert!(!validation_results.get("INVALID_PATH").unwrap().valid);
        assert!(!validation_results.get("MIXED_PATH").unwrap().valid);

        // Test unused detection
        let unused = analyzer.find_unused();
        assert!(unused.iter().any(|v| v.name == "OLD_VAR"));

        // Test dependency analysis
        let deps = analyzer.analyze_dependencies();
        assert!(deps.contains_key("REF_VAR"));
        assert_eq!(deps.get("REF_VAR").unwrap(), &vec!["VALID_PATH".to_string()]);
    }

    #[test]
    fn test_validation_result_structure() {
        let result = ValidationResult {
            valid: false,
            errors: vec!["Error 1".to_string(), "Error 2".to_string()],
            warnings: vec!["Warning 1".to_string()],
        };

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_case_insensitive_duplicate_detection() {
        let vars = vec![
            create_test_var("path", "/lower"),
            create_test_var("PATH", "/upper"),
            create_test_var("Path", "/mixed"),
            create_test_var("pAtH", "/weird"),
        ];

        let analyzer = Analyzer::new(vars);
        let duplicates = analyzer.find_duplicates();

        assert_eq!(duplicates.len(), 1);
        assert!(duplicates.contains_key("PATH"));
        assert_eq!(duplicates.get("PATH").unwrap().len(), 4);
    }

    #[test]
    fn test_empty_analyzer() {
        let analyzer = Analyzer::new(vec![]);

        assert!(analyzer.find_duplicates().is_empty());
        assert!(analyzer.validate_all().is_empty());
        assert!(analyzer.find_unused().is_empty());
        assert!(analyzer.analyze_dependencies().is_empty());
    }

    #[test]
    fn test_circular_dependencies() {
        let vars = vec![
            create_test_var("VAR_A", "${VAR_B}/a"),
            create_test_var("VAR_B", "${VAR_C}/b"),
            create_test_var("VAR_C", "${VAR_A}/c"),
        ];

        let analyzer = Analyzer::new(vars);
        let deps = analyzer.analyze_dependencies();

        assert!(deps.contains_key("VAR_A"));
        assert!(deps.contains_key("VAR_B"));
        assert!(deps.contains_key("VAR_C"));
        assert_eq!(deps.get("VAR_A").unwrap(), &vec!["VAR_B".to_string()]);
        assert_eq!(deps.get("VAR_B").unwrap(), &vec!["VAR_C".to_string()]);
        assert_eq!(deps.get("VAR_C").unwrap(), &vec!["VAR_A".to_string()]);
    }

    #[test]
    fn test_multiple_dependency_formats() {
        let vars = vec![
            create_test_var("BASE", "/base"),
            create_test_var("DEP1", "$BASE/path"),            // Unix style
            create_test_var("DEP2", "${BASE}/path"),          // Unix style with braces
            create_test_var("DEP3", "%BASE%\\path"),          // Windows style
            create_test_var("MULTI", "${BASE}:$BASE:%BASE%"), // Multiple references
        ];

        let analyzer = Analyzer::new(vars);
        let deps = analyzer.analyze_dependencies();

        assert!(deps.contains_key("DEP1"));
        assert!(deps.contains_key("DEP2"));
        assert!(deps.contains_key("DEP3"));
        assert!(deps.contains_key("MULTI"));

        // All should depend on BASE
        assert_eq!(deps.get("DEP1").unwrap(), &vec!["BASE".to_string()]);
        assert_eq!(deps.get("DEP2").unwrap(), &vec!["BASE".to_string()]);
        assert_eq!(deps.get("DEP3").unwrap(), &vec!["BASE".to_string()]);
        assert_eq!(deps.get("MULTI").unwrap(), &vec!["BASE".to_string()]);
    }
}

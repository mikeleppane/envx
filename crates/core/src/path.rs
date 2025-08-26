use color_eyre::Result;
use std::collections::HashSet;
use std::path::Path;

/// Manages PATH-like environment variables
pub struct PathManager {
    entries: Vec<String>,
    separator: char,
}

impl PathManager {
    #[must_use]
    pub fn new(path_value: &str) -> Self {
        let separator = if cfg!(windows) { ';' } else { ':' };
        let entries = path_value
            .split(separator)
            .filter(|s| !s.is_empty())
            .map(std::string::ToString::to_string)
            .collect();

        Self { entries, separator }
    }

    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        let normalized = Self::normalize_path(path);
        self.entries.iter().any(|e| Self::normalize_path(e) == normalized)
    }

    #[must_use]
    pub fn find_index(&self, path: &str) -> Option<usize> {
        let normalized = Self::normalize_path(path);
        self.entries.iter().position(|e| Self::normalize_path(e) == normalized)
    }

    pub fn add_first(&mut self, path: String) {
        self.entries.insert(0, path);
    }

    pub fn add_last(&mut self, path: String) {
        self.entries.push(path);
    }

    pub fn remove_first(&mut self, pattern: &str) -> usize {
        if let Some(idx) = self.find_index(pattern) {
            self.entries.remove(idx);
            1
        } else {
            0
        }
    }

    pub fn remove_all(&mut self, pattern: &str) -> usize {
        let normalized = Self::normalize_path(pattern);
        let original_len = self.entries.len();

        // Pre-normalize all entries to avoid borrowing self in the closure
        let normalized_entries: Vec<String> = self.entries.iter().map(|e| Self::normalize_path(e)).collect();

        // Keep only entries that don't match the normalized pattern
        let mut new_entries = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            if normalized_entries[i] != normalized {
                new_entries.push(entry.clone());
            }
        }
        self.entries = new_entries;

        original_len - self.entries.len()
    }

    /// Moves an entry from one position to another in the PATH entries.
    ///
    /// # Errors
    ///
    /// Returns an error if either `from` or `to` index is out of bounds.
    pub fn move_entry(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.entries.len() || to >= self.entries.len() {
            return Err(color_eyre::eyre::eyre!("Index out of bounds"));
        }

        if from == to {
            return Ok(()); // No-op if moving to same position
        }

        let entry = self.entries.remove(from);

        self.entries.insert(to, entry);

        Ok(())
    }

    #[must_use]
    pub fn get_invalid(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| !Path::new(e).exists())
            .cloned()
            .collect()
    }

    pub fn remove_invalid(&mut self) -> usize {
        let original_len = self.entries.len();
        self.entries.retain(|e| Path::new(e).exists());
        original_len - self.entries.len()
    }

    #[must_use]
    pub fn get_duplicates(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut duplicates = Vec::new();

        for entry in &self.entries {
            let normalized = Self::normalize_path(entry);
            if !seen.insert(normalized.clone()) {
                duplicates.push(entry.clone());
            }
        }

        duplicates
    }

    pub fn deduplicate(&mut self, keep_first: bool) -> usize {
        let mut seen = HashSet::new();
        let original_len = self.entries.len();

        if keep_first {
            // Keep first occurrence
            let mut deduped = Vec::new();
            for entry in &self.entries {
                let normalized = Self::normalize_path(entry);
                if seen.insert(normalized) {
                    deduped.push(entry.clone());
                }
            }
            self.entries = deduped;
        } else {
            // Keep last occurrence
            let mut deduped = Vec::new();
            for entry in self.entries.iter().rev() {
                let normalized = Self::normalize_path(entry);
                if seen.insert(normalized) {
                    deduped.push(entry.clone());
                }
            }
            deduped.reverse();
            self.entries = deduped;
        }

        original_len - self.entries.len()
    }

    #[must_use]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.entries.join(&self.separator.to_string())
    }

    /// Normalize path for comparison (handle case sensitivity and trailing slashes)
    fn normalize_path(path: &str) -> String {
        let mut normalized = path.to_string();

        // Remove trailing slashes
        while normalized.ends_with('/') || normalized.ends_with('\\') {
            normalized.pop();
        }

        // On Windows, normalize to lowercase for case-insensitive comparison
        #[cfg(windows)]
        {
            normalized = normalized.to_lowercase();
        }

        // Convert forward slashes to backslashes on Windows
        #[cfg(windows)]
        {
            normalized = normalized.replace('/', "\\");
        }

        // Convert backslashes to forward slashes on Unix
        #[cfg(unix)]
        {
            normalized = normalized.replace('\\', "/");
        }

        normalized
    }
}

// ...existing code...

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a PathManager with test data
    fn create_test_manager() -> PathManager {
        let path = if cfg!(windows) {
            "C:\\Windows;C:\\Program Files;C:\\Users\\Test;C:\\Windows;D:\\Tools"
        } else {
            "/usr/bin:/usr/local/bin:/home/user/bin:/usr/bin:/opt/tools"
        };
        PathManager::new(path)
    }

    #[test]
    fn test_new_empty() {
        let mgr = PathManager::new("");
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_new_with_paths() {
        let mgr = create_test_manager();
        assert!(!mgr.is_empty());
        assert_eq!(mgr.len(), 5);
    }

    #[test]
    fn test_new_filters_empty_entries() {
        let path = if cfg!(windows) {
            "C:\\Windows;;C:\\Program Files;;;D:\\Tools;"
        } else {
            "/usr/bin::/usr/local/bin:::/opt/tools:"
        };
        let mgr = PathManager::new(path);
        assert_eq!(mgr.len(), 3);
    }

    #[test]
    fn test_separator_detection() {
        let mgr = PathManager::new("");
        if cfg!(windows) {
            assert_eq!(mgr.separator, ';');
        } else {
            assert_eq!(mgr.separator, ':');
        }
    }

    #[test]
    fn test_entries() {
        let mgr = create_test_manager();
        let entries = mgr.entries();
        assert_eq!(entries.len(), 5);
        if cfg!(windows) {
            assert!(entries.contains(&"C:\\Windows".to_string()));
            assert!(entries.contains(&"C:\\Program Files".to_string()));
        } else {
            assert!(entries.contains(&"/usr/bin".to_string()));
            assert!(entries.contains(&"/usr/local/bin".to_string()));
        }
    }

    #[test]
    fn test_contains() {
        let mgr = create_test_manager();
        if cfg!(windows) {
            assert!(mgr.contains("C:\\Windows"));
            assert!(mgr.contains("c:\\windows")); // Case insensitive on Windows
            assert!(mgr.contains("C:/Windows")); // Forward slash normalization
            assert!(!mgr.contains("C:\\NonExistent"));
        } else {
            assert!(mgr.contains("/usr/bin"));
            assert!(mgr.contains("/usr/bin/")); // Trailing slash normalization
            assert!(!mgr.contains("/nonexistent"));
        }
    }

    #[test]
    fn test_contains_with_trailing_slashes() {
        let mgr = create_test_manager();
        if cfg!(windows) {
            assert!(mgr.contains("C:\\Windows\\"));
            assert!(mgr.contains("C:\\Windows/"));
        } else {
            assert!(mgr.contains("/usr/bin/"));
        }
    }

    #[test]
    fn test_find_index() {
        let mgr = create_test_manager();
        if cfg!(windows) {
            assert_eq!(mgr.find_index("C:\\Windows"), Some(0));
            assert_eq!(mgr.find_index("C:\\Program Files"), Some(1));
            assert_eq!(mgr.find_index("D:\\Tools"), Some(4));
            assert_eq!(mgr.find_index("C:\\NonExistent"), None);
        } else {
            assert_eq!(mgr.find_index("/usr/bin"), Some(0));
            assert_eq!(mgr.find_index("/opt/tools"), Some(4));
            assert_eq!(mgr.find_index("/nonexistent"), None);
        }
    }

    #[test]
    fn test_add_first() {
        let mut mgr = create_test_manager();
        let original_len = mgr.len();

        if cfg!(windows) {
            mgr.add_first("C:\\NewPath".to_string());
            assert_eq!(mgr.entries()[0], "C:\\NewPath");
        } else {
            mgr.add_first("/new/path".to_string());
            assert_eq!(mgr.entries()[0], "/new/path");
        }
        assert_eq!(mgr.len(), original_len + 1);
    }

    #[test]
    fn test_add_last() {
        let mut mgr = create_test_manager();
        let original_len = mgr.len();

        if cfg!(windows) {
            mgr.add_last("C:\\NewPath".to_string());
            assert_eq!(mgr.entries()[mgr.len() - 1], "C:\\NewPath");
        } else {
            mgr.add_last("/new/path".to_string());
            assert_eq!(mgr.entries()[mgr.len() - 1], "/new/path");
        }
        assert_eq!(mgr.len(), original_len + 1);
    }

    #[test]
    fn test_remove_first() {
        let mut mgr = create_test_manager();
        let original_len = mgr.len();

        if cfg!(windows) {
            let removed = mgr.remove_first("C:\\Windows");
            assert_eq!(removed, 1);
            assert_eq!(mgr.len(), original_len - 1);
            // Should only remove first occurrence
            assert!(mgr.contains("C:\\Windows")); // Second occurrence still there

            let removed = mgr.remove_first("C:\\NonExistent");
            assert_eq!(removed, 0);
            assert_eq!(mgr.len(), original_len - 1);
        } else {
            let removed = mgr.remove_first("/usr/bin");
            assert_eq!(removed, 1);
            assert_eq!(mgr.len(), original_len - 1);
            // Should only remove first occurrence
            assert!(mgr.contains("/usr/bin")); // Second occurrence still there
        }
    }

    #[test]
    fn test_remove_all() {
        let mut mgr = create_test_manager();

        if cfg!(windows) {
            let removed = mgr.remove_all("C:\\Windows");
            assert_eq!(removed, 2); // There are two C:\Windows entries
            assert!(!mgr.contains("C:\\Windows"));
            assert_eq!(mgr.len(), 3);
        } else {
            let removed = mgr.remove_all("/usr/bin");
            assert_eq!(removed, 2); // There are two /usr/bin entries
            assert!(!mgr.contains("/usr/bin"));
            assert_eq!(mgr.len(), 3);
        }
    }

    #[test]
    fn test_remove_all_nonexistent() {
        let mut mgr = create_test_manager();
        let original_len = mgr.len();

        let removed = mgr.remove_all("NonExistent");
        assert_eq!(removed, 0);
        assert_eq!(mgr.len(), original_len);
    }

    #[test]
    fn test_move_entry() {
        let mut mgr = create_test_manager();
        let first = mgr.entries()[0].clone();
        let second = mgr.entries()[1].clone();

        // Move first to second position
        assert!(mgr.move_entry(0, 1).is_ok());
        assert_eq!(mgr.entries()[0], second);
        assert_eq!(mgr.entries()[1], first);

        // Move back
        assert!(mgr.move_entry(1, 0).is_ok());
        assert_eq!(mgr.entries()[0], first);
        assert_eq!(mgr.entries()[1], second);
    }

    #[test]
    fn test_move_entry_to_end() {
        let mut mgr = create_test_manager();
        let first = mgr.entries()[0].clone();
        let last_idx = mgr.len() - 1;

        assert!(mgr.move_entry(0, last_idx).is_ok());
        assert_eq!(mgr.entries()[last_idx], first);
    }

    #[test]
    fn test_move_entry_out_of_bounds() {
        let mut mgr = create_test_manager();

        assert!(mgr.move_entry(10, 0).is_err());
        assert!(mgr.move_entry(0, 10).is_err());
        assert!(mgr.move_entry(10, 10).is_err());
    }

    #[test]
    fn test_get_duplicates() {
        let mgr = create_test_manager();
        let duplicates = mgr.get_duplicates();

        if cfg!(windows) {
            assert_eq!(duplicates.len(), 1);
            assert_eq!(duplicates[0], "C:\\Windows");
        } else {
            assert_eq!(duplicates.len(), 1);
            assert_eq!(duplicates[0], "/usr/bin");
        }
    }

    #[test]
    fn test_get_duplicates_no_dupes() {
        let path = if cfg!(windows) {
            "C:\\Path1;C:\\Path2;C:\\Path3"
        } else {
            "/path1:/path2:/path3"
        };
        let mgr = PathManager::new(path);
        let duplicates = mgr.get_duplicates();
        assert!(duplicates.is_empty());
    }

    #[test]
    fn test_get_duplicates_case_insensitive_windows() {
        if cfg!(windows) {
            let mgr = PathManager::new("C:\\Windows;c:\\windows;C:\\WINDOWS");
            let duplicates = mgr.get_duplicates();
            assert_eq!(duplicates.len(), 2); // First one is not a duplicate
        }
    }

    #[test]
    fn test_deduplicate_keep_first() {
        let mut mgr = create_test_manager();
        let removed = mgr.deduplicate(true);

        assert_eq!(removed, 1); // One duplicate removed
        assert_eq!(mgr.len(), 4);

        // Check no duplicates remain
        let duplicates = mgr.get_duplicates();
        assert!(duplicates.is_empty());

        // Verify first occurrence was kept
        if cfg!(windows) {
            assert_eq!(mgr.entries()[0], "C:\\Windows");
        } else {
            assert_eq!(mgr.entries()[0], "/usr/bin");
        }
    }

    #[test]
    fn test_deduplicate_keep_last() {
        let mut mgr = create_test_manager();
        let removed = mgr.deduplicate(false);

        assert_eq!(removed, 1); // One duplicate removed
        assert_eq!(mgr.len(), 4);

        // Check no duplicates remain
        let duplicates = mgr.get_duplicates();
        assert!(duplicates.is_empty());

        // Verify last occurrence was kept
        if cfg!(windows) {
            // C:\Windows was at index 0 and 3, so after dedup keeping last, it should be at index 2
            assert!(mgr.contains("C:\\Windows"));
            assert_eq!(mgr.find_index("C:\\Windows"), Some(2));
        } else {
            assert!(mgr.contains("/usr/bin"));
            assert_eq!(mgr.find_index("/usr/bin"), Some(2));
        }
    }

    #[test]
    fn test_to_string() {
        let mgr = create_test_manager();
        let result = mgr.to_string();

        if cfg!(windows) {
            // On Windows, paths are separated by semicolons
            assert!(result.contains(';'));
            // Windows paths can contain colons (e.g., C:), so don't check for absence of colons
            assert!(result.contains("C:\\Windows"));
            assert!(result.contains("C:\\Program Files"));

            // Verify the separator is used correctly by counting occurrences
            let separator_count = result.matches(';').count();
            assert_eq!(separator_count, mgr.len() - 1); // n-1 separators for n entries
        } else {
            // On Unix, paths are separated by colons
            assert!(result.contains(':'));
            assert!(!result.contains(';'));
            assert!(result.contains("/usr/bin"));
            assert!(result.contains("/usr/local/bin"));

            // Verify the separator is used correctly by counting occurrences
            let separator_count = result.matches(':').count();
            assert_eq!(separator_count, mgr.len() - 1); // n-1 separators for n entries
        }
    }

    #[test]
    fn test_to_string_empty() {
        let mgr = PathManager::new("");
        assert_eq!(mgr.to_string(), "");
    }

    #[test]
    fn test_to_string_single_entry() {
        let mut mgr = PathManager::new("");
        if cfg!(windows) {
            mgr.add_first("C:\\Single".to_string());
            assert_eq!(mgr.to_string(), "C:\\Single");
        } else {
            mgr.add_first("/single".to_string());
            assert_eq!(mgr.to_string(), "/single");
        }
    }

    #[test]
    fn test_normalize_path_trailing_slashes() {
        if cfg!(windows) {
            assert_eq!(PathManager::normalize_path("C:\\Path\\"), "c:\\path");
            assert_eq!(PathManager::normalize_path("C:\\Path/"), "c:\\path");
            assert_eq!(PathManager::normalize_path("C:\\Path\\\\"), "c:\\path");
        } else {
            assert_eq!(PathManager::normalize_path("/path/"), "/path");
            assert_eq!(PathManager::normalize_path("/path//"), "/path");
        }
    }

    #[test]
    fn test_normalize_path_case_sensitivity() {
        if cfg!(windows) {
            // Windows: case-insensitive
            assert_eq!(
                PathManager::normalize_path("C:\\Path"),
                PathManager::normalize_path("c:\\path")
            );
            assert_eq!(
                PathManager::normalize_path("C:\\PATH"),
                PathManager::normalize_path("c:\\path")
            );
        } else {
            // Unix: case-sensitive
            assert_ne!(
                PathManager::normalize_path("/Path"),
                PathManager::normalize_path("/path")
            );
            assert_ne!(
                PathManager::normalize_path("/PATH"),
                PathManager::normalize_path("/path")
            );
        }
    }

    #[test]
    fn test_normalize_path_slash_conversion() {
        if cfg!(windows) {
            // Windows: convert forward slashes to backslashes
            assert_eq!(PathManager::normalize_path("C:/Path/To/Dir"), "c:\\path\\to\\dir");
            assert_eq!(PathManager::normalize_path("C:\\Path/To\\Dir"), "c:\\path\\to\\dir");
        } else {
            // Unix: convert backslashes to forward slashes
            assert_eq!(PathManager::normalize_path("/path\\to\\dir"), "/path/to/dir");
            assert_eq!(PathManager::normalize_path("/path\\to/dir"), "/path/to/dir");
        }
    }

    // Note: get_invalid() and remove_invalid() tests would require actual filesystem
    // operations or mocking, which is beyond the scope of unit tests.
    // These would be better as integration tests.

    #[test]
    fn test_complex_scenario() {
        let mut mgr = PathManager::new("");

        // Build a complex PATH
        if cfg!(windows) {
            mgr.add_last("C:\\Windows".to_string());
            mgr.add_last("C:\\Program Files".to_string());
            mgr.add_first("C:\\Priority".to_string());
            mgr.add_last("C:\\Windows".to_string()); // Duplicate
            mgr.add_last("c:\\program files".to_string()); // Case variant duplicate

            assert_eq!(mgr.len(), 5);

            // Remove duplicates
            let removed = mgr.deduplicate(true);
            assert_eq!(removed, 2);
            assert_eq!(mgr.len(), 3);

            // Verify order
            assert_eq!(mgr.entries()[0], "C:\\Priority");
            assert_eq!(mgr.entries()[1], "C:\\Windows");
            assert_eq!(mgr.entries()[2], "C:\\Program Files");
        } else {
            mgr.add_last("/usr/bin".to_string());
            mgr.add_last("/usr/local/bin".to_string());
            mgr.add_first("/priority".to_string());
            mgr.add_last("/usr/bin".to_string()); // Duplicate
            mgr.add_last("/usr/local/bin/".to_string()); // Trailing slash duplicate

            assert_eq!(mgr.len(), 5);

            // Remove duplicates
            let removed = mgr.deduplicate(true);
            assert_eq!(removed, 2);
            assert_eq!(mgr.len(), 3);

            // Verify order
            assert_eq!(mgr.entries()[0], "/priority");
            assert_eq!(mgr.entries()[1], "/usr/bin");
            assert_eq!(mgr.entries()[2], "/usr/local/bin");
        }
    }
}

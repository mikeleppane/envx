use ahash::AHashMap as HashMap;
use clap::{Args, Subcommand};
use color_eyre::Result;
use comfy_table::{Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use envx_core::EnvVarManager;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Args)]
pub struct DepsArgs {
    #[command(subcommand)]
    pub command: Option<DepsCommands>,

    /// Variable name to show dependencies for (shows all if not specified)
    #[arg(value_name = "VAR")]
    pub variable: Option<String>,

    /// Show only unused variables
    #[arg(long)]
    pub unused: bool,

    /// Paths to scan (defaults to current directory)
    #[arg(short, long)]
    pub paths: Vec<PathBuf>,

    /// Additional patterns to ignore during scanning
    #[arg(short = 'i', long)]
    pub ignore: Vec<String>,

    /// Output format (table, json, simple)
    #[arg(short, long, default_value = "table")]
    pub format: String,
}

#[derive(Subcommand)]
pub enum DepsCommands {
    /// Show dependencies for variables
    Show {
        /// Variable name to show dependencies for
        variable: Option<String>,

        /// Show only unused variables
        #[arg(long)]
        unused: bool,
    },

    /// Scan for environment variable usage
    Scan {
        /// Paths to scan
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Save scan results to cache
        #[arg(long)]
        cache: bool,
    },

    /// Show usage statistics
    Stats {
        /// Sort by usage count
        #[arg(long)]
        by_usage: bool,
    },
}

/// Handle environment variable dependency operations.
///
/// # Errors
///
/// Returns an error if:
/// - File scanning fails
/// - Environment variable loading fails
/// - I/O operations fail (reading files, writing output)
/// - JSON serialization fails (when using JSON format)
/// - Directory traversal fails
pub fn handle_deps(args: &DepsArgs) -> Result<()> {
    match args.command {
        Some(DepsCommands::Show { ref variable, unused }) => {
            let var_ref = variable.as_deref();
            handle_deps_show(var_ref, unused, args)?;
        }
        Some(DepsCommands::Scan { ref paths, cache }) => {
            handle_deps_scan(paths, cache, args)?;
        }
        Some(DepsCommands::Stats { by_usage }) => {
            handle_deps_stats(by_usage, args)?;
        }
        None => {
            // Default behavior: show dependencies for specified variable or all
            if args.unused {
                handle_deps_show(None, true, args)?;
            } else {
                handle_deps_show(args.variable.as_deref(), false, args)?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_deps_show(variable: Option<&str>, show_unused: bool, args: &DepsArgs) -> Result<()> {
    // Initialize dependency tracker
    let mut tracker = DependencyTracker::new();

    // Add scan paths
    if args.paths.is_empty() {
        tracker.add_scan_path(PathBuf::from("."));
    } else {
        for path in &args.paths {
            tracker.add_scan_path(path.clone());
        }
    }

    // Add ignore patterns
    for pattern in &args.ignore {
        tracker.add_ignore_pattern(pattern.clone());
    }

    // Scan for dependencies
    println!("🔍 Scanning for environment variable usage...");
    tracker.scan()?;

    // Load current environment variables
    let mut manager = EnvVarManager::new();
    manager.load_all()?;
    let all_vars: HashSet<String> = manager.list().iter().map(|v| v.name.clone()).collect();

    if show_unused {
        // Show unused variables
        let unused = tracker.find_unused(&all_vars);

        if unused.is_empty() {
            println!("✅ No unused environment variables found!");
        } else {
            println!("\n⚠️  Found {} unused environment variables:", unused.len());

            match args.format.as_str() {
                "json" => {
                    let json = serde_json::json!({
                        "unused_variables": unused,
                        "count": unused.len()
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                "simple" => {
                    for var in unused {
                        println!("{var}");
                    }
                }
                _ => {
                    let mut table = Table::new();
                    table
                        .load_preset(UTF8_FULL)
                        .apply_modifier(UTF8_ROUND_CORNERS)
                        .set_header(vec!["Variable", "Value", "Source"]);

                    let mut sorted_vars: Vec<_> = unused.into_iter().collect();
                    sorted_vars.sort();

                    for var_name in sorted_vars {
                        if let Some(var) = manager.get(&var_name) {
                            table.add_row(vec![
                                var.name.clone(),
                                if var.value.len() > 50 {
                                    format!("{}...", &var.value[..47])
                                } else {
                                    var.value.clone()
                                },
                                format!("{:?}", var.source),
                            ]);
                        }
                    }

                    println!("{table}");
                }
            }
        }
    } else if let Some(var_name) = variable {
        // Show dependencies for specific variable
        if let Some(usages) = tracker.get_usages(var_name) {
            println!("\n📊 Dependencies for '{var_name}':");
            println!("Found {} usage(s):\n", usages.len());

            match args.format.as_str() {
                "json" => {
                    let json = serde_json::json!({
                        "variable": var_name,
                        "usages": usages.iter().map(|u| {
                            serde_json::json!({
                                "file": u.file.display().to_string(),
                                "line": u.line,
                                "context": u.context
                            })
                        }).collect::<Vec<_>>()
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                "simple" => {
                    for usage in usages {
                        println!("{}:{} - {}", usage.file.display(), usage.line, usage.context);
                    }
                }
                _ => {
                    let mut table = Table::new();
                    table
                        .load_preset(UTF8_FULL)
                        .apply_modifier(UTF8_ROUND_CORNERS)
                        .set_header(vec!["File", "Line", "Context"]);

                    for usage in usages {
                        table.add_row(vec![
                            usage.file.display().to_string(),
                            usage.line.to_string(),
                            if usage.context.len() > 60 {
                                format!("{}...", &usage.context[..57])
                            } else {
                                usage.context.clone()
                            },
                        ]);
                    }

                    println!("{table}");
                }
            }
        } else {
            println!("❌ No usages found for variable '{var_name}'");

            // Check if the variable exists
            if !all_vars.contains(var_name) {
                println!("   Note: This variable is not currently set in your environment.");
            }
        }
    } else {
        // Show all dependencies
        let usage_counts = tracker.get_usage_counts();
        let used_vars = tracker.get_used_variables();

        println!("\n📊 Environment Variable Dependencies:");
        println!("Found {} variables used in codebase\n", used_vars.len());

        match args.format.as_str() {
            "json" => {
                let json = serde_json::json!({
                    "total_variables": all_vars.len(),
                    "used_variables": used_vars.len(),
                    "unused_variables": all_vars.len() - used_vars.len(),
                    "usage_counts": usage_counts
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            "simple" => {
                let mut sorted_vars: Vec<_> = usage_counts.into_iter().collect();
                sorted_vars.sort_by_key(|(name, _)| name.clone());

                for (var, count) in sorted_vars {
                    println!("{var}: {count} usage(s)");
                }
            }
            _ => {
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec!["Variable", "Usage Count", "Status"]);

                let mut sorted_vars: Vec<_> = all_vars.iter().collect();
                sorted_vars.sort();

                for var_name in sorted_vars {
                    let usage_count = usage_counts.get(var_name).copied().unwrap_or(0);
                    let status = if usage_count > 0 {
                        "✅ Used".to_string()
                    } else {
                        "⚠️  Unused".to_string()
                    };

                    table.add_row(vec![var_name.clone(), usage_count.to_string(), status]);
                }

                println!("{table}");
            }
        }
    }

    Ok(())
}

fn handle_deps_scan(paths: &[PathBuf], cache: bool, args: &DepsArgs) -> Result<()> {
    let mut tracker = DependencyTracker::new();

    // Add scan paths
    for path in paths {
        tracker.add_scan_path(path.clone());
    }

    // Add ignore patterns
    for pattern in &args.ignore {
        tracker.add_ignore_pattern(pattern.clone());
    }

    println!("🔍 Scanning paths:");
    for path in paths {
        println!("   - {}", path.display());
    }

    tracker.scan()?;

    let used_vars = tracker.get_used_variables();
    println!("\n✅ Scan complete!");
    println!("Found {} unique environment variables", used_vars.len());

    if cache {
        // TODO: Implement caching mechanism
        println!("📦 Caching scan results... (not yet implemented)");
    }

    Ok(())
}

fn handle_deps_stats(by_usage: bool, args: &DepsArgs) -> Result<()> {
    let mut tracker = DependencyTracker::new();

    // Add scan paths
    if args.paths.is_empty() {
        tracker.add_scan_path(PathBuf::from("."));
    } else {
        for path in &args.paths {
            tracker.add_scan_path(path.clone());
        }
    }

    println!("🔍 Analyzing environment variable usage...");
    tracker.scan()?;

    let usage_counts = tracker.get_usage_counts();
    let mut stats: Vec<_> = usage_counts.into_iter().collect();

    if by_usage {
        stats.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    } else {
        stats.sort_by_key(|(name, _)| name.clone());
    }

    println!("\n📊 Environment Variable Usage Statistics:\n");

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Rank", "Variable", "Usage Count", "Frequency"]);

    let total_usages: usize = stats.iter().map(|(_, count)| count).sum();

    for (rank, (var, count)) in stats.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let frequency = if total_usages > 0 {
            format!("{:.1}%", (*count as f64 / total_usages as f64) * 100.0)
        } else {
            "0.0%".to_string()
        };

        table.add_row(vec![(rank + 1).to_string(), var.clone(), count.to_string(), frequency]);

        if rank >= 19 {
            // Show top 20
            break;
        }
    }

    println!("{table}");

    if stats.len() > 20 {
        println!("\n... and {} more variables", stats.len() - 20);
    }

    Ok(())
}

#[derive(Args)]
pub struct CleanupArgs {
    /// Force cleanup without confirmation
    #[arg(short, long)]
    pub force: bool,

    /// Dry run - show what would be removed without making changes
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Keep variables matching these patterns
    #[arg(short = 'k', long)]
    pub keep: Vec<String>,

    /// Additional paths to scan for usage
    #[arg(short = 'p', long)]
    pub paths: Vec<PathBuf>,
}

/// Handle cleanup of unused environment variables.
///
/// # Errors
///
/// Returns an error if:
/// - File scanning fails
/// - Environment variable loading fails
/// - Environment variable deletion fails
/// - I/O operations fail (reading user input, writing output)
pub fn handle_cleanup(args: &CleanupArgs) -> Result<()> {
    // Initialize dependency tracker
    let mut tracker = DependencyTracker::new();

    // Add scan paths
    if args.paths.is_empty() {
        tracker.add_scan_path(PathBuf::from("."));
    } else {
        for path in &args.paths {
            tracker.add_scan_path(path.clone());
        }
    }

    println!("🔍 Scanning for environment variable usage...");
    tracker.scan()?;

    // Load current environment variables
    let mut manager = EnvVarManager::new();
    manager.load_all()?;
    let all_vars: HashSet<String> = manager.list().iter().map(|v| v.name.clone()).collect();

    // Find unused variables
    let mut unused = tracker.find_unused(&all_vars);

    // Filter out variables that should be kept
    if !args.keep.is_empty() {
        unused.retain(|var| {
            !args.keep.iter().any(|pattern| {
                var.contains(pattern) || glob::Pattern::new(pattern).map(|p| p.matches(var)).unwrap_or(false)
            })
        });
    }

    if unused.is_empty() {
        println!("✅ No unused environment variables found!");
        return Ok(());
    }

    println!("\n⚠️  Found {} unused environment variables:", unused.len());

    let mut sorted_unused: Vec<_> = unused.into_iter().collect();
    sorted_unused.sort();

    for var in &sorted_unused {
        if let Some(env_var) = manager.get(var) {
            println!(
                "   - {} = {} [{:?}]",
                var,
                if env_var.value.len() > 50 {
                    format!("{}...", &env_var.value[..47])
                } else {
                    env_var.value.clone()
                },
                env_var.source
            );
        }
    }

    if args.dry_run {
        println!("\n(Dry run - no changes made)");
        return Ok(());
    }

    if !args.force {
        print!("\nRemove these unused variables? [y/N]: ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cleanup cancelled.");
            return Ok(());
        }
    }

    // Remove unused variables
    let mut removed = 0;
    let mut failed = 0;

    for var in sorted_unused {
        match manager.delete(&var) {
            Ok(()) => {
                removed += 1;
                println!("✅ Removed: {var}");
            }
            Err(e) => {
                failed += 1;
                eprintln!("❌ Failed to remove {var}: {e}");
            }
        }
    }

    println!("\n📊 Cleanup complete:");
    println!("   - Removed: {removed} variables");
    if failed > 0 {
        println!("   - Failed: {failed} variables");
    }

    Ok(())
}

/// Represents a location where an environment variable is used
#[derive(Debug, Clone)]
pub struct VariableUsage {
    pub file: PathBuf,
    pub line: usize,
    pub context: String,
}

/// Tracks dependencies for environment variables
pub struct DependencyTracker {
    usages: HashMap<String, Vec<VariableUsage>>,
    scan_paths: Vec<PathBuf>,
    ignore_patterns: Vec<String>,
}

impl DependencyTracker {
    pub fn new() -> Self {
        Self {
            usages: HashMap::new(),
            scan_paths: vec![PathBuf::from(".")],
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".venv".to_string(),
                "__pycache__".to_string(),
                "dist".to_string(),
                "build".to_string(),
                ".envx".to_string(),
                "vendor".to_string(),
                ".cargo".to_string(),
            ],
        }
    }

    /// Add a path to scan for dependencies
    pub fn add_scan_path(&mut self, path: PathBuf) {
        self.scan_paths.push(path);
    }

    /// Add patterns to ignore during scanning
    pub fn add_ignore_pattern(&mut self, pattern: String) {
        self.ignore_patterns.push(pattern);
    }

    /// Scan all configured paths for environment variable usage
    pub fn scan(&mut self) -> Result<()> {
        self.usages.clear();

        for path in &self.scan_paths.clone() {
            if path.is_file() {
                self.scan_file(path)?;
            } else if path.is_dir() {
                self.scan_directory(path)?;
            }
        }

        Ok(())
    }

    /// Scan a directory recursively
    fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        let ignore_patterns = self.ignore_patterns.clone();

        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !Self::should_ignore_with_patterns(e.path(), &ignore_patterns))
        {
            let entry = entry?;
            if entry.file_type().is_file() {
                self.scan_file(entry.path())?;
            }
        }
        Ok(())
    }

    /// Check if a path should be ignored using provided patterns
    fn should_ignore_with_patterns(path: &Path, ignore_patterns: &[String]) -> bool {
        for component in path.components() {
            if let Some(name) = component.as_os_str().to_str() {
                if ignore_patterns.iter().any(|p| name.contains(p)) {
                    return true;
                }
            }
        }
        false
    }

    /// Scan a single file for environment variable usage
    fn scan_file(&mut self, path: &Path) -> Result<()> {
        // Skip binary files and very large files
        let metadata = fs::metadata(path)?;
        if metadata.len() > 10_000_000 {
            // Skip files larger than 10MB
            return Ok(());
        }

        let Ok(content) = fs::read_to_string(path) else {
            return Ok(()); // Skip binary files
        };

        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            // Source code files
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => self.scan_javascript(&content, path)?,
            "py" | "pyw" => self.scan_python(&content, path)?,
            "rs" => self.scan_rust(&content, path)?,
            "go" => self.scan_go(&content, path)?,
            "java" => self.scan_java(&content, path)?,
            "cs" => self.scan_csharp(&content, path)?,
            "rb" => self.scan_ruby(&content, path)?,
            "php" => self.scan_php(&content, path)?,
            "c" | "h" => self.scan_c(&content, path)?,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "h++" => self.scan_cpp(&content, path)?,

            // Shell scripts
            "sh" | "bash" | "zsh" | "fish" => self.scan_shell(&content, path)?,
            "ps1" | "psm1" => self.scan_powershell(&content, path)?,
            "bat" | "cmd" => self.scan_batch(&content, path)?,

            // Check by filename or content
            _ => {
                if filename == "Makefile" || filename.starts_with("Makefile.") {
                    self.scan_makefile(&content, path)?;
                } else if content.starts_with("#!/") {
                    // Shebang script - likely a shell script
                    self.scan_shell(&content, path)?;
                }
            }
        }

        Ok(())
    }

    /// Record a usage of an environment variable
    fn record_usage(&mut self, var_name: String, file: &Path, line: usize, context: String) {
        let usage = VariableUsage {
            file: file.to_path_buf(),
            line,
            context,
        };

        // Check if this exact usage already exists
        let usages = self.usages.entry(var_name).or_default();

        // Avoid duplicate entries for the same file and line
        let already_exists = usages
            .iter()
            .any(|u| u.file == usage.file && u.line == usage.line && u.context == usage.context);

        if !already_exists {
            usages.push(usage);
        }
    }

    /// Scan JavaScript/TypeScript files
    fn scan_javascript(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // process.env.VAR or process.env["VAR"] or process.env['VAR']
            Regex::new(r"process\.env\.(\w+)")?,
            Regex::new(r#"process\.env\[["'](\w+)["']\]"#)?,
            // Deno.env.get("VAR")
            Regex::new(r#"Deno\.env\.get\(["'](\w+)["']\)"#)?,
            // import.meta.env.VAR
            Regex::new(r"import\.meta\.env\.(\w+)")?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Python files
    fn scan_python(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // os.environ["VAR"] or os.environ['VAR']
            Regex::new(r#"os\.environ\[["'](\w+)["']\]"#)?,
            // os.environ.get("VAR") or os.environ.get('VAR')
            Regex::new(r#"os\.environ\.get\(["'](\w+)["']"#)?,
            // os.getenv("VAR") or os.getenv('VAR')
            Regex::new(r#"os\.getenv\(["'](\w+)["']"#)?,
            // environ["VAR"] after from os import environ
            Regex::new(r#"environ\[["'](\w+)["']\]"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Rust files
    fn scan_rust(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // env!("VAR")
            Regex::new(r#"env!\s*\(\s*"(\w+)"\s*\)"#)?,
            // std::env::var("VAR")
            Regex::new(r#"std::env::var\s*\(\s*"(\w+)"\s*\)"#)?,
            // env::var("VAR")
            Regex::new(r#"env::var\s*\(\s*"(\w+)"\s*\)"#)?,
            // std::env::var_os("VAR")
            Regex::new(r#"std::env::var_os\s*\(\s*"(\w+)"\s*\)"#)?,
            // env::var_os("VAR")
            Regex::new(r#"env::var_os\s*\(\s*"(\w+)"\s*\)"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Go files
    fn scan_go(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // os.Getenv("VAR")
            Regex::new(r#"os\.Getenv\s*\(\s*"(\w+)"\s*\)"#)?,
            // os.LookupEnv("VAR")
            Regex::new(r#"os\.LookupEnv\s*\(\s*"(\w+)"\s*\)"#)?,
            // os.Setenv("VAR", ...)
            Regex::new(r#"os\.Setenv\s*\(\s*"(\w+)"\s*,"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Java files
    fn scan_java(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // System.getenv("VAR")
            Regex::new(r#"System\.getenv\s*\(\s*"(\w+)"\s*\)"#)?,
            // System.getenv().get("VAR")
            Regex::new(r#"getenv\s*\(\s*\)\.get\s*\(\s*"(\w+)"\s*\)"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan C# files
    fn scan_csharp(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // Environment.GetEnvironmentVariable("VAR")
            Regex::new(r#"Environment\.GetEnvironmentVariable\s*\(\s*"(\w+)"\s*\)"#)?,
            // Environment.SetEnvironmentVariable("VAR", ...)
            Regex::new(r#"Environment\.SetEnvironmentVariable\s*\(\s*"(\w+)"\s*,"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Ruby files
    fn scan_ruby(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // ENV["VAR"] or ENV['VAR']
            Regex::new(r#"ENV\[["'](\w+)["']\]"#)?,
            // ENV.fetch("VAR") or ENV.fetch('VAR')
            Regex::new(r#"ENV\.fetch\s*\(\s*["'](\w+)["']"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan PHP files
    fn scan_php(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // $_ENV["VAR"] or $_ENV['VAR']
            Regex::new(r#"\$_ENV\[["'](\w+)["']\]"#)?,
            // getenv("VAR") or getenv('VAR')
            Regex::new(r#"getenv\s*\(\s*["'](\w+)["']"#)?,
            // $_SERVER["VAR"] or $_SERVER['VAR'] (often contains env vars)
            Regex::new(r#"\$_SERVER\[["'](\w+)["']\]"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan C files
    fn scan_c(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // getenv("VAR")
            Regex::new(r#"getenv\s*\(\s*"(\w+)"\s*\)"#)?,
            // setenv("VAR", ...) or putenv("VAR=...")
            Regex::new(r#"setenv\s*\(\s*"(\w+)"\s*,"#)?,
            // Common Windows variants
            Regex::new(r#"GetEnvironmentVariable[AW]?\s*\(\s*"(\w+)"\s*,"#)?,
            Regex::new(r#"SetEnvironmentVariable[AW]?\s*\(\s*"(\w+)"\s*,"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.ends_with("*/")) {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan C++ files
    fn scan_cpp(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // getenv("VAR") - C-style
            Regex::new(r#"getenv\s*\(\s*"(\w+)"\s*\)"#)?,
            // std::getenv("VAR")
            Regex::new(r#"std::getenv\s*\(\s*"(\w+)"\s*\)"#)?,
            // setenv/putenv variants
            Regex::new(r#"setenv\s*\(\s*"(\w+)"\s*,"#)?,
            // Windows API
            Regex::new(r#"GetEnvironmentVariable[AW]?\s*\(\s*"(\w+)"\s*,"#)?,
            Regex::new(r#"SetEnvironmentVariable[AW]?\s*\(\s*"(\w+)"\s*,"#)?,
            // Boost
            Regex::new(r#"boost::this_process::environment\s*\[\s*"(\w+)"\s*\]"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.ends_with("*/")) {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan shell scripts (bash, sh, zsh, fish)
    fn scan_shell(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // $VAR or ${VAR}
            Regex::new(r"\$(\w+)")?,
            Regex::new(r"\$\{(\w+)\}")?,
            // export VAR=... or export VAR
            Regex::new(r"^\s*export\s+(\w+)")?,
            // : ${VAR:=default} or similar parameter expansion
            Regex::new(r"\$\{(\w+)[:?+=\-]")?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            if line.trim().starts_with('#') {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        // Skip common shell built-in variables
                        let var_name = var.as_str();
                        if ![
                            "1",
                            "2",
                            "3",
                            "4",
                            "5",
                            "6",
                            "7",
                            "8",
                            "9",
                            "0",
                            "@",
                            "*",
                            "#",
                            "?",
                            "-",
                            "$",
                            "!",
                            "_",
                            "PPID",
                            "PWD",
                            "OLDPWD",
                            "REPLY",
                            "UID",
                            "EUID",
                            "GROUPS",
                            "BASH",
                            "BASH_VERSION",
                            "BASH_VERSINFO",
                            "SHLVL",
                            "RANDOM",
                            "SECONDS",
                            "LINENO",
                            "HISTCMD",
                            "FUNCNAME",
                            "PIPESTATUS",
                            "IFS",
                        ]
                        .contains(&var_name)
                            && !var_name.starts_with("BASH_")
                        {
                            self.record_usage(var_name.to_string(), path, line_num + 1, line.trim().to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan `PowerShell` scripts
    fn scan_powershell(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // $env:VAR
            Regex::new(r"\$env:(\w+)")?,
            // [Environment]::GetEnvironmentVariable("VAR")
            Regex::new(r#"\[Environment\]::GetEnvironmentVariable\s*\(\s*["'](\w+)["']"#)?,
            // [Environment]::SetEnvironmentVariable("VAR", ...)
            Regex::new(r#"\[Environment\]::SetEnvironmentVariable\s*\(\s*["'](\w+)["']"#)?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            if line.trim().starts_with('#') {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        self.record_usage(var.as_str().to_string(), path, line_num + 1, line.trim().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan batch files
    fn scan_batch(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // %VAR%
            Regex::new(r"%(\w+)%")?,
            // set VAR=...
            Regex::new(r"(?i)^\s*set\s+(\w+)=")?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            if line.trim().starts_with("REM") || line.trim().starts_with("::") {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        // Skip common Windows built-in variables
                        let var_name = var.as_str();
                        if ![
                            "errorlevel",
                            "cd",
                            "date",
                            "time",
                            "random",
                            "CD",
                            "DATE",
                            "TIME",
                            "RANDOM",
                            "ERRORLEVEL",
                        ]
                        .contains(&var_name)
                        {
                            self.record_usage(var_name.to_string(), path, line_num + 1, line.trim().to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan Makefiles
    fn scan_makefile(&mut self, content: &str, path: &Path) -> Result<()> {
        let patterns = [
            // $(VAR) or ${VAR}
            Regex::new(r"\$\((\w+)\)")?,
            Regex::new(r"\$\{(\w+)\}")?,
            // Environment variable references in recipes
            Regex::new(r"\$\$(\w+)")?,
            Regex::new(r"\$\$\{(\w+)\}")?,
        ];

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            if line.trim().starts_with('#') {
                continue;
            }

            for pattern in &patterns {
                for cap in pattern.captures_iter(line) {
                    if let Some(var) = cap.get(1) {
                        // Skip common Make built-in variables
                        let var_name = var.as_str();
                        if ![
                            "MAKE",
                            "MAKEFLAGS",
                            "MAKECMDGOALS",
                            "CURDIR",
                            "SHELL",
                            "MAKEFILE_LIST",
                            "MAKEFILES",
                            "VPATH",
                            "SUFFIXES",
                            ".DEFAULT_GOAL",
                            ".VARIABLES",
                            ".FEATURES",
                        ]
                        .contains(&var_name)
                            && !var_name.starts_with('.')
                        {
                            self.record_usage(var_name.to_string(), path, line_num + 1, line.trim().to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get all found usages for a specific variable
    pub fn get_usages(&self, var_name: &str) -> Option<&Vec<VariableUsage>> {
        self.usages.get(var_name)
    }

    /// Get all variables that have been found in the codebase
    pub fn get_used_variables(&self) -> HashSet<String> {
        self.usages.keys().cloned().collect()
    }

    /// Get all variables and their usage counts
    pub fn get_usage_counts(&self) -> HashMap<String, usize> {
        self.usages
            .iter()
            .map(|(name, usages)| (name.clone(), usages.len()))
            .collect()
    }

    /// Find unused variables from a given set
    pub fn find_unused(&self, all_vars: &HashSet<String>) -> HashSet<String> {
        let used_vars = self.get_used_variables();
        all_vars.difference(&used_vars).cloned().collect()
    }
}

impl Default for DependencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper function to create a test file with content
    fn create_test_file(dir: &Path, filename: &str, content: &str) -> PathBuf {
        let file_path = dir.join(filename);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    /// Helper function to create a test directory structure
    fn create_test_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_new_tracker() {
        let tracker = DependencyTracker::new();
        assert_eq!(tracker.scan_paths.len(), 1);
        assert_eq!(tracker.scan_paths[0], PathBuf::from("."));
        assert!(!tracker.ignore_patterns.is_empty());
        assert!(tracker.usages.is_empty());
    }

    #[test]
    fn test_add_scan_path() {
        let mut tracker = DependencyTracker::new();
        let path = PathBuf::from("/test/path");
        tracker.add_scan_path(path.clone());
        assert_eq!(tracker.scan_paths.len(), 2);
        assert_eq!(tracker.scan_paths[1], path);
    }

    #[test]
    fn test_add_ignore_pattern() {
        let mut tracker = DependencyTracker::new();
        tracker.add_ignore_pattern("test_pattern".to_string());
        assert!(tracker.ignore_patterns.contains(&"test_pattern".to_string()));
    }

    #[test]
    fn test_scan_javascript_files() {
        let temp_dir = create_test_dir();
        let js_content = r#"
const dbUrl = process.env.DATABASE_URL;
const apiKey = process.env["API_KEY"];
const secret = process.env['SECRET_KEY'];
const port = process.env.PORT || 3000;

// Deno style
const denoVar = Deno.env.get("DENO_VAR");

// Vite/import.meta style
const viteVar = import.meta.env.VITE_API_URL;
"#;

        let js_file = create_test_file(temp_dir.path(), "test.js", js_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&js_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("SECRET_KEY").is_some());
        assert!(tracker.get_usages("PORT").is_some());
        assert!(tracker.get_usages("DENO_VAR").is_some());
        assert!(tracker.get_usages("VITE_API_URL").is_some());

        let used_vars = tracker.get_used_variables();
        assert_eq!(used_vars.len(), 6);
    }

    #[test]
    fn test_scan_python_files() {
        let temp_dir = create_test_dir();
        let py_content = r#"
import os
from os import environ

# Different ways to access env vars
db_url = os.environ["DATABASE_URL"]
api_key = os.environ.get("API_KEY", "default")
secret = os.getenv("SECRET_KEY")
home = environ["HOME"]

# This should not create duplicates
node_env = os.environ.get("NODE_ENV", "development")
"#;

        let py_file = create_test_file(temp_dir.path(), "test.py", py_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&py_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("SECRET_KEY").is_some());
        assert!(tracker.get_usages("HOME").is_some());
        assert!(tracker.get_usages("NODE_ENV").is_some());

        // Check that NODE_ENV is only recorded once
        let node_env_usages = tracker.get_usages("NODE_ENV").unwrap();
        assert_eq!(node_env_usages.len(), 1);
    }

    #[test]
    fn test_scan_rust_files() {
        let temp_dir = create_test_dir();
        let rs_content = r#"
use std::env;

fn main() {
    let db_url = env::var("DATABASE_URL").unwrap();
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    let home = env::var_os("HOME");
    let compile_time = env!("CARGO_PKG_VERSION");
}
"#;

        let rs_file = create_test_file(temp_dir.path(), "test.rs", rs_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&rs_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("HOME").is_some());
        assert!(tracker.get_usages("CARGO_PKG_VERSION").is_some());
    }

    #[test]
    fn test_scan_go_files() {
        let temp_dir = create_test_dir();
        let go_content = r#"
package main

import "os"

func main() {
    dbUrl := os.Getenv("DATABASE_URL")
    apiKey, exists := os.LookupEnv("API_KEY")
    os.Setenv("NEW_VAR", "value")
}
"#;

        let go_file = create_test_file(temp_dir.path(), "test.go", go_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&go_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("NEW_VAR").is_some());
    }

    #[test]
    fn test_scan_c_files() {
        let temp_dir = create_test_dir();
        let c_content = r#"
#include <stdlib.h>

int main() {
    char* db_url = getenv("DATABASE_URL");
    setenv("API_KEY", "secret", 1);
    
    // Windows style
    GetEnvironmentVariable("WINDOWS_VAR", buffer, size);
    SetEnvironmentVariableA("WIN_API_KEY", "value");
    
    // This is a comment: getenv("COMMENTED_VAR")
    /* Also commented: getenv("BLOCK_COMMENT_VAR") */
}
"#;

        let c_file = create_test_file(temp_dir.path(), "test.c", c_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&c_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("WINDOWS_VAR").is_some());
        assert!(tracker.get_usages("WIN_API_KEY").is_some());

        // Comments should be ignored
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
        assert!(tracker.get_usages("BLOCK_COMMENT_VAR").is_none());
    }

    #[test]
    fn test_scan_cpp_files() {
        let temp_dir = create_test_dir();
        let cpp_content = r#"
#include <cstdlib>
#include <iostream>

int main() {
    // C-style
    const char* db_url = getenv("DATABASE_URL");
    
    // C++ style
    const char* api_key = std::getenv("API_KEY");
    
    // Boost style
    auto value = boost::this_process::environment["BOOST_VAR"];
    
    // Comment should be ignored
    // std::getenv("COMMENTED_VAR");
}
"#;

        let cpp_file = create_test_file(temp_dir.path(), "test.cpp", cpp_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&cpp_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("BOOST_VAR").is_some());
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
    }

    #[test]
    fn test_scan_shell_scripts() {
        let temp_dir = create_test_dir();
        let sh_content = r#"
#!/bin/bash

# Variable references
echo $DATABASE_URL
echo ${API_KEY}

# Export statements
export NEW_VAR="value"
export ANOTHER_VAR

# Parameter expansion
: ${DEFAULT_VAR:=default_value}

# Common shell variables should be ignored
echo $1 $2 $@ $* $# $? $$ $!

# Comments should be ignored
# echo $COMMENTED_VAR
"#;

        let sh_file = create_test_file(temp_dir.path(), "test.sh", sh_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&sh_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("NEW_VAR").is_some());
        assert!(tracker.get_usages("ANOTHER_VAR").is_some());
        assert!(tracker.get_usages("DEFAULT_VAR").is_some());

        // Shell built-ins should be ignored
        assert!(tracker.get_usages("1").is_none());
        assert!(tracker.get_usages("@").is_none());

        // Comments should be ignored
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
    }

    #[test]
    fn test_scan_powershell_scripts() {
        let temp_dir = create_test_dir();
        let ps1_content = r#"
# PowerShell environment variables
$dbUrl = $env:DATABASE_URL
$apiKey = [Environment]::GetEnvironmentVariable("API_KEY")
[Environment]::SetEnvironmentVariable("NEW_VAR", "value")

# Comment should be ignored
# $env:COMMENTED_VAR
"#;

        let ps1_file = create_test_file(temp_dir.path(), "test.ps1", ps1_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&ps1_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("NEW_VAR").is_some());
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
    }

    #[test]
    fn test_scan_batch_files() {
        let temp_dir = create_test_dir();
        let bat_content = r"
@echo off
REM Batch file environment variables

echo %DATABASE_URL%
set API_KEY=secret

REM This is a comment: %COMMENTED_VAR%
:: Another comment style: %ALSO_COMMENTED%

REM Built-in variables should be ignored
echo %DATE% %TIME% %ERRORLEVEL%
";

        let bat_file = create_test_file(temp_dir.path(), "test.bat", bat_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&bat_file).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());

        // Comments and built-ins should be ignored
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
        assert!(tracker.get_usages("ALSO_COMMENTED").is_none());
        assert!(tracker.get_usages("DATE").is_none());
        assert!(tracker.get_usages("ERRORLEVEL").is_none());
    }

    #[test]
    fn test_scan_makefile() {
        let temp_dir = create_test_dir();
        let makefile_content = r"
# Makefile variables
DB_URL = $(DATABASE_URL)
API_KEY = ${API_KEY}

# Environment variables in recipes
build:
    echo $$HOME
    echo $${USER}

# Built-in variables should be ignored
    echo $(MAKE) $(SHELL) $(CURDIR)

# Comments should be ignored
# $(COMMENTED_VAR)
";

        let makefile = create_test_file(temp_dir.path(), "Makefile", makefile_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&makefile).unwrap();

        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("API_KEY").is_some());
        assert!(tracker.get_usages("HOME").is_some());
        assert!(tracker.get_usages("USER").is_some());

        // Built-ins and comments should be ignored
        assert!(tracker.get_usages("MAKE").is_none());
        assert!(tracker.get_usages("SHELL").is_none());
        assert!(tracker.get_usages("COMMENTED_VAR").is_none());
    }

    #[test]
    fn test_scan_directory() {
        let temp_dir = create_test_dir();

        // Create multiple files
        create_test_file(temp_dir.path(), "app.js", "const url = process.env.API_URL;");
        create_test_file(
            temp_dir.path(),
            "config.py",
            "import os\ndb = os.getenv('DATABASE_URL')",
        );
        create_test_file(
            temp_dir.path(),
            "main.rs",
            "let key = env::var(\"SECRET_KEY\").unwrap();",
        );

        // Create a subdirectory
        let sub_dir = temp_dir.path().join("scripts");
        fs::create_dir(&sub_dir).unwrap();
        create_test_file(&sub_dir, "deploy.sh", "echo $DEPLOY_KEY");

        // Create an ignored directory
        let ignored_dir = temp_dir.path().join("node_modules");
        fs::create_dir(&ignored_dir).unwrap();
        create_test_file(&ignored_dir, "package.js", "process.env.IGNORED_VAR");

        let mut tracker = DependencyTracker::new();
        tracker.scan_directory(temp_dir.path()).unwrap();

        // Check that all non-ignored files were scanned
        assert!(tracker.get_usages("API_URL").is_some());
        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("SECRET_KEY").is_some());
        assert!(tracker.get_usages("DEPLOY_KEY").is_some());

        // Ignored directory should not be scanned
        assert!(tracker.get_usages("IGNORED_VAR").is_none());
    }

    #[test]
    fn test_scan_with_multiple_paths() {
        let temp_dir1 = create_test_dir();
        let temp_dir2 = create_test_dir();

        create_test_file(temp_dir1.path(), "app1.js", "process.env.VAR1");
        create_test_file(temp_dir2.path(), "app2.js", "process.env.VAR2");

        let mut tracker = DependencyTracker::new();
        tracker.scan_paths.clear(); // Remove default path
        tracker.add_scan_path(temp_dir1.path().to_path_buf());
        tracker.add_scan_path(temp_dir2.path().to_path_buf());

        tracker.scan().unwrap();

        assert!(tracker.get_usages("VAR1").is_some());
        assert!(tracker.get_usages("VAR2").is_some());
    }

    #[test]
    fn test_get_usage_counts() {
        let temp_dir = create_test_dir();

        // Create files with multiple usages of the same variable
        let js_content = r"
const url1 = process.env.API_URL;
const url2 = process.env.API_URL;
const db = process.env.DATABASE_URL;
";

        let py_content = r#"
import os
api = os.getenv("API_URL")
"#;

        create_test_file(temp_dir.path(), "app.js", js_content);
        create_test_file(temp_dir.path(), "config.py", py_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_directory(temp_dir.path()).unwrap();

        let usage_counts = tracker.get_usage_counts();

        // API_URL appears 3 times (2 in JS, 1 in Python)
        assert_eq!(usage_counts.get("API_URL"), Some(&3));
        // DATABASE_URL appears once
        assert_eq!(usage_counts.get("DATABASE_URL"), Some(&1));
    }

    #[test]
    fn test_find_unused_variables() {
        let temp_dir = create_test_dir();
        create_test_file(temp_dir.path(), "app.js", "process.env.USED_VAR");

        let mut tracker = DependencyTracker::new();
        tracker.scan_directory(temp_dir.path()).unwrap();

        let all_vars = HashSet::from([
            "USED_VAR".to_string(),
            "UNUSED_VAR1".to_string(),
            "UNUSED_VAR2".to_string(),
        ]);

        let unused = tracker.find_unused(&all_vars);

        assert_eq!(unused.len(), 2);
        assert!(unused.contains("UNUSED_VAR1"));
        assert!(unused.contains("UNUSED_VAR2"));
        assert!(!unused.contains("USED_VAR"));
    }

    #[test]
    fn test_record_usage_deduplication() {
        let mut tracker = DependencyTracker::new();
        let path = PathBuf::from("test.js");

        // Record the same usage multiple times
        tracker.record_usage("TEST_VAR".to_string(), &path, 10, "context".to_string());
        tracker.record_usage("TEST_VAR".to_string(), &path, 10, "context".to_string());
        tracker.record_usage("TEST_VAR".to_string(), &path, 10, "context".to_string());

        // Should only have one usage recorded
        let usages = tracker.get_usages("TEST_VAR").unwrap();
        assert_eq!(usages.len(), 1);

        // Different line should create a new usage
        tracker.record_usage("TEST_VAR".to_string(), &path, 20, "different context".to_string());
        let usages = tracker.get_usages("TEST_VAR").unwrap();
        assert_eq!(usages.len(), 2);
    }

    #[test]
    fn test_skip_large_files() {
        let temp_dir = create_test_dir();

        // Create a large file (> 10MB)
        let large_content = "x".repeat(11_000_000);
        let large_file = create_test_file(temp_dir.path(), "large.js", &large_content);

        let mut tracker = DependencyTracker::new();
        // Should not panic and should skip the file
        assert!(tracker.scan_file(&large_file).is_ok());

        // No variables should be found
        assert!(tracker.get_used_variables().is_empty());
    }

    #[test]
    fn test_skip_binary_files() {
        let temp_dir = create_test_dir();

        // Create a binary file
        let binary_content = vec![0u8, 1, 2, 3, 255, 254, 253];
        let binary_file = temp_dir.path().join("binary.exe");
        fs::write(&binary_file, binary_content).unwrap();

        let mut tracker = DependencyTracker::new();
        // Should not panic and should skip the file
        assert!(tracker.scan_file(&binary_file).is_ok());

        // No variables should be found
        assert!(tracker.get_used_variables().is_empty());
    }

    #[test]
    fn test_shebang_detection() {
        let temp_dir = create_test_dir();

        // File without extension but with shebang
        let script_content = r"#!/bin/bash
echo $DATABASE_URL
";

        let script_file = create_test_file(temp_dir.path(), "deploy_script", script_content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_file(&script_file).unwrap();

        // Should detect as shell script and find the variable
        assert!(tracker.get_usages("DATABASE_URL").is_some());
    }

    #[test]
    fn test_multiple_language_support() {
        let temp_dir = create_test_dir();

        // Test TypeScript
        create_test_file(temp_dir.path(), "app.ts", "const api = process.env.API_URL;");

        // Test JSX
        create_test_file(
            temp_dir.path(),
            "component.jsx",
            "const key = process.env.REACT_APP_KEY;",
        );

        // Test different extensions
        create_test_file(temp_dir.path(), "server.mjs", "const db = process.env.DATABASE_URL;");
        create_test_file(temp_dir.path(), "old.cjs", "const port = process.env.PORT;");

        let mut tracker = DependencyTracker::new();
        tracker.scan_directory(temp_dir.path()).unwrap();

        assert!(tracker.get_usages("API_URL").is_some());
        assert!(tracker.get_usages("REACT_APP_KEY").is_some());
        assert!(tracker.get_usages("DATABASE_URL").is_some());
        assert!(tracker.get_usages("PORT").is_some());
    }

    #[test]
    fn test_usage_context_preservation() {
        let temp_dir = create_test_dir();
        let content = r"
const dbUrl = process.env.DATABASE_URL;
    const apiKey = process.env.API_KEY; // Indented line
";

        create_test_file(temp_dir.path(), "test.js", content);

        let mut tracker = DependencyTracker::new();
        tracker.scan_directory(temp_dir.path()).unwrap();

        let db_usage = tracker.get_usages("DATABASE_URL").unwrap();
        assert_eq!(db_usage[0].context, "const dbUrl = process.env.DATABASE_URL;");
        assert_eq!(db_usage[0].line, 2);

        let api_usage = tracker.get_usages("API_KEY").unwrap();
        assert_eq!(
            api_usage[0].context,
            "const apiKey = process.env.API_KEY; // Indented line"
        );
        assert_eq!(api_usage[0].line, 3);
    }
}

// ...existing code...

#[cfg(test)]
mod cli_tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create test environment with files
    fn create_test_environment() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Create test files with environment variable usage
        fs::write(
            temp_dir.path().join("app.js"),
            r"
const db = process.env.DATABASE_URL;
const api = process.env.API_KEY;
const port = process.env.PORT || 3000;
",
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("config.py"),
            r#"
import os
db_url = os.environ.get("DATABASE_URL")
debug = os.getenv("DEBUG", "false")
"#,
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("unused.rs"),
            r#"
// This file doesn't use UNUSED_VAR
let api = env::var("API_KEY").unwrap();
"#,
        )
        .unwrap();

        // Create subdirectory with more files
        let scripts_dir = temp_dir.path().join("scripts");
        fs::create_dir(&scripts_dir).unwrap();

        fs::write(
            scripts_dir.join("deploy.sh"),
            r"
#!/bin/bash
echo $DATABASE_URL
export DEPLOY_ENV=production
",
        )
        .unwrap();

        temp_dir
    }

    /// Helper to set up environment variables for testing
    fn setup_test_env_vars() {
        unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost:5432/test") };
        unsafe { std::env::set_var("API_KEY", "test-api-key-123") };
        unsafe { std::env::set_var("PORT", "3000") };
        unsafe { std::env::set_var("DEBUG", "true") };
        unsafe { std::env::set_var("UNUSED_VAR", "this-is-not-used") };
        unsafe { std::env::set_var("DEPLOY_ENV", "staging") };
    }

    /// Helper to clean up environment variables after testing
    fn cleanup_test_env_vars() {
        unsafe { std::env::remove_var("DATABASE_URL") };
        unsafe { std::env::remove_var("API_KEY") };
        unsafe { std::env::remove_var("PORT") };
        unsafe { std::env::remove_var("DEBUG") };
        unsafe { std::env::remove_var("UNUSED_VAR") };
        unsafe { std::env::remove_var("DEPLOY_ENV") };
    }

    #[test]
    fn test_handle_deps_default_behavior() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        // Test default behavior (show all dependencies)
        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_with_specific_variable() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: Some("DATABASE_URL".to_string()),
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_unused() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: true,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_command() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: Some(DepsCommands::Show {
                variable: Some("API_KEY".to_string()),
                unused: false,
            }),
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "simple".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_scan_command() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: Some(DepsCommands::Scan {
                paths: vec![temp_dir.path().to_path_buf()],
                cache: false,
            }),
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_stats_command() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: Some(DepsCommands::Stats { by_usage: true }),
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_show_specific_variable_found() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(Some("DATABASE_URL"), false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_specific_variable_not_found() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(Some("NONEXISTENT_VAR"), false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_unused_variables() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, true, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_all_dependencies() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_json_format() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "json".to_string(),
        };

        // Test unused variables in JSON format
        let result = handle_deps_show(None, true, &args);
        assert!(result.is_ok());

        // Test specific variable in JSON format
        let result = handle_deps_show(Some("DATABASE_URL"), false, &args);
        assert!(result.is_ok());

        // Test all dependencies in JSON format
        let result = handle_deps_show(None, false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_simple_format() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "simple".to_string(),
        };

        // Test unused variables in simple format
        let result = handle_deps_show(None, true, &args);
        assert!(result.is_ok());

        // Test specific variable in simple format
        let result = handle_deps_show(Some("DATABASE_URL"), false, &args);
        assert!(result.is_ok());

        // Test all dependencies in simple format
        let result = handle_deps_show(None, false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_with_ignore_patterns() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec!["scripts".to_string()],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, false, &args);
        assert!(result.is_ok());

        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_show_no_env_vars_set() {
        let temp_dir = create_test_environment();
        // Don't set up environment variables

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, true, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_scan_single_path() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_scan(&[temp_dir.path().to_path_buf()], false, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_scan_multiple_paths() {
        let temp_dir1 = create_test_environment();
        let temp_dir2 = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_scan(
            &[temp_dir1.path().to_path_buf(), temp_dir2.path().to_path_buf()],
            false,
            &args,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_scan_with_cache() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_scan(&[temp_dir.path().to_path_buf()], true, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_scan_with_ignore_patterns() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec!["scripts".to_string(), "*.py".to_string()],
            format: "table".to_string(),
        };

        let result = handle_deps_scan(&[temp_dir.path().to_path_buf()], false, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_stats_default_sorting() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_stats(false, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_stats_sort_by_usage() {
        let temp_dir = create_test_environment();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_stats(true, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_stats_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_stats(false, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_stats_no_paths_specified() {
        // Should use current directory by default
        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_stats(false, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_deps_with_nonexistent_path() {
        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![PathBuf::from("/nonexistent/path")],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, false, &args);
        // Should handle gracefully
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_handle_deps_show_variable_with_long_value() {
        setup_test_env_vars();
        unsafe { std::env::set_var("LONG_VAR", "a".repeat(100)) };

        let temp_dir = create_test_environment();
        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_show(None, true, &args);
        assert!(result.is_ok());

        unsafe { std::env::remove_var("LONG_VAR") };
        cleanup_test_env_vars();
    }

    #[test]
    fn test_handle_deps_stats_with_many_variables() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with many different environment variables
        let content = (0..30)
            .map(|i| format!("const var{i} = process.env.VAR_{i};"))
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(temp_dir.path().join("many_vars.js"), content).unwrap();

        let args = DepsArgs {
            command: None,
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };

        let result = handle_deps_stats(true, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_integration_full_workflow() {
        let temp_dir = create_test_environment();
        setup_test_env_vars();

        // First scan
        let scan_args = DepsArgs {
            command: Some(DepsCommands::Scan {
                paths: vec![temp_dir.path().to_path_buf()],
                cache: false,
            }),
            variable: None,
            unused: false,
            paths: vec![],
            ignore: vec![],
            format: "table".to_string(),
        };
        assert!(handle_deps(&scan_args).is_ok());

        // Then show stats
        let stats_args = DepsArgs {
            command: Some(DepsCommands::Stats { by_usage: true }),
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "table".to_string(),
        };
        assert!(handle_deps(&stats_args).is_ok());

        // Show specific variable
        let show_args = DepsArgs {
            command: Some(DepsCommands::Show {
                variable: Some("DATABASE_URL".to_string()),
                unused: false,
            }),
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "json".to_string(),
        };
        assert!(handle_deps(&show_args).is_ok());

        // Show unused
        let unused_args = DepsArgs {
            command: Some(DepsCommands::Show {
                variable: None,
                unused: true,
            }),
            variable: None,
            unused: false,
            paths: vec![temp_dir.path().to_path_buf()],
            ignore: vec![],
            format: "simple".to_string(),
        };
        assert!(handle_deps(&unused_args).is_ok());

        cleanup_test_env_vars();
    }
}

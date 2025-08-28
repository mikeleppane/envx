use clap::Parser;
use color_eyre::Report;
use color_eyre::Result;
use envx_cli::Cli;
use std::process;

#[cfg(not(windows))]
use jemallocator::Jemalloc;
#[cfg(windows)]
use mimalloc::MiMalloc;

#[cfg(windows)]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(not(windows))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
    color_eyre::install()?;

    let cli = Cli::parse();

    if let Err(error) = envx_cli::execute(cli) {
        handle_error(&error);
    }

    Ok(())
}

fn handle_error(error: &Report) {
    use std::io::Write;
    let stderr = &mut std::io::stderr();

    // Print a user-friendly error header
    let _ = writeln!(
        stderr,
        "\n{} An error occurred while running envx\n",
        styled_error_prefix()
    );

    // Check for common error types and provide helpful messages
    let error_string = error.to_string();
    let root_cause = error.root_cause().to_string();

    // Provide context-specific help messages
    if root_cause.contains("permission denied") || root_cause.contains("access denied") {
        let _ = writeln!(stderr, "  {} Permission denied", styled_bullet());
        let _ = writeln!(stderr, "  This error typically occurs when:");
        let _ = writeln!(
            stderr,
            "  • You don't have permission to modify system environment variables"
        );
        let _ = writeln!(stderr, "  • The configuration directory is not accessible");
        let _ = writeln!(
            stderr,
            "\n  {} Try running with elevated privileges or check file permissions",
            styled_hint()
        );
    } else if root_cause.contains("not found") {
        if root_cause.contains("Profile") {
            let _ = writeln!(stderr, "  {} Profile not found", styled_bullet());
            let _ = writeln!(stderr, "  The specified profile doesn't exist.");
            let _ = writeln!(
                stderr,
                "\n  {} List available profiles with: envx profile list",
                styled_hint()
            );
        } else if root_cause.contains("Snapshot") {
            let _ = writeln!(stderr, "  {} Snapshot not found", styled_bullet());
            let _ = writeln!(stderr, "  The specified snapshot doesn't exist.");
            let _ = writeln!(
                stderr,
                "\n  {} List available snapshots with: envx snapshot list",
                styled_hint()
            );
        } else if root_cause.contains("Variable") || root_cause.contains("VAR") {
            let _ = writeln!(stderr, "  {} Environment variable not found", styled_bullet());
            let _ = writeln!(stderr, "\n  {} List all variables with: envx list", styled_hint());
        } else {
            let _ = writeln!(stderr, "  {} Resource not found", styled_bullet());
            let _ = writeln!(stderr, "  {root_cause}");
        }
    } else if root_cause.contains("already exists") {
        let _ = writeln!(stderr, "  {} Resource already exists", styled_bullet());
        let _ = writeln!(stderr, "  {root_cause}");
        let _ = writeln!(
            stderr,
            "\n  {} Use --force or --overwrite flag to replace existing resources",
            styled_hint()
        );
    } else if error_string.contains("config") || error_string.contains("yaml") {
        let _ = writeln!(stderr, "  {} Configuration error", styled_bullet());
        let _ = writeln!(stderr, "  There was a problem with the configuration file.");
        let _ = writeln!(stderr, "\n  {} Check your .envx/config.yaml syntax", styled_hint());
    } else {
        // Generic error display
        let _ = writeln!(stderr, "  {} {}", styled_bullet(), root_cause);
    }

    // Add debug information if verbose
    if std::env::var("RUST_BACKTRACE").is_ok() || std::env::var("ENVX_DEBUG").is_ok() {
        let _ = writeln!(stderr, "\n{}", styled_section_header("Debug Information"));
        let _ = writeln!(stderr, "{error:?}");
    } else {
        let _ = writeln!(
            stderr,
            "\n  {} Run with RUST_BACKTRACE=1 for more details",
            styled_info()
        );
    }

    let _ = writeln!(stderr);

    // Exit with error code
    process::exit(1);
}

// Styling functions for pretty output
fn styled_error_prefix() -> String {
    if supports_color() {
        "\x1b[31m❌\x1b[0m".to_string()
    } else {
        "ERROR:".to_string()
    }
}

fn styled_bullet() -> String {
    if supports_color() {
        "\x1b[31m•\x1b[0m".to_string()
    } else {
        "•".to_string()
    }
}

fn styled_hint() -> String {
    if supports_color() {
        "\x1b[32m💡 Hint:\x1b[0m".to_string()
    } else {
        "Hint:".to_string()
    }
}

fn styled_info() -> String {
    if supports_color() {
        "\x1b[36mℹ️  Info:\x1b[0m".to_string()
    } else {
        "Info:".to_string()
    }
}

fn styled_section_header(text: &str) -> String {
    if supports_color() {
        format!("\x1b[33m=== {text} ===\x1b[0m")
    } else {
        format!("=== {text} ===")
    }
}

fn supports_color() -> bool {
    // Check if we're in a terminal that supports color
    #[cfg(windows)]
    {
        std::env::var("TERM").is_ok() || std::env::var("WT_SESSION").is_ok()
    }
    #[cfg(not(windows))]
    {
        std::env::var("TERM").unwrap_or_default() != "dumb"
    }
}

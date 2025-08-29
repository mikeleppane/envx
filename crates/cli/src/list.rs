use color_eyre::Result;
use color_eyre::eyre::eyre;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use console::{Term, style};
use envx_core::EnvVarManager;

/// Handles the list command to display environment variables with various formatting options.
///
/// # Arguments
/// * `source` - Optional filter by source (system, user, process, shell)
/// * `query` - Optional search query to filter variables
/// * `format` - Output format (table, json, simple, compact)
/// * `sort` - Sort order (name, value, source)
/// * `names_only` - Whether to display only variable names
/// * `limit` - Optional limit on number of variables to display
/// * `stats` - Whether to display statistics
///
/// # Errors
/// Returns an error if:
/// - Environment variables cannot be loaded
/// - Invalid source type is provided
/// - JSON serialization fails
pub fn handle_list_command(
    source: Option<&str>,
    query: Option<&str>,
    format: &str,
    sort: &str,
    names_only: bool,
    limit: Option<usize>,
    stats: bool,
) -> Result<()> {
    let mut manager = EnvVarManager::new();
    manager.load_all()?;

    // Get filtered variables
    let mut vars = if let Some(q) = &query {
        manager.search(q)
    } else if let Some(src) = source {
        let source_filter = match src {
            "system" => envx_core::EnvVarSource::System,
            "user" => envx_core::EnvVarSource::User,
            "process" => envx_core::EnvVarSource::Process,
            "shell" => envx_core::EnvVarSource::Shell,
            _ => return Err(eyre!("Invalid source: {}", src)),
        };
        manager.filter_by_source(&source_filter)
    } else {
        manager.list()
    };

    // Sort variables
    match sort {
        "name" => vars.sort_by(|a, b| a.name.cmp(&b.name)),
        "value" => vars.sort_by(|a, b| a.value.cmp(&b.value)),
        "source" => vars.sort_by(|a, b| format!("{:?}", a.source).cmp(&format!("{:?}", b.source))),
        _ => {}
    }

    // Apply limit if specified
    let total_count = vars.len();
    if let Some(lim) = limit {
        vars.truncate(lim);
    }

    // Show statistics if requested
    if stats || (format == "table" && !names_only) {
        print_statistics(&manager, &vars, total_count, query, source);
    }

    // Handle names_only flag
    if names_only {
        for var in vars {
            println!("{}", var.name);
        }
        return Ok(());
    }

    // Format output
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&vars)?);
        }
        "simple" => {
            for var in vars {
                println!("{} = {}", style(&var.name).cyan(), var.value);
            }
        }
        "compact" => {
            for var in vars {
                let source_str = format_source_compact(&var.source);
                println!(
                    "{} {} = {}",
                    source_str,
                    style(&var.name).bright(),
                    style(truncate_value(&var.value, 60)).dim()
                );
            }
        }
        _ => {
            print_table(vars, limit.is_some());
        }
    }

    // Show limit notice
    if let Some(lim) = limit {
        if total_count > lim {
            println!(
                "\n{}",
                style(format!(
                    "Showing {lim} of {total_count} total variables. Use --limit to see more."
                ))
                .yellow()
            );
        }
    }

    Ok(())
}

fn print_statistics(
    manager: &EnvVarManager,
    filtered_vars: &[&envx_core::EnvVar],
    total_count: usize,
    query: Option<&str>,
    source: Option<&str>,
) {
    let _term = Term::stdout();

    // Count by source
    let system_count = manager.filter_by_source(&envx_core::EnvVarSource::System).len();
    let user_count = manager.filter_by_source(&envx_core::EnvVarSource::User).len();
    let process_count = manager.filter_by_source(&envx_core::EnvVarSource::Process).len();
    let shell_count = manager.filter_by_source(&envx_core::EnvVarSource::Shell).len();

    // Header
    println!("{}", style("═".repeat(60)).blue().bold());
    println!("{}", style("Environment Variables Summary").cyan().bold());
    println!("{}", style("═".repeat(60)).blue().bold());

    // Filter info
    if query.is_some() || source.is_some() {
        print!("  {} ", style("Filter:").yellow());
        if let Some(q) = query {
            print!("query='{}' ", style(q).green());
        }
        if let Some(s) = source {
            print!("source={} ", style(s).green());
        }
        println!();
        println!(
            "  {} {}/{} variables",
            style("Showing:").yellow(),
            style(filtered_vars.len()).green().bold(),
            total_count
        );
    } else {
        println!(
            "  {} {} variables",
            style("Total:").yellow(),
            style(total_count).green().bold()
        );
    }

    println!();
    println!("  {} By Source:", style("►").cyan());

    // Source breakdown with visual bars
    let max_count = system_count.max(user_count).max(process_count).max(shell_count);
    let bar_width = 30;

    print_source_bar("System", system_count, max_count, bar_width, "red");
    print_source_bar("User", user_count, max_count, bar_width, "yellow");
    print_source_bar("Process", process_count, max_count, bar_width, "green");
    print_source_bar("Shell", shell_count, max_count, bar_width, "cyan");

    println!("{}", style("─".repeat(60)).blue());
    println!();
}

fn print_source_bar(label: &str, count: usize, max: usize, width: usize, color: &str) {
    let filled = if max > 0 { (count * width / max).max(1) } else { 0 };

    let bar = "█".repeat(filled);
    let empty = "░".repeat(width - filled);

    let colored_bar = match color {
        "red" => style(bar).red(),
        "yellow" => style(bar).yellow(),
        "green" => style(bar).green(),
        "cyan" => style(bar).cyan(),
        _ => style(bar).white(),
    };

    println!(
        "    {:10} {} {}{} {}",
        style(label).bold(),
        colored_bar,
        style(empty).dim(),
        style(format!(" {count:4}")).bold(),
        style("vars").dim()
    );
}

fn print_table(vars: Vec<&envx_core::EnvVar>, _is_limited: bool) {
    if vars.is_empty() {
        println!("{}", style("No environment variables found.").yellow());
    }

    let mut table = Table::new();

    // Configure table style
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(120)
        .set_header(vec![
            Cell::new("Source").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Name").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Value").add_attribute(Attribute::Bold).fg(Color::Cyan),
        ]);

    // Add rows with colored source indicators
    for var in vars {
        let (source_str, source_color) = format_source(&var.source);
        let truncated_value = truncate_value(&var.value, 50);

        table.add_row(vec![
            Cell::new(source_str).fg(source_color),
            Cell::new(&var.name).fg(Color::White),
            Cell::new(truncated_value).fg(Color::Grey),
        ]);
    }

    println!("{table}");
}

fn format_source(source: &envx_core::EnvVarSource) -> (String, Color) {
    match source {
        envx_core::EnvVarSource::System => ("System".to_string(), Color::Red),
        envx_core::EnvVarSource::User => ("User".to_string(), Color::Yellow),
        envx_core::EnvVarSource::Process => ("Process".to_string(), Color::Green),
        envx_core::EnvVarSource::Shell => ("Shell".to_string(), Color::Cyan),
        envx_core::EnvVarSource::Application(app) => (format!("App:{app}"), Color::Magenta),
    }
}

fn format_source_compact(source: &envx_core::EnvVarSource) -> console::StyledObject<String> {
    match source {
        envx_core::EnvVarSource::System => style("[SYS]".to_string()).red().bold(),
        envx_core::EnvVarSource::User => style("[USR]".to_string()).yellow().bold(),
        envx_core::EnvVarSource::Process => style("[PRC]".to_string()).green().bold(),
        envx_core::EnvVarSource::Shell => style("[SHL]".to_string()).cyan().bold(),
        envx_core::EnvVarSource::Application(app) => style(format!("[{}]", &app[..3.min(app.len())].to_uppercase()))
            .magenta()
            .bold(),
    }
}

fn truncate_value(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_string()
    } else {
        format!("{}...", &value[..max_len - 3])
    }
}

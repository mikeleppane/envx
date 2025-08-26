use crate::app::{App, ConfirmAction, EditField, Mode};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

// Version constant - you can also get this from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    draw_header(f, chunks[0], app);
    draw_main_content(f, chunks[1], app);
    draw_status_bar(f, chunks[2], app);

    // Draw overlays based on mode
    match &app.mode {
        Mode::Edit | Mode::Add => draw_edit_dialog(f, app),
        Mode::Confirm(action) => draw_confirm_dialog(f, action),
        Mode::View(var_name) => draw_view_dialog(f, app, var_name),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),     // Title
            Constraint::Length(15), // Version
        ])
        .split(area);

    // Main title
    let title_text = match &app.mode {
        Mode::Search => vec![
            Span::styled("envx ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                app.search_input.value(),
                Style::default().fg(Color::White).add_modifier(Modifier::ITALIC),
            ),
        ],
        _ => vec![
            Span::styled("envx ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Environment Variable Manager", Style::default().fg(Color::White)),
        ],
    };

    let title = Paragraph::new(Line::from(title_text)).block(
        Block::default()
            .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(title, header_chunks[0]);

    // Version info
    let version_text = vec![
        Span::styled("v", Style::default().fg(Color::DarkGray)),
        Span::styled(VERSION, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ];

    let version = Paragraph::new(Line::from(version_text))
        .alignment(Alignment::Right)
        .block(
            Block::default()
                .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(version, header_chunks[1]);
}

#[allow(clippy::too_many_lines)]
fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),     // Keybindings
            Constraint::Length(20), // Info section
        ])
        .split(area);

    // Keybindings with color coding
    let keybindings = match &app.mode {
        Mode::Normal => vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("jk", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Navigate ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("v", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" View ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(" Search ", Style::default().fg(Color::DarkGray)),
            Span::styled("a", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::styled(" Add ", Style::default().fg(Color::DarkGray)),
            Span::styled("e", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::styled(" Edit ", Style::default().fg(Color::DarkGray)),
            Span::styled("d", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Delete ", Style::default().fg(Color::DarkGray)),
            Span::styled("r", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Refresh ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit", Style::default().fg(Color::DarkGray)),
        ],
        Mode::Search => vec![
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Apply", Style::default().fg(Color::DarkGray)),
        ],
        Mode::Edit | Mode::Add => vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Switch Field ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Ctrl+Enter",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Save ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
        ],
        Mode::Confirm(_) => vec![
            Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Yes ", Style::default().fg(Color::DarkGray)),
            Span::styled("n", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" No ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
        ],
        Mode::View(_) => vec![
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back to list", Style::default().fg(Color::DarkGray)),
        ],
    };

    // Add status message if present
    let mut left_content = vec![Line::from(keybindings)];
    if let Some((message, _)) = &app.status_message {
        left_content.push(Line::from(vec![
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                message,
                Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    let keybindings_widget = Paragraph::new(left_content).block(
        Block::default()
            .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(keybindings_widget, status_chunks[0]);

    // Right info section
    let info_content = if app.filtered_vars.is_empty() {
        vec![Span::styled(
            "No items",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )]
    } else {
        vec![
            Span::styled("Item ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", app.selected_index + 1),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" of ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", app.filtered_vars.len()),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]
    };

    let info_widget = Paragraph::new(Line::from(info_content))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(info_widget, status_chunks[1]);
}

// ... rest of the functions remain the same ...

fn draw_main_content(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().borders(Borders::ALL).title(format!(
        "Environment Variables ({}/{})",
        if app.filtered_vars.is_empty() {
            0
        } else {
            app.selected_index + 1
        },
        app.filtered_vars.len()
    ));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Calculate visible height
    let visible_height = inner_area.height as usize;

    // Update scroll offset based on selection
    app.calculate_scroll(visible_height);

    // Calculate the range of items to display
    let end_index = std::cmp::min(app.scroll_offset + visible_height, app.filtered_vars.len());

    // Create list items for only the visible range
    let items: Vec<ListItem> = app
        .filtered_vars
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(end_index - app.scroll_offset)
        .map(|(i, var)| {
            let style = if i == app.selected_index {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let source_color = match &var.source {
                envx_core::EnvVarSource::System => Color::Yellow,
                envx_core::EnvVarSource::User => Color::Green,
                envx_core::EnvVarSource::Process => Color::Blue,
                envx_core::EnvVarSource::Shell => Color::Magenta,
                envx_core::EnvVarSource::Application(_) => Color::Cyan,
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{:<30}", truncate_string(&var.name, 30)),
                    style.fg(Color::White),
                ),
                Span::raw(" │ "),
                Span::styled(
                    format!("{:<50}", truncate_string(&var.value, 50)),
                    style.fg(Color::Gray),
                ),
                Span::raw(" │ "),
                Span::styled(format!("{:?}", var.source), style.fg(source_color)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner_area);

    // Draw scrollbar if needed
    if app.filtered_vars.len() > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(app.filtered_vars.len()).position(app.scroll_offset);

        f.render_stateful_widget(
            scrollbar,
            inner_area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }

    // Draw selection indicator in the margin
    #[allow(clippy::cast_possible_truncation)]
    if !app.filtered_vars.is_empty() {
        let relative_selected = app.selected_index.saturating_sub(app.scroll_offset);
        if relative_selected < visible_height {
            let y = inner_area.y + relative_selected as u16;
            if y < inner_area.bottom() {
                let selection_indicator =
                    Span::styled("►", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
                f.render_widget(
                    Paragraph::new(selection_indicator),
                    Rect::new(inner_area.x.saturating_sub(2), y, 2, 1),
                );
            }
        }
    }
}

fn draw_edit_dialog(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area());

    let title = if matches!(app.mode, Mode::Add) {
        "Add Variable"
    } else {
        "Edit Variable"
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Name input
            Constraint::Length(1), // Separator
            Constraint::Min(5),    // Value textarea
            Constraint::Length(2), // Help text
        ])
        .split(inner_area);

    // Name input
    let name_style = if app.active_edit_field == EditField::Name {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let name_input = Paragraph::new(app.edit_name_input.value()).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Name")
            .border_style(name_style),
    );
    f.render_widget(name_input, chunks[0]);

    // Value textarea
    let value_style = if app.active_edit_field == EditField::Value {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let value_block = Block::default()
        .borders(Borders::ALL)
        .title("Value (supports multiple lines)")
        .border_style(value_style);

    // Render the block first, then the textarea widget inside it
    let inner_value_area = value_block.inner(chunks[2]);
    f.render_widget(value_block, chunks[2]);
    f.render_widget(&app.edit_value_textarea, inner_value_area);

    // Help text
    let help = Paragraph::new("Press Tab to switch fields, Ctrl+Enter to save, Esc to cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[3]);
}

fn draw_view_dialog(f: &mut Frame, app: &App, var_name: &str) {
    let area = centered_rect(90, 90, f.area());

    // Find the variable
    let var = app.filtered_vars.iter().find(|v| v.name == var_name);

    if let Some(var) = var {
        let block = Block::default()
            .title(format!("View Variable: {var_name}"))
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));

        let inner_area = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Variable info
                Constraint::Length(1), // Separator
                Constraint::Min(5),    // Value display
                Constraint::Length(2), // Help text
            ])
            .split(inner_area);

        // Variable info
        let info = vec![
            Line::from(vec![
                Span::styled("Source: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{:?}", var.source),
                    Style::default().fg(match &var.source {
                        envx_core::EnvVarSource::System => Color::Yellow,
                        envx_core::EnvVarSource::User => Color::Green,
                        envx_core::EnvVarSource::Process => Color::Blue,
                        envx_core::EnvVarSource::Shell => Color::Magenta,
                        envx_core::EnvVarSource::Application(_) => Color::Cyan,
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Modified: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(var.modified.format("%Y-%m-%d %H:%M:%S").to_string()),
            ]),
        ];

        let info_widget = Paragraph::new(info).block(Block::default().borders(Borders::NONE));
        f.render_widget(info_widget, chunks[0]);

        // Value display with line numbers
        let value_lines: Vec<Line> = if var.value.lines().count() > 1 {
            var.value
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    Line::from(vec![
                        Span::styled(format!("{:4} │ ", i + 1), Style::default().fg(Color::DarkGray)),
                        Span::raw(line),
                    ])
                })
                .collect()
        } else {
            vec![Line::from(var.value.clone())]
        };

        let value_widget = Paragraph::new(value_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Value")
                    .border_style(Style::default().fg(Color::Green)),
            )
            .wrap(Wrap { trim: false })
            .scroll((0, 0)); // Can be made scrollable in the future

        f.render_widget(value_widget, chunks[2]);

        // Help text
        let help = Paragraph::new("Press Esc or q to return to the list")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(help, chunks[3]);
    }
}

fn draw_confirm_dialog(f: &mut Frame, action: &ConfirmAction) {
    let area = centered_rect(50, 20, f.area());

    let message = match action {
        ConfirmAction::Delete(name) => format!("Delete variable '{name}'?"),
        ConfirmAction::Save(name, _) => format!("Save variable '{name}'?"),
    };

    let block = Block::default()
        .title("Confirm")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let text = Text::from(vec![
        Line::from(""),
        Line::from(message),
        Line::from(""),
        Line::from("Press [y] to confirm, [n] to cancel"),
    ]);

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, inner_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

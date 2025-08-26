mod app;
mod ui;

pub use app::App;

use color_eyre::Result;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::event::{self, Event},
};
use std::{
    io,
    time::{Duration, Instant},
};

/// Run the TUI application
///
/// # Errors
///
/// Returns an error if:
/// - Terminal setup fails
/// - App initialization fails
/// - Terminal operations fail during execution
/// - Cleanup operations fail
pub fn run() -> Result<()> {
    // Setup terminal
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Setup panic hook
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen).ok();
        crossterm::terminal::disable_raw_mode().ok();
        panic_hook(panic_info);
    }));

    // Enter alternate screen and enable raw mode
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    terminal.clear()?;

    // Simple event loop with debouncing
    let mut last_key_time: Option<Instant> = None;

    loop {
        // Draw UI
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(50))? {
            if let Ok(Event::Key(key)) = event::read() {
                // Only handle key press events, ignore key release
                if key.kind == event::KeyEventKind::Press {
                    let now = Instant::now();

                    // Debounce: ignore if key pressed within 100ms
                    if let Some(last_time) = last_key_time {
                        if now.duration_since(last_time) < Duration::from_millis(50) {
                            continue;
                        }
                    }

                    last_key_time = Some(now);

                    if app.handle_key_event(key)? {
                        break;
                    }
                }
            }
        }

        // Tick for status message timeout
        app.tick();
    }

    // Cleanup
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;

    terminal.show_cursor()?;

    Ok(())
}

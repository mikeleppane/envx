use color_eyre::Result;
use envx_core::{EnvVar, EnvVarManager};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;
use tui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
    Edit,
    Add,
    Confirm(ConfirmAction),
    View(String), // View mode for viewing full variable value
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    Delete(String),
    Save(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditField {
    Name,
    Value,
}

pub struct App {
    pub manager: EnvVarManager,
    pub mode: Mode,
    pub selected_index: usize,
    pub filtered_vars: Vec<EnvVar>,
    pub search_input: Input,
    pub edit_name_input: Input,
    pub edit_value_textarea: TextArea<'static>,
    pub active_edit_field: EditField,
    pub status_message: Option<(String, std::time::Instant)>,
    pub should_quit: bool,
    pub scroll_offset: usize,
}

impl App {
    /// Creates a new App instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable manager fails to load variables.
    pub fn new() -> Result<Self> {
        let mut manager = EnvVarManager::new();
        manager.load_all()?;
        let vars = manager.list().into_iter().cloned().collect();

        Ok(Self {
            manager,
            mode: Mode::Normal,
            selected_index: 0,
            filtered_vars: vars,
            search_input: Input::default(),
            edit_name_input: Input::default(),
            edit_value_textarea: TextArea::default(),
            active_edit_field: EditField::Name,
            status_message: None,
            should_quit: false,
            scroll_offset: 0,
        })
    }

    /// Handles a key event based on the current mode.
    ///
    /// Returns `true` if the event was handled and requires a re-render.
    ///
    /// # Errors
    ///
    /// Returns an error if there's a failure in environment variable operations,
    /// such as loading, saving, or deleting variables.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match self.mode {
            Mode::Normal => self.handle_normal_mode(key),
            Mode::Search => Ok(self.handle_search_mode(key)),
            Mode::Edit | Mode::Add => Ok(self.handle_edit_mode(key)),
            Mode::Confirm(ref action) => self.handle_confirm_mode(key, action.clone()),
            Mode::View(_) => Ok(self.handle_view_mode(key)),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q' | 'Q') => {
                self.should_quit = true;
                return Ok(true);
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_input.reset();
            }
            KeyCode::Char('a' | 'A') => {
                self.mode = Mode::Add;
                self.edit_name_input.reset();
                self.edit_value_textarea = TextArea::default();
                self.active_edit_field = EditField::Name;
            }
            KeyCode::Char('e' | 'E') => {
                if !self.filtered_vars.is_empty() {
                    let var = &self.filtered_vars[self.selected_index];
                    self.edit_name_input = Input::default().with_value(var.name.clone());

                    // Initialize textarea with the current value
                    let mut textarea = TextArea::from(var.value.lines().collect::<Vec<&str>>());
                    textarea.move_cursor(CursorMove::End);
                    self.edit_value_textarea = textarea;

                    self.active_edit_field = EditField::Name;
                    self.mode = Mode::Edit;
                }
            }
            KeyCode::Char('v' | 'V') | KeyCode::Enter => {
                if !self.filtered_vars.is_empty() {
                    let var = &self.filtered_vars[self.selected_index];
                    self.mode = Mode::View(var.name.clone());
                }
            }
            KeyCode::Char('d' | 'D') => {
                if !self.filtered_vars.is_empty() {
                    let var_name = self.filtered_vars[self.selected_index].name.clone();
                    self.mode = Mode::Confirm(ConfirmAction::Delete(var_name));
                }
            }
            KeyCode::Char('r' | 'R') => {
                self.refresh_vars()?;
                self.set_status("Refreshed environment variables");
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection_down();
            }
            KeyCode::PageUp => {
                self.page_up();
            }
            KeyCode::PageDown => {
                self.page_down();
            }
            KeyCode::Home => {
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            KeyCode::End => {
                if !self.filtered_vars.is_empty() {
                    self.selected_index = self.filtered_vars.len() - 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_view_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        false
    }

    const fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn move_selection_down(&mut self) {
        if !self.filtered_vars.is_empty() && self.selected_index < self.filtered_vars.len() - 1 {
            self.selected_index += 1;
        }
    }

    const fn page_up(&mut self) {
        if self.selected_index >= 10 {
            self.selected_index -= 10;
        } else {
            self.selected_index = 0;
        }
    }

    fn page_down(&mut self) {
        let max_index = if self.filtered_vars.is_empty() {
            0
        } else {
            self.filtered_vars.len() - 1
        };
        if self.selected_index + 10 <= max_index {
            self.selected_index += 10;
        } else {
            self.selected_index = max_index;
        }
    }

    pub const fn calculate_scroll(&mut self, visible_height: usize) {
        // Ensure selected item is visible
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index.saturating_sub(visible_height - 1);
        }
    }

    fn handle_search_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.apply_search();
            }
            _ => {
                self.search_input.handle_event(&Event::Key(key));
                self.apply_search();
            }
        }
        false
    }

    fn handle_edit_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Tab => {
                // Toggle between name and value fields
                self.active_edit_field = match self.active_edit_field {
                    EditField::Name => EditField::Value,
                    EditField::Value => EditField::Name,
                };
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Enter to save
                let name = self.edit_name_input.value().to_string();
                let value = self.edit_value_textarea.lines().join("\n");
                if !name.is_empty() {
                    self.mode = Mode::Confirm(ConfirmAction::Save(name, value));
                }
            }
            _ => {
                // Handle input based on active field
                match self.active_edit_field {
                    EditField::Name => {
                        self.edit_name_input.handle_event(&Event::Key(key));
                    }
                    EditField::Value => {
                        self.edit_value_textarea.input(key);
                    }
                }
            }
        }
        false
    }

    fn handle_confirm_mode(&mut self, key: KeyEvent, action: ConfirmAction) -> Result<bool> {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                match action {
                    ConfirmAction::Delete(name) => match self.manager.delete(&name) {
                        Ok(()) => {
                            self.refresh_vars()?;
                            self.set_status(&format!("Deleted variable: {name}"));
                        }
                        Err(e) => {
                            self.set_status(&format!("Error deleting variable: {e}"));
                        }
                    },
                    ConfirmAction::Save(name, value) => match self.manager.set(&name, &value, true) {
                        Ok(()) => {
                            self.refresh_vars()?;
                            self.set_status(&format!("Saved variable: {name}"));
                        }
                        Err(e) => {
                            self.set_status(&format!("Error saving variable: {e}"));
                        }
                    },
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(false)
    }

    pub fn tick(&mut self) {
        // Remove status message after timeout
        if let Some((_, timestamp)) = &self.status_message {
            if timestamp.elapsed().as_secs() > 3 {
                self.status_message = None;
            }
        }
    }

    fn apply_search(&mut self) {
        let search_term = self.search_input.value();
        if search_term.is_empty() {
            self.filtered_vars = self.manager.list().into_iter().cloned().collect();
        } else {
            self.filtered_vars = self.manager.search(search_term).into_iter().cloned().collect();
        }

        // Reset selection and scroll if it's out of bounds
        if self.selected_index >= self.filtered_vars.len() && !self.filtered_vars.is_empty() {
            self.selected_index = self.filtered_vars.len() - 1;
        }
        self.scroll_offset = 0; // Reset scroll when search changes
    }

    fn refresh_vars(&mut self) -> Result<()> {
        self.manager.load_all()?;
        self.apply_search();
        Ok(())
    }

    fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), std::time::Instant::now()));
    }
}

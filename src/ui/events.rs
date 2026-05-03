//! Event handling for the Ionix TUI.
//!
//! Key fix: On Windows, crossterm fires Key events for both Press and Release.
//! We ONLY accept KeyEventKind::Press and discard everything else.
//!
//! Key bindings:
//! - Esc: cancel inline edit → cancel search → quit
//! - Ctrl+S: save to file
//! - Space/y: toggle bool
//! - Enter: toggle bool / edit numeric or string inline / toggle menu
//! - n: set bool to false
//! - r: revert item, R: revert all
//! - +/-: increment/decrement numeric values
//! - /: search, h: help, x: expert
//! - j/k or Up/Down: navigate, Left/Right: page up/down

use crate::schema::ConfigType;
use crate::ui::app::{AppState, FilterMode, ListEntry, SearchMode};
use anyhow::Result;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Refresh,
}

pub struct EventHandler {
    poll_timeout: Duration,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            poll_timeout: Duration::from_millis(50),
        }
    }

    pub fn next(&mut self) -> Result<AppEvent> {
        use crossterm::event::{poll, read, Event, KeyEventKind};

        if poll(self.poll_timeout)? {
            let event = read()?;
            match event {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        Ok(AppEvent::Key(key))
                    } else {
                        Ok(AppEvent::Refresh)
                    }
                }
                Event::Resize(w, h) => Ok(AppEvent::Resize(w, h)),
                _ => Ok(AppEvent::Refresh),
            }
        } else {
            Ok(AppEvent::Refresh)
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    Save,
    /// Save then quit
    SaveAndQuit,
    /// Quit with unsaved changes — show TUI save dialog
    QuitWithSavePrompt,
}

fn is_modified(app: &AppState) -> bool {
    !app.modified.is_empty()
}

/// Handle a key event. Returns an action for the main loop, or None for navigation/edit.
pub fn handle_key_event(app: &mut AppState, key: crossterm::event::KeyEvent) -> Option<KeyAction> {
    use crossterm::event::KeyCode::*;

    let code = key.code;

    // ----- Save dialog mode: Y/N/Enter/Esc -----
    if app.save_dialog {
        match code {
            Char('y') | Char('Y') | Enter => {
                app.save_dialog = false;
                return Some(KeyAction::SaveAndQuit);
            }
            Char('n') | Char('N') => {
                app.save_dialog = false;
                return Some(KeyAction::Quit);
            }
            Esc => {
                app.save_dialog = false;
                app.set_status("Quit cancelled");
                return None;
            }
            _ => {}
        }
        return None;
    }

    // ----- Inline edit mode: HIGHEST PRIORITY -----
    // This must come BEFORE any other key handling so that Enter
    // confirms the edit instead of triggering toggle/enter-edit again.
    if app.inline_edit.is_some() {
        let edit = app.inline_edit.clone().unwrap();
        let mut value = edit.value.clone();
        let mut cursor = edit.cursor;
        let is_numeric = edit.is_numeric;
        let edit_key = edit.key.clone();

        match code {
            Enter => {
                // Confirm: apply the edited value
                if is_numeric {
                    if let Ok(val) = value.parse::<i64>() {
                        // Use the key to set the value directly in modified map
                        let type_ok = app.schema.get(&edit_key).map(|item| {
                            match item.config_type {
                                ConfigType::U8 if (0..=255).contains(&val) => true,
                                ConfigType::U16 if (0..=65535).contains(&val) => true,
                                ConfigType::U32 | ConfigType::U64 | ConfigType::Usize if val >= 0 => true,
                                _ => false,
                            }
                        }).unwrap_or(false);

                        if type_ok {
                            app.modified.insert(edit_key.clone(), toml::Value::Integer(val));
                            app.inline_edit = None;
                            app.set_status(format!("{} = {}", edit_key, val));
                        } else {
                            app.set_error(format!("Value out of range: {}", value));
                        }
                    } else {
                        app.set_error(format!("Invalid number: {}", value));
                    }
                } else {
                    app.modified.insert(edit_key.clone(), toml::Value::String(value.clone()));
                    app.inline_edit = None;
                    app.set_status(format!("{} = \"{}\"", edit_key, value));
                }
            }
            Esc => {
                app.inline_edit = None;
                app.set_status("Edit cancelled");
            }
            Backspace => {
                if cursor > 0 {
                    cursor -= 1;
                    value.remove(cursor);
                    if let Some(ref mut e) = app.inline_edit {
                        e.value = value;
                        e.cursor = cursor;
                    }
                }
            }
            Delete => {
                if cursor < value.len() {
                    value.remove(cursor);
                    if let Some(ref mut e) = app.inline_edit {
                        e.value = value;
                    }
                }
            }
            Left => {
                if cursor > 0 {
                    cursor -= 1;
                    if let Some(ref mut e) = app.inline_edit {
                        e.cursor = cursor;
                    }
                }
            }
            Right => {
                if cursor < value.len() {
                    cursor += 1;
                    if let Some(ref mut e) = app.inline_edit {
                        e.cursor = cursor;
                    }
                }
            }
            Home => {
                if let Some(ref mut e) = app.inline_edit {
                    e.cursor = 0;
                }
            }
            End => {
                if let Some(ref mut e) = app.inline_edit {
                    e.cursor = e.value.len();
                }
            }
            Char(c) => {
                if is_numeric && !c.is_ascii_digit() {
                    app.set_error("Only digits allowed");
                    return None;
                }
                value.insert(cursor, c);
                cursor += 1;
                if let Some(ref mut e) = app.inline_edit {
                    e.value = value;
                    e.cursor = cursor;
                }
            }
            _ => {}
        }
        return None;
    }

    // ----- ESC: cancel search → back in menu → quit -----
    if code == Esc {
        if app.search_mode == SearchMode::Searching {
            app.exit_search();
            app.set_status("Search cancelled");
            return None;
        } else if app.in_menu() {
            // In a submenu → go back to parent
            app.exit_menu();
            app.set_status("Went back");
            return None;
        } else if is_modified(app) {
            // At root with unsaved changes → show save dialog
            app.save_dialog = true;
            return None;
        } else {
            return Some(KeyAction::Quit);
        }
    }

    // ----- Other global hotkeys -----
    match code {
        Char('/') | Char('?') if app.search_mode != SearchMode::Searching => {
            app.start_search();
            return None;
        }
        Char('h') | Char('H') if app.search_mode == SearchMode::None => {
            app.show_help = !app.show_help;
            return None;
        }
        Char('x') | Char('X') if app.search_mode == SearchMode::None => {
            app.toggle_expert();
            return None;
        }
        _ => {}
    }

    // ----- Search mode: handle text input -----
    if app.search_mode == SearchMode::Searching {
        match code {
            Enter => { app.exit_search(); }
            Backspace => {
                if app.search_query.is_empty() {
                    app.exit_search();
                    app.set_status("Search cancelled");
                } else {
                    app.search_query.pop();
                    app.update_search(app.search_query.clone());
                }
            }
            Char(c) => {
                app.search_query.push(c);
                app.update_search(app.search_query.clone());
            }
            _ => {}
        }
        return None;
    }

    // ----- Check for menu entry (full-page view) -----
    // Enter on menu header opens full-page menu view
    let menu_to_enter = match app.selected_entry() {
        Some(ListEntry::Menu { key, .. }) if code == Enter && !app.in_menu() => Some(key.clone()),
        _ => None,
    };
    if let Some(key) = menu_to_enter {
        app.enter_menu(&key);
        app.set_status(format!("Entered menu: {}", app.current_menu_name().unwrap_or_default()));
        return None;
    }

    // ----- Handle BackMenu (when in submenu, Back entry at bottom) -----
    if let Some(ListEntry::BackMenu { .. }) = app.selected_entry() {
        if code == Enter || code == Char('e') || code == Char('E') || code == Esc {
            app.exit_menu();
            app.set_status("Went back");
            return None;
        }
    }

    // ----- Edit mode (always active): handle value editing -----
    let item_info = match app.selected_entry() {
        Some(ListEntry::Item { schema_idx, item }) => {
            Some((*schema_idx, item.config_type, item.key.clone()))
        }
        Some(ListEntry::Menu { .. }) | Some(ListEntry::BackMenu { .. }) => None,
        None => None,
    };

    if let Some((idx, cfg_type, key_name)) = item_info {
        match code {
            // Space/y: toggle bool
            Char(' ') | Char('y') | Char('Y')
            if cfg_type == ConfigType::Bool =>
            {
                app.toggle_bool(idx);
                let new_val = app.effective_value(&key_name).as_bool().unwrap_or(false);
                app.set_status(format!("{} = {}", key_name, if new_val { "true [*]" } else { "false [ ]" }));
                return None;
            }
            // Enter: toggle bool OR enter inline edit for numeric/string
            Enter => {
                match cfg_type {
                    ConfigType::Bool => {
                        app.toggle_bool(idx);
                        let new_val = app.effective_value(&key_name).as_bool().unwrap_or(false);
                        app.set_status(format!("{} = {}", key_name, if new_val { "true [*]" } else { "false [ ]" }));
                    }
                    ConfigType::U8 | ConfigType::U16 | ConfigType::U32
                    | ConfigType::U64 | ConfigType::Usize => {
                        let current = app.effective_value(&key_name).as_integer().unwrap_or(0);
                        let s = current.to_string();
                        app.inline_edit = Some(crate::ui::app::InlineEdit {
                            key: key_name.clone(),
                            value: s.clone(),
                            cursor: s.len(),
                            is_numeric: true,
                        });
                        app.set_status(format!("Edit {}: type number, Enter=confirm, Esc=cancel", key_name));
                    }
                    ConfigType::String => {
                        let current = app.effective_value(&key_name).as_str().unwrap_or("").to_string();
                        let cursor = current.len();
                        app.inline_edit = Some(crate::ui::app::InlineEdit {
                            key: key_name.clone(),
                            value: current,
                            cursor,
                            is_numeric: false,
                        });
                        app.set_status(format!("Edit {}: type value, Enter=confirm, Esc=cancel", key_name));
                    }
                }
                return None;
            }
            // n: force false (bool only)
            Char('n') | Char('N') if cfg_type == ConfigType::Bool => {
                app.modified.insert(key_name.clone(), toml::Value::Boolean(false));
                app.set_status(format!("{} = false [ ]", key_name));
                return None;
            }
            // r: revert item
            Char('r') if !key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                app.revert_item(&key_name);
                app.set_status(format!("Reverted {}", key_name));
                return None;
            }
            // R: revert all
            Char('R') => {
                app.revert_all();
                app.set_status("Reverted all");
                return None;
            }
            // +/-: increment/decrement numeric values
            Char('+') | Char('=') if cfg_type.is_unsigned() => {
                let current = app.effective_value(&key_name).as_integer().unwrap_or(0);
                let new_val = current + 1;
                app.set_numeric(idx, new_val);
                app.set_status(format!("{} = {}", key_name, new_val));
                return None;
            }
            Char('-') if cfg_type.is_unsigned() => {
                let current = app.effective_value(&key_name).as_integer().unwrap_or(0);
                let new_val = current.saturating_sub(1);
                app.set_numeric(idx, new_val);
                app.set_status(format!("{} = {}", key_name, new_val));
                return None;
            }
            _ => {}
        }
    }

    // ----- Navigation -----
    match code {
        Up | Char('k') => app.move_up(),
        Down | Char('j') => app.move_down(),
        Left => app.move_page_up(),
        Right => app.move_page_down(),
        PageUp => app.move_page_up(),
        PageDown => app.move_page_down(),
        Home => { app.selected_index = 0; app.scroll_offset = 0; }
        End => { app.selected_index = app.list_entries.len().saturating_sub(1); }
        _ => {}
    }

    // ----- Filter views -----
    match code {
        Char('1') => app.set_filter(FilterMode::All),
        Char('2') => app.set_filter(FilterMode::Modified),
        Char('3') => app.set_filter(FilterMode::Expert),
        _ => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ConfigSchema;

    fn make_test_app() -> AppState {
        let toml = r#"
[[items]]
name = "Test Bool"
key = "TEST_BOOL"
type = "bool"
default = false
[[items]]
name = "Test Num"
key = "TEST_NUM"
type = "u64"
default = 42
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        AppState::new(schema)
    }

    fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty())
    }

    #[test]
    fn test_navigation() {
        let mut app = make_test_app();
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Down));
        assert_eq!(app.selected_index, 1);
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Up));
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_space_toggles_bool() {
        let mut app = make_test_app();
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Char(' ')));
        assert!(app.is_modified("TEST_BOOL"));
        assert_eq!(app.effective_value("TEST_BOOL").as_bool(), Some(true));
    }

    #[test]
    fn test_esc_quits_when_clean() {
        let mut app = make_test_app();
        let action = handle_key_event(&mut app, key(crossterm::event::KeyCode::Esc));
        assert_eq!(action, Some(KeyAction::Quit));
    }

    #[test]
    fn test_esc_shows_save_dialog_when_modified() {
        let mut app = make_test_app();
        app.modified.insert("TEST_BOOL".to_string(), toml::Value::Boolean(true));
        let action = handle_key_event(&mut app, key(crossterm::event::KeyCode::Esc));
        assert_eq!(action, None);
        assert!(app.save_dialog);
    }


    #[test]
    fn test_enter_numeric_opens_inline_edit() {
        let mut app = make_test_app();
        app.selected_index = 1; // Select the numeric item
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Enter));
        assert!(app.inline_edit.is_some());
    }

    #[test]
    fn test_enter_confirms_inline_edit() {
        let mut app = make_test_app();
        app.selected_index = 1;
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Enter));
        // Clear the old value first (backspace "42")
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Backspace));
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Backspace));
        // Type new value
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Char('1')));
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Char('0')));
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Char('0')));
        // Confirm
        handle_key_event(&mut app, key(crossterm::event::KeyCode::Enter));
        assert!(app.inline_edit.is_none());
        assert_eq!(app.effective_value("TEST_NUM").as_integer(), Some(100));
    }
}
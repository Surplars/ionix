//! Help panel widget - shows detailed help text for selected item.

use crate::ui::app::{AppState, ListEntry};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

const HELP_LINES: usize = 12;

pub struct HelpPanel {
    show_full: bool,
}

impl HelpPanel {
    pub fn new() -> Self {
        Self { show_full: false }
    }

    pub fn show_full(mut self, full: bool) -> Self {
        self.show_full = full;
        self
    }
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for HelpPanel {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.height < 3 {
            return;
        }

        let block = Block::bordered()
            .title(" Help (H=toggle) ")
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        block.render(area, buf);

        if !state.show_help {
            let msg = Line::raw(" Help panel hidden (press H to show)");
            buf.set_line(inner.x + 1, inner.y + 1, &msg, inner.width - 2);
            return;

    // Handle menu header or exit entry
    match state.selected_entry() {
        Some(ListEntry::BackMenu { .. }) => {
            let msg = Line::styled(
                " [Enter/Esc] Go back",
                Style::default().fg(Color::Magenta),
            );
            buf.set_line(inner.x + 1, inner.y + 1, &msg, inner.width - 2);
            return;
        }
        Some(ListEntry::Menu { name, .. }) => {
            let msg = Line::styled(
                format!(" Menu: {} - [Enter] Open", name),
                Style::default().fg(Color::Cyan),
            );
            buf.set_line(inner.x + 1, inner.y + 1, &msg, inner.width - 2);
            return;
        }
        _ => {}
    }

        }

        if let Some((_, item)) = state.selected_item() {
            let mut y = inner.y + 1;

            // Item name
            let name_line = Line::styled(
                format!(" {} ({})", item.name, item.key),
                Style::default().add_modifier(ratatui::style::Modifier::BOLD),
            );
            buf.set_line(inner.x + 1, y, &name_line, inner.width - 2);
            y += 1;

            // Type and default
            let default_str = state.display_value(item);
            let type_line = Line::raw(format!(
                " Type: {}, Value: {}",
                item.config_type,
                default_str
            ));
            buf.set_line(inner.x + 1, y, &type_line, inner.width - 2);
            y += 1;

            // Deprecation warning
            if let Some(dep) = &item.deprecated {
                let dep_line = Line::styled(
                    format!(" DEPRECATED: {}", dep.message),
                    Style::default().fg(Color::Yellow),
                );
                buf.set_line(inner.x + 1, y, &dep_line, inner.width - 2);
                y += 1;

                if let Some(replacement) = &dep.replaced_by {
                    let rep_line = Line::styled(
                        format!(" -> Replaced by: {}", replacement),
                        Style::default().fg(Color::DarkGray),
                    );
                    buf.set_line(inner.x + 1, y, &rep_line, inner.width - 2);
                    y += 1;
                }
            }

            // Dependencies
            if !item.depends_on.is_empty() {
                let dep_line = Line::raw(format!(" Depends on: {}", item.depends_on.join(", ")));
                buf.set_line(inner.x + 1, y, &dep_line, inner.width - 2);
                y += 1;
            }

            // Flags
            let mut flags = Vec::new();
            if item.expert {
                flags.push("expert");
            }
            if item.rebuild {
                flags.push("rebuild");
            }
            if !flags.is_empty() {
                let flags_line = Line::styled(
                    format!(" [{}]", flags.join("] [")),
                    Style::default().fg(Color::DarkGray),
                );
                buf.set_line(inner.x + 1, y, &flags_line, inner.width - 2);
                y += 1;
            }

            y += 1; // Spacer

            // Help text (word wrap)
            if !item.help.is_empty() {
                let wrapped = Self::wrap_text(&item.help, inner.width as usize - 4);
                for line in wrapped.into_iter().take(HELP_LINES.saturating_sub(
                    (y - inner.y) as usize
                )) {
                    let help_line = Line::raw(format!(" {}", line));
                    buf.set_line(inner.x + 1, y, &help_line, inner.width - 2);
                    y += 1;
                    if y >= inner.y + inner.height {
                        break;
                    }
                }
            }

            // Edit hint
            let edit_hint = match item.config_type {
                crate::schema::ConfigType::Bool => " [Enter/Space/y] Toggle | [n] Set false",
                _ => " [Enter] Edit | [+/-] Adjust",
            };
            let hint_line = Line::styled(
                edit_hint,
                Style::default().fg(Color::Green),
            );
            if y < inner.y + inner.height {
                buf.set_line(inner.x + 1, y, &hint_line, inner.width - 2);
            }
        } else {
            let msg = Line::raw(" No item selected");
            buf.set_line(inner.x + 1, inner.y + 1, &msg, inner.width - 2);
        }
    }
}

impl HelpPanel {
    fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = String::new();

        for word in text.split_whitespace() {
            if current.len() + word.len() + 1 <= max_width {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            } else {
                if !current.is_empty() {
                    lines.push(current.clone());
                    current.clear();
                }
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }

        lines
    }
}

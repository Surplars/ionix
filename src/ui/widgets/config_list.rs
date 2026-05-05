//! Configuration list widget - main menu displaying all config items.

use crate::ui::app::{AppState, ListEntry};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

pub struct ConfigList;

impl ConfigList {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConfigList {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for ConfigList {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let selected = state.selected_index;
        let title = if let Some(name) = state.current_menu_name() {
            format!(
                " Ionix - {} ({}/{}) ",
                name,
                selected.saturating_add(1),
                state.list_entries.len()
            )
        } else {
            let stats = state.stats();
            format!(
                " Ionix Configuration ({}/{}, {} modified) ",
                selected.saturating_add(1),
                state.list_entries.len(),
                stats.modified
            )
        };

        let block = Block::bordered()
            .title(title)
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        block.render(area, buf);
        state.set_visible_height(inner.height as usize);

        let entries = &state.list_entries;
        if entries.is_empty() {
            let msg = Line::raw(" No items match current filter");
            buf.set_line(
                inner.x + 1,
                inner.y + inner.height / 2,
                &msg,
                inner.width.saturating_sub(2),
            );
            return;
        }

        let visible_height = inner.height as usize;
        let start_idx = state
            .scroll_offset
            .min(entries.len().saturating_sub(visible_height));
        let end_idx = (start_idx + visible_height).min(entries.len());

        for (rel_idx, entry) in entries[start_idx..end_idx].iter().enumerate() {
            let y = inner.y + rel_idx as u16;
            let abs_idx = start_idx + rel_idx;
            let is_selected = abs_idx == selected;

            match entry {
                ListEntry::BackMenu { .. } => {
                    let line = if is_selected { "> .." } else { "  .." };
                    let style = entry_style(is_selected, Color::Magenta);
                    buf.set_line(inner.x + 1, y, &Line::styled(line, style), inner.width - 2);
                }
                ListEntry::Menu { name, expanded, .. } => {
                    let cursor = if is_selected { ">" } else { " " };
                    let glyph = if *expanded { ">" } else { "+" };
                    let width = inner.width.saturating_sub(4) as usize;
                    let text_width = width.saturating_sub(4);
                    let name = truncate(name, text_width);
                    let line = format!(
                        "{} {:<text_width$} {} ",
                        cursor,
                        name,
                        glyph,
                        text_width = text_width
                    );
                    let style = entry_style(is_selected, Color::Cyan);
                    buf.set_line(inner.x + 1, y, &Line::styled(line, style), inner.width - 2);
                }
                ListEntry::Item { item, .. } => {
                    let is_modified = state.is_modified(&item.key);
                    let deps_satisfied = state.dependencies_satisfied(item);
                    let blockers = state.check_enable_blockers(item);
                    let has_blockers = !blockers.is_empty();

                    let style = item_style(is_selected, is_modified, deps_satisfied, has_blockers);
                    let cursor = if is_selected { ">" } else { " " };
                    let modified = if is_modified { "*" } else { " " };
                    let tag = if item.expert { "EX" } else { "" };
                    let value = item_value(state, item);

                    let width = inner.width.saturating_sub(2) as usize;
                    let value_width = if width >= 72 { 18 } else { 12 };
                    let type_width = 6;
                    let tag_width = 2;
                    let fixed_width = 2 + 1 + 3 + value_width + 1 + type_width + 1 + tag_width;
                    let name_width = width.saturating_sub(fixed_width).max(12);
                    let line = format!(
                        "{}{} {:<name_width$} {:>value_width$} {:>type_width$} {:>tag_width$}",
                        cursor,
                        modified,
                        truncate(&item.name, name_width),
                        truncate(&value, value_width),
                        item.config_type,
                        tag,
                        name_width = name_width,
                        value_width = value_width,
                        type_width = type_width,
                        tag_width = tag_width,
                    );

                    buf.set_line(inner.x + 1, y, &Line::styled(line, style), inner.width - 2);
                }
            }
        }

        if entries.len() > visible_height {
            render_scrollbar(buf, inner, selected, entries.len(), visible_height);
        }
    }
}

fn item_value(state: &AppState, item: &crate::schema::ConfigItem) -> String {
    if let Some(edit) = &state.inline_edit {
        if edit.key == item.key {
            let mut display = edit.value.clone();
            if edit.cursor <= display.len() {
                display.insert(edit.cursor, '_');
            }
            return format!("<{}>", display);
        }
    }
    state.display_value(item)
}

fn entry_style(selected: bool, color: Color) -> Style {
    if selected {
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

fn item_style(selected: bool, modified: bool, deps_satisfied: bool, has_blockers: bool) -> Style {
    let mut style = Style::default();

    if selected {
        style = style.bg(Color::Blue).fg(Color::White);
    } else if has_blockers || !deps_satisfied {
        style = style.fg(Color::DarkGray);
    }

    if modified && !selected {
        style = style.add_modifier(Modifier::BOLD).fg(Color::Yellow);
    }

    style
}

fn render_scrollbar(buf: &mut Buffer, area: Rect, selected: usize, total: usize, visible: usize) {
    let scrollbar_height = (area.height as f32 * (visible as f32 / total as f32))
        .ceil()
        .max(1.0) as u16;
    let thumb_position =
        (selected as f32 / total as f32 * (area.height - scrollbar_height) as f32) as u16;

    for y in area.y..area.y + area.height {
        if let Some(cell) = buf.cell_mut((area.x + area.width - 1, y)) {
            cell.set_symbol("|").set_fg(Color::DarkGray);
        }
    }

    for i in 0..scrollbar_height {
        if let Some(cell) = buf.cell_mut((area.x + area.width - 1, area.y + thumb_position + i)) {
            cell.set_symbol("#").set_fg(Color::White);
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max <= 3 {
        s.chars().take(max).collect()
    } else {
        let mut out: String = s.chars().take(max - 3).collect();
        out.push_str("...");
        out
    }
}

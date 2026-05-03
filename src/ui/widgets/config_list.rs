//! Configuration list widget - main menu displaying all config items.

#![allow(deprecated)]

use crate::ui::app::{AppState, ListEntry};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

pub struct ConfigList {
    /// Height of visible area (updated by StatefulWidget)
    height: u16,
}

impl ConfigList {
    pub fn new() -> Self {
        Self { height: 20 }
    }
}

impl Default for ConfigList {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for ConfigList {
    type State = AppState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.height = area.height;

        let entries = &state.list_entries;
        let selected = state.selected_index;
        let scroll = state.scroll_offset;

        let block = Block::bordered()
            .title(" Kernel Configuration ")
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        block.render(area, buf);

        if entries.is_empty() {
            let msg = Line::raw(" No items match current filter");
            buf.set_line(inner.x + 1, inner.y + inner.height / 2, &msg, inner.width - 2);
            return;
        }

        let visible_height = inner.height as usize;
        let start_idx = scroll.min(entries.len().saturating_sub(visible_height));
        let end_idx = (start_idx + visible_height).min(entries.len());

        for (rel_idx, entry) in entries[start_idx..end_idx].iter().enumerate() {
            let y = inner.y + rel_idx as u16;
            let abs_idx = start_idx + rel_idx;

            let is_selected = abs_idx == selected;

            match entry {
                ListEntry::BackMenu { .. } => {
                    let line = " Exit ";
                    let style = if is_selected {
                        Style::default().bg(Color::Magenta).fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                    };
                    let styled_line = Line::styled(line, style);
                    buf.set_line(inner.x + 1, y, &styled_line, inner.width - 2);
                }
                ListEntry::Menu { key: _, name, expanded } => {
                    let arrow = if *expanded { "[-]" } else { "[+]" };
                    let line = format!(" {} -- {}", arrow, name);
                    let style = if is_selected {
                        Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    };
                    let styled_line = Line::styled(line, style);
                    buf.set_line(inner.x + 1, y, &styled_line, inner.width - 2);
                }
                ListEntry::Item { schema_idx: _, item } => {
                    let is_modified = state.is_modified(&item.key);
                    let deps_satisfied = state.dependencies_satisfied(item);
                    let blockers = state.check_enable_blockers(item);
                    let has_blockers = !blockers.is_empty();

                    let style = Self::item_style(is_selected, is_modified, deps_satisfied, has_blockers);

                    let prefix = if is_selected { " >" } else { "  " };
                    let modified_flag = if is_modified { "*" } else { " " };

                    let value_str = if let Some(ref edit) = state.inline_edit {
                        if edit.key == item.key {
                            let mut display = edit.value.clone();
                            if edit.cursor <= display.len() {
                                display.insert(edit.cursor, '\u{2593}');
                            }
                            format!(">> {} <<", display)
                        } else {
                            state.display_value(item)
                        }
                    } else {
                        state.display_value(item)
                    };
                    let type_str = item.config_type.to_string();

                    let line = format!(
                        "{}{} {} {:20} {:>8} = {}",
                        prefix,
                        modified_flag,
                        Self::visibility_indicator(item.expert),
                        item.key,
                        type_str,
                        value_str
                    );

                    let styled_line = Line::styled(line, style);
                    buf.set_line(inner.x + 1, y, &styled_line, inner.width - 2);

                    if is_selected && has_blockers {
                        let warning = format!(" ! Blocked by: {}", blockers.join(", "));
                        let warn_style = Style::default().fg(Color::Yellow);
                        let warn_line = Line::styled(warning, warn_style);
                        buf.set_line(inner.x + 1, y + 1, &warn_line, inner.width - 2);
                    }

                    if is_selected && !item.depends_on.is_empty() {
                        let deps: Vec<String> = item
                            .depends_on
                            .iter()
                            .map(|d| {
                                let satisfied = state.effective_value(d).as_bool().unwrap_or(false);
                                if satisfied {
                                    d.clone()
                                } else {
                                    format!("!{d}")
                                }
                            })
                            .collect();

                        let dep_line = Line::styled(
                            format!(" Deps: {}", deps.join(" + ")),
                            Style::default().fg(Color::DarkGray),
                        );
                        buf.set_line(inner.x + 1, y + if has_blockers { 2 } else { 1 }, &dep_line, inner.width - 2);
                    }
                }
            }
        }

        if entries.len() > visible_height {
            Self::render_scrollbar(buf, inner, selected, entries.len(), visible_height);
        }
    }
}

impl ConfigList {
    fn item_style(
        selected: bool,
        modified: bool,
        deps_satisfied: bool,
        has_blockers: bool,
    ) -> Style {
        let mut style = Style::default();

        if selected {
            style = style.bg(Color::Blue).fg(Color::White);
        } else if has_blockers {
            style = style.fg(Color::DarkGray);
        } else if !deps_satisfied {
            style = style.fg(Color::DarkGray);
        }

        if modified && !selected {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }

    fn visibility_indicator(expert: bool) -> &'static str {
        if expert {
            "[X]"
        } else {
            "[ ]"
        }
    }

    fn render_scrollbar(
        buf: &mut Buffer,
        area: Rect,
        selected: usize,
        total: usize,
        visible: usize,
    ) {
        let scrollbar_height = (area.height as f32 * (visible as f32 / total as f32)).ceil() as u16;
        let thumb_position = (selected as f32 / total as f32 * (area.height - scrollbar_height) as f32) as u16;

        for i in 0..scrollbar_height {
            buf.get_mut(area.x + area.width - 1, area.y + thumb_position + i)
                .set_fg(Color::White);
        }
    }
}
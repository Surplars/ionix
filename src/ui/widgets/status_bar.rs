//! Status bar widget showing current mode and messages.

use crate::ui::app::AppState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

const STATUSBAR_MAX_MSG: usize = 40;

pub struct StatusBar {}

impl StatusBar {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for StatusBar {
    type State = AppState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 5 {
            return;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Mode indicator
        let (mode_str, mode_color) = if state.save_dialog {
            ("[SAVE?] ", Color::Yellow)
        } else if state.inline_edit.is_some() {
            ("[INPUT] ", Color::Magenta)
        } else if state.search_mode == crate::ui::app::SearchMode::Searching {
            ("[SEARCH] ", Color::Cyan)
        } else {
            ("[EDIT] ", Color::Green)
        };
        spans.push(Span::styled(mode_str, Style::default().fg(mode_color).add_modifier(ratatui::style::Modifier::BOLD)));

        // Expert mode
        if state.expert_mode {
            spans.push(Span::styled("[EXPERT] ", Style::default().fg(Color::Yellow)));
        }

        // Status message
        if let Some(ref msg) = state.status_message {
            let truncated = truncate(msg, STATUSBAR_MAX_MSG);
            spans.push(Span::raw(truncated));
        }

        // Stats at the right
        let stats = state.stats();
        let stats_str = format!(
            " {}/{} | {} modified",
            stats.visible,
            stats.total,
            stats.modified,
        );
        spans.push(Span::styled(stats_str, Style::default().fg(Color::DarkGray)));

        let line = Line::from(spans);
        buf.set_line(inner.x, inner.y, &line, inner.width);

        // Error message (if any)
        if let Some(err) = &state.error_message {
            let err_line = Line::styled(
                format!(" Error: {}", truncate(err, STATUSBAR_MAX_MSG)),
                Style::default().fg(Color::Red),
            );
            if inner.height > 1 {
                buf.set_line(inner.x, inner.y + 1, &err_line, inner.width);
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

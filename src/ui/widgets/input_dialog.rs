//! Input dialog widget for editing numeric and string values.

use crossterm::event::{KeyEvent, KeyCode};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Widget},
};

pub struct InputDialog {
    title: String,
    prompt: String,
    input: String,
    cursor_pos: usize,
    error: Option<String>,
    is_numeric: bool,
}

impl InputDialog {
    pub fn new(title: &str, prompt: &str) -> Self {
        Self {
            title: title.to_string(),
            prompt: prompt.to_string(),
            input: String::new(),
            cursor_pos: 0,
            error: None,
            is_numeric: false,
        }
    }

    pub fn numeric(mut self, numeric: bool) -> Self {
        self.is_numeric = numeric;
        self
    }

    pub fn initial_value(mut self, value: &str) -> Self {
        self.input = value.to_string();
        self.cursor_pos = value.len();
        self
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<DialogResult> {
        match key.code {
            KeyCode::Enter => {
                return Some(DialogResult::Submit(self.input.clone()));
            }
            KeyCode::Esc => {
                return Some(DialogResult::Cancel);
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input.len();
            }
            KeyCode::Char(c) => {
                if self.is_numeric && !c.is_ascii_digit() {
                    self.set_error(Some("Only digits allowed".to_string()));
                    return None;
                }
                self.input.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
                self.set_error(None);
            }
            _ => {}
        }

        None
    }
}

impl Widget for InputDialog {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 {
            return;
        }

        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        // Prompt
        let prompt_line = Line::raw(&self.prompt);
        buf.set_line(inner.x + 1, inner.y + 1, &prompt_line, inner.width - 2);

        // Input field
        let field_width = inner.width.saturating_sub(4);
        let display_input = if self.input.len() > field_width as usize {
            let start = self.input.len() - field_width as usize;
            &self.input[start..]
        } else {
            &self.input
        };

        let cursor_rel = self.cursor_pos.saturating_sub(
            if self.input.len() > field_width as usize {
                self.input.len() - field_width as usize
            } else {
                0
            },
        );

        let mut input_text = String::new();
        for (i, c) in display_input.chars().enumerate() {
            if i == cursor_rel {
                input_text.push('\u{2593}'); // Block cursor
            }
            input_text.push(c);
        }
        if cursor_rel == display_input.len() {
            input_text.push('\u{2593}');
        }

        let input_style = if self.error.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Yellow)
        };

        let input_line = Line::styled(input_text, input_style);
        buf.set_line(inner.x + 2, inner.y + 3, &input_line, field_width);

        // Underline
        let underline = "_".repeat(field_width as usize);
        let underline_line = Line::raw(underline.as_str());
        buf.set_line(inner.x + 2, inner.y + 4, &underline_line, field_width);

        // Error message
        if let Some(err) = &self.error {
            let err_line = Line::styled(err, Style::default().fg(Color::Red));
            buf.set_line(inner.x + 1, inner.y + 5, &err_line, inner.width - 2);
        }

        // Hint
        let hint = Line::styled(
            " Enter: confirm | Esc: cancel ",
            Style::default().fg(Color::DarkGray),
        );
        buf.set_line(
            inner.x + 1,
            inner.y + inner.height.saturating_sub(1),
            &hint,
            inner.width - 2,
        );
    }
}

#[derive(Debug, Clone)]
pub enum DialogResult {
    Submit(String),
    Cancel,
}

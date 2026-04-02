use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::style::Theme;

/// Input state for the TUI.
pub struct InputState {
    pub text: String,
    pub cursor_pos: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.remove(prev);
            self.cursor_pos = prev;
        }
    }

    pub fn clear(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.cursor_pos = 0;
        text
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.text[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos += self.text[self.cursor_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
        }
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the input area with `❯ ` prompt and separator line above.
pub fn render_input(frame: &mut Frame, area: Rect, state: &InputState) {
    // Separator line at the top of the input area.
    if area.height >= 2 {
        let sep_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        let sep = Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Theme::separator(),
        )));
        frame.render_widget(sep, sep_area);
    }

    // Input line below the separator.
    let input_y = if area.height >= 2 { area.y + 1 } else { area.y };
    let input_area = Rect {
        x: area.x,
        y: input_y,
        width: area.width,
        height: 1,
    };

    let line = Line::from(vec![
        Span::styled("❯ ", Theme::input_prompt()),
        Span::styled(state.text.as_str(), Theme::input_area()),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, input_area);

    // Place cursor after the prompt prefix ("❯ " is 2 display columns).
    let cursor_x = area.x + 2 + state.cursor_pos as u16;
    let cursor_y = input_y;
    frame.set_cursor_position((cursor_x, cursor_y));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_insert() {
        let mut input = InputState::new();
        input.insert('h');
        input.insert('i');
        assert_eq!(input.text, "hi");
        assert_eq!(input.cursor_pos, 2);
    }

    #[test]
    fn test_input_backspace() {
        let mut input = InputState::new();
        input.insert('a');
        input.insert('b');
        input.backspace();
        assert_eq!(input.text, "a");
    }

    #[test]
    fn test_input_clear() {
        let mut input = InputState::new();
        input.insert('x');
        let text = input.clear();
        assert_eq!(text, "x");
        assert!(input.text.is_empty());
        assert_eq!(input.cursor_pos, 0);
    }

    #[test]
    fn test_input_move() {
        let mut input = InputState::new();
        input.insert('a');
        input.insert('b');
        input.move_left();
        assert_eq!(input.cursor_pos, 1);
        input.move_right();
        assert_eq!(input.cursor_pos, 2);
    }
}

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

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

/// Render the input area.
pub fn render_input(frame: &mut Frame, area: Rect, state: &InputState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .title(" Input ");

    let paragraph = Paragraph::new(state.text.as_str())
        .block(block)
        .style(Theme::input_area());

    frame.render_widget(paragraph, area);

    // Place cursor.
    let x = area.x + 1 + state.cursor_pos as u16;
    let y = area.y + 1;
    frame.set_cursor_position((x, y));
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

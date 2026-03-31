use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::chat::{render_chat, ChatMessage};
use crate::input::{render_input, InputState};
use crate::style::Theme;

/// Application state for the TUI.
pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: InputState,
    pub scroll: u16,
    pub should_quit: bool,
    pub status: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input: InputState::new(),
            scroll: 0,
            should_quit: false,
            status: "Ready".into(),
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the full application UI.
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),       // Chat area
            Constraint::Length(3),     // Input area
            Constraint::Length(1),     // Status bar
        ])
        .split(frame.area());

    // Chat messages.
    render_chat(frame, chunks[0], &app.messages, app.scroll);

    // Input area.
    render_input(frame, chunks[1], &app.input);

    // Status bar.
    let status = ratatui::widgets::Paragraph::new(format!(" {} ", app.status))
        .style(Theme::status_bar());
    frame.render_widget(status, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new() {
        let app = App::new();
        assert!(app.messages.is_empty());
        assert!(!app.should_quit);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn test_app_scroll() {
        let mut app = App::new();
        app.scroll_down();
        assert_eq!(app.scroll, 1);
        app.scroll_up();
        assert_eq!(app.scroll, 0);
        app.scroll_up(); // Should not go below 0.
        assert_eq!(app.scroll, 0);
    }
}

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::chat::{ChatMessage, render_chat};
use crate::input::{InputState, render_input};
use crate::style::Theme;
use crate::welcome::{WelcomeInfo, render_welcome};

/// Which screen the app is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Chat,
}

/// Application state for the TUI.
pub struct App {
    pub screen: Screen,
    pub messages: Vec<ChatMessage>,
    pub input: InputState,
    pub scroll: u16,
    pub should_quit: bool,
    pub welcome: WelcomeInfo,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Welcome,
            messages: Vec::new(),
            input: InputState::new(),
            scroll: 0,
            should_quit: false,
            welcome: WelcomeInfo::default(),
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Transition from Welcome to Chat screen.
    pub fn enter_chat(&mut self) {
        self.screen = Screen::Chat;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the full application UI.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Title bar at the top.
    let chunks = Layout::vertical([
        Constraint::Length(1), // Title bar
        Constraint::Min(5),    // Main content
        Constraint::Length(2), // Input area (separator + input line)
        Constraint::Length(1), // Footer
    ])
    .split(area);

    render_title_bar(frame, chunks[0]);

    match app.screen {
        Screen::Welcome => {
            render_welcome(frame, chunks[1], &app.welcome);
        }
        Screen::Chat => {
            render_chat(frame, chunks[1], &app.messages, app.scroll);
        }
    }

    render_input(frame, chunks[2], &app.input);
    render_footer(frame, chunks[3], &app.welcome.model);
}

/// Render the title bar: `────── CCX-RS v0.1.0 ──────...`.
fn render_title_bar(frame: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let title = format!(" CCX-RS v{version} ");
    let total_width = area.width as usize;

    // Build the dashed title line.
    let title_len = title.len();
    let left_dashes = 6;
    let right_dashes = total_width.saturating_sub(left_dashes + title_len);

    let line = Line::from(vec![
        Span::styled("─".repeat(left_dashes), Theme::separator()),
        Span::styled(title, Theme::title_bar()),
        Span::styled("─".repeat(right_dashes), Theme::separator()),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render the footer: left = `? for shortcuts`, right = `● model · /effort`.
fn render_footer(frame: &mut Frame, area: Rect, model: &str) {
    // Short model display name.
    let model_short = if model.len() > 20 {
        &model[..20]
    } else {
        model
    };

    // Left side.
    let left = Paragraph::new(Line::from(Span::styled("? for shortcuts", Theme::footer())));

    // Right side.
    let right = Paragraph::new(Line::from(vec![
        Span::styled("● ", Theme::footer_accent()),
        Span::styled(format!("{model_short} · /effort"), Theme::footer()),
    ]))
    .alignment(Alignment::Right);

    // Render both sides into the same area.
    let halves =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    frame.render_widget(left, halves[0]);
    frame.render_widget(right, halves[1]);
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
        assert_eq!(app.screen, Screen::Welcome);
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

    #[test]
    fn test_enter_chat() {
        let mut app = App::new();
        assert_eq!(app.screen, Screen::Welcome);
        app.enter_chat();
        assert_eq!(app.screen, Screen::Chat);
    }
}

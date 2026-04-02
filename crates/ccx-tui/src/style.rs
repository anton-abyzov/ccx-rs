use ratatui::style::{Color, Modifier, Style};

/// Rich violet accent color.
pub const ACCENT: Color = Color::Rgb(138, 99, 210);

/// Pet/buddy orange-brown color.
pub const PET: Color = Color::Rgb(190, 110, 60);

/// Muted border color.
pub const BORDER: Color = Color::Rgb(68, 68, 68);

/// Green for model indicator and status.
pub const GREEN: Color = Color::Rgb(80, 200, 120);

/// Dark background for panels.
pub const PANEL_BG: Color = Color::Reset;

/// Color palette for the TUI.
pub struct Theme;

impl Theme {
    /// User message prefix and text (cyan `❯` prompt).
    pub fn user_message() -> Style {
        Style::default().fg(Color::Cyan)
    }

    /// Assistant message text (white).
    pub fn assistant_message() -> Style {
        Style::default().fg(Color::White)
    }

    /// Tool name header (yellow, bold).
    pub fn tool_name() -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    /// Tool output text (gray).
    pub fn tool_output() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Error text (red).
    pub fn error() -> Style {
        Style::default().fg(Color::Red)
    }

    /// Footer bar text.
    pub fn footer() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Footer accent (model indicator dot — green).
    pub fn footer_accent() -> Style {
        Style::default().fg(GREEN)
    }

    /// Input prompt (`❯`) in orange.
    pub fn input_prompt() -> Style {
        Style::default().fg(ACCENT)
    }

    /// Input text (white).
    pub fn input_area() -> Style {
        Style::default().fg(Color::White)
    }

    /// Panel border (subtle gray).
    pub fn border() -> Style {
        Style::default().fg(BORDER)
    }

    /// Muted separator lines.
    pub fn separator() -> Style {
        Style::default().fg(BORDER)
    }

    /// Thinking indicator (magenta, italic).
    pub fn thinking() -> Style {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC)
    }

    /// Welcome title text.
    pub fn welcome_title() -> Style {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    /// Welcome info text (model, dir, tools).
    pub fn welcome_info() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Pet/buddy color (orange-brown).
    pub fn pet() -> Style {
        Style::default().fg(PET)
    }

    /// Model info text (green).
    pub fn model_info() -> Style {
        Style::default().fg(GREEN)
    }

    /// Path info text (dim gray).
    pub fn path_info() -> Style {
        Style::default().fg(Color::Rgb(100, 100, 100))
    }

    /// Right panel heading text (orange bold).
    pub fn panel_heading() -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }

    /// Right panel body text.
    pub fn panel_body() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    /// Title bar style.
    pub fn title_bar() -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_styles() {
        // Verify styles are valid (don't panic).
        let _ = Theme::user_message();
        let _ = Theme::assistant_message();
        let _ = Theme::tool_name();
        let _ = Theme::error();
        let _ = Theme::footer();
        let _ = Theme::footer_accent();
        let _ = Theme::input_prompt();
        let _ = Theme::welcome_title();
        let _ = Theme::panel_heading();
        let _ = Theme::pet();
        let _ = Theme::model_info();
        let _ = Theme::path_info();
    }

    #[test]
    fn test_accent_color() {
        assert!(matches!(ACCENT, Color::Rgb(138, 99, 210)));
    }
}

use ratatui::style::{Color, Modifier, Style};

/// Color palette for the TUI.
pub struct Theme;

impl Theme {
    pub fn user_message() -> Style {
        Style::default().fg(Color::White)
    }

    pub fn assistant_message() -> Style {
        Style::default().fg(Color::Cyan)
    }

    pub fn tool_name() -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tool_output() -> Style {
        Style::default().fg(Color::Gray)
    }

    pub fn error() -> Style {
        Style::default().fg(Color::Red)
    }

    pub fn status_bar() -> Style {
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
    }

    pub fn input_area() -> Style {
        Style::default().fg(Color::White)
    }

    pub fn border() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn thinking() -> Style {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC)
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
        let _ = Theme::status_bar();
    }
}

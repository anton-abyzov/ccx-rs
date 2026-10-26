use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::style::Theme;

/// Data displayed on the welcome screen.
pub struct WelcomeInfo {
    pub model: String,
    pub auth_source: String,
    pub email: Option<String>,
    pub cwd: String,
    pub tool_count: usize,
    pub provider: String,
}

impl Default for WelcomeInfo {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            auth_source: "API Key".into(),
            email: None,
            cwd: "~/".into(),
            tool_count: 11,
            provider: "anthropic".into(),
        }
    }
}

pub fn init_file_for_provider(provider: &str) -> &'static str {
    if provider == "anthropic" {
        "CLAUDE.md"
    } else {
        "CCX.md"
    }
}

pub fn getting_started_tips(provider: &str, auth_source: &str) -> Vec<String> {
    let auth_tip = match (provider, auth_source) {
        ("anthropic", "Not authenticated") => " Run /login or add an API key",
        ("anthropic", source) if source.starts_with("Claude ") => {
            " Run /login again if access expires"
        }
        ("anthropic", _) => " Run /login to use Claude auth",
        ("openrouter", _) => " OpenRouter sessions use API keys",
        ("openai", _) => " OpenAI sessions use API keys",
        _ => " Switch providers with /model",
    };

    vec![
        " Type a message to start coding".to_string(),
        format!(
            " Run /init to create a {}",
            init_file_for_provider(provider)
        ),
        auth_tip.to_string(),
        " Use /model to switch providers".to_string(),
        " Type /help for all commands".to_string(),
        " Press Tab after / to autocomplete".to_string(),
        " Ctrl+C to quit".to_string(),
    ]
}

/// Render the welcome screen into the given area.
pub fn render_welcome(frame: &mut Frame, area: Rect, info: &WelcomeInfo) {
    // Split into left (60%) and right (40%) panels.
    let panels =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).split(area);

    render_left_panel(frame, panels[0], info);
    render_right_panel(frame, panels[1], &info.provider, &info.auth_source);
}

fn render_left_panel(frame: &mut Frame, area: Rect, info: &WelcomeInfo) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build content lines with vertical centering.
    let pet = vec![
        Line::from(""),
        Line::from(Span::styled("Welcome back!", Theme::welcome_title())),
        Line::from(""),
        Line::from(Span::styled("    ▄▄▄▄▄▄▄▄▄", Theme::pet())),
        Line::from(vec![
            Span::styled("   █", Theme::pet()),
            Span::styled("  ●   ●", Theme::welcome_title()),
            Span::styled("  █", Theme::pet()),
        ]),
        Line::from(vec![
            Span::styled("   █", Theme::pet()),
            Span::styled("    ▽", Theme::welcome_title()),
            Span::styled("    █", Theme::pet()),
        ]),
        Line::from(Span::styled("    ▀▀▀▀▀▀▀▀▀", Theme::pet())),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Theme::welcome_info()),
            Span::styled(&info.model, Theme::model_info()),
            Span::styled(" · ", Theme::welcome_info()),
            Span::styled(&info.auth_source, Theme::welcome_info()),
        ]),
        Line::from(vec![
            Span::styled("  ", Theme::welcome_info()),
            Span::styled(info.email.as_deref().unwrap_or(""), Theme::welcome_info()),
        ]),
        Line::from(vec![
            Span::styled("  ", Theme::welcome_info()),
            Span::styled(&info.cwd, Theme::path_info()),
        ]),
        Line::from(vec![
            Span::styled("  Tools: ", Theme::welcome_info()),
            Span::styled(info.tool_count.to_string(), Theme::welcome_info()),
        ]),
    ];

    // Calculate vertical offset to center content.
    let content_height = pet.len() as u16;
    let v_offset = if inner.height > content_height {
        (inner.height - content_height) / 2
    } else {
        0
    };

    // Add top padding.
    let mut lines = vec![Line::from(""); v_offset as usize];
    lines.extend(pet);

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);
}

fn render_right_panel(frame: &mut Frame, area: Rect, provider: &str, auth_source: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sep_width = inner.width.saturating_sub(2) as usize;
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Tips for getting started",
            Theme::panel_heading(),
        )),
    ];
    lines.extend(
        getting_started_tips(provider, auth_source)
            .into_iter()
            .map(|tip| Line::from(Span::styled(tip, Theme::panel_body()))),
    );
    lines.extend([
        Line::from(""),
        Line::from(Span::styled("─".repeat(sep_width), Theme::separator())),
        Line::from(""),
        Line::from(Span::styled(" Recent activity", Theme::panel_heading())),
        Line::from(Span::styled(" No recent activity", Theme::panel_body())),
    ]);

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_info_default() {
        let info = WelcomeInfo::default();
        assert_eq!(info.model, "claude-sonnet-4-6");
        assert_eq!(info.tool_count, 11);
    }

    #[test]
    fn test_tips_for_claude_subscription() {
        let tips = getting_started_tips("anthropic", "Claude Max");
        assert!(tips.iter().any(|tip| tip.contains("CLAUDE.md")));
        assert!(tips.iter().any(|tip| tip.contains("access expires")));
    }

    #[test]
    fn test_tips_for_unauthenticated_session() {
        let tips = getting_started_tips("anthropic", "Not authenticated");
        assert!(tips.iter().any(|tip| tip.contains("/login")));
        assert!(tips.iter().any(|tip| tip.contains("API key")));
    }

    #[test]
    fn test_tips_for_openrouter_session() {
        let tips = getting_started_tips("openrouter", "OpenRouter");
        assert!(tips.iter().any(|tip| tip.contains("CCX.md")));
        assert!(tips.iter().any(|tip| tip.contains("API keys")));
    }
}

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::style::Theme;

/// Data displayed on the welcome screen.
pub struct WelcomeInfo {
    pub model: String,
    pub auth_source: String,
    pub cwd: String,
    pub tool_count: usize,
}

impl Default for WelcomeInfo {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            auth_source: "API Key".into(),
            cwd: "~/".into(),
            tool_count: 11,
        }
    }
}

/// Render the welcome screen into the given area.
pub fn render_welcome(frame: &mut Frame, area: Rect, info: &WelcomeInfo) {
    // Split into left (60%) and right (40%) panels.
    let panels = Layout::horizontal([
        Constraint::Percentage(55),
        Constraint::Percentage(45),
    ])
    .split(area);

    render_left_panel(frame, panels[0], info);
    render_right_panel(frame, panels[1]);
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
        Line::from(Span::styled("      ╱▔▔▔▔▔╲", Theme::welcome_title())),
        Line::from(Span::styled("     ╱ ●   ● ╲", Theme::welcome_title())),
        Line::from(Span::styled("    ╱    ▽    ╲", Theme::welcome_title())),
        Line::from(Span::styled("   ╱___________╲", Theme::welcome_title())),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Theme::welcome_info()),
            Span::styled(&info.model, Theme::welcome_info()),
            Span::styled(" · ", Theme::welcome_info()),
            Span::styled(&info.auth_source, Theme::welcome_info()),
        ]),
        Line::from(vec![
            Span::styled("  ", Theme::welcome_info()),
            Span::styled(&info.cwd, Theme::welcome_info()),
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

fn render_right_panel(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            " Tips for getting started",
            Theme::panel_heading(),
        )),
        Line::from(Span::styled(
            " Type a message to start coding",
            Theme::panel_body(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "─".repeat(inner.width.saturating_sub(2) as usize),
            Theme::separator(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " Recent activity",
            Theme::panel_heading(),
        )),
        Line::from(Span::styled(
            " No recent activity",
            Theme::panel_body(),
        )),
    ];

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
}

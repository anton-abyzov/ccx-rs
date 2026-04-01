use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::style::Theme;

/// A chat message for display.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    Tool,
    Error,
}

/// Render chat messages into the given area.
pub fn render_chat(frame: &mut Frame, area: Rect, messages: &[ChatMessage], scroll: u16) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in messages {
        match msg.role {
            ChatRole::User => {
                // User: `❯ text` in cyan.
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("❯ ", Theme::user_message()),
                        Span::styled(line.to_string(), Theme::user_message()),
                    ]));
                }
            }
            ChatRole::Assistant => {
                // Assistant: `● text` in white.
                let mut first = true;
                for line in msg.content.lines() {
                    let prefix = if first { "● " } else { "  " };
                    first = false;
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Theme::assistant_message()),
                        Span::styled(line.to_string(), Theme::assistant_message()),
                    ]));
                }
                if msg.content.is_empty() {
                    lines.push(Line::from(Span::styled("● ", Theme::assistant_message())));
                }
            }
            ChatRole::Tool => {
                // Tool: `⚙ ToolName` in yellow with detail below.
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("⚙ ", Theme::tool_name()),
                        Span::styled(line.to_string(), Theme::tool_output()),
                    ]));
                }
            }
            ChatRole::Error => {
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("✗ ", Theme::error()),
                        Span::styled(line.to_string(), Theme::error()),
                    ]));
                }
            }
        }

        // Separator between messages.
        let sep_width = area.width.saturating_sub(4) as usize;
        if sep_width > 0 {
            lines.push(Line::from(Span::styled(
                "─".repeat(sep_width),
                Theme::separator(),
            )));
        }
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: "Hello".into(),
        };
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "Hello");
    }
}

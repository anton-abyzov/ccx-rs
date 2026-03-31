use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
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
    let lines: Vec<Line> = messages
        .iter()
        .flat_map(|msg| {
            let (prefix, style) = match msg.role {
                ChatRole::User => ("> ", Theme::user_message()),
                ChatRole::Assistant => ("  ", Theme::assistant_message()),
                ChatRole::Tool => ("  [tool] ", Theme::tool_output()),
                ChatRole::Error => ("  ERROR: ", Theme::error()),
            };

            let mut result: Vec<Line> = Vec::new();
            for line in msg.content.lines() {
                result.push(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(line.to_string(), style),
                ]));
            }
            if result.is_empty() {
                result.push(Line::from(Span::styled(prefix, style)));
            }
            result.push(Line::from("")); // blank line between messages
            result
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Theme::border())
        .title(" Chat ");

    let paragraph = Paragraph::new(lines)
        .block(block)
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

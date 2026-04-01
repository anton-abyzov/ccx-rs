use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;

pub struct SendMessageTool;

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Send a message to a teammate or broadcast to all team members"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient agent name, or '*' for broadcast"
                },
                "message": {
                    "type": "string",
                    "description": "The message content"
                },
                "summary": {
                    "type": "string",
                    "description": "Optional short summary of the message"
                }
            },
            "required": ["to", "message"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let to = input["to"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("'to' is required".into()))?
            .trim()
            .to_string();

        if to.is_empty() {
            return Err(ToolError::InvalidInput("'to' must not be empty".into()));
        }

        let message = input["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("'message' is required".into()))?
            .to_string();

        let summary = input["summary"].as_str().map(|s| s.to_string());

        let base = meta_helpers::resolve_base_dir(ctx)?;
        let team_name = meta_helpers::current_team(&base)?;
        let msg_dir = meta_helpers::messages_dir(&base, &team_name);
        std::fs::create_dir_all(&msg_dir).map_err(ToolError::Io)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = json!({
            "to": to,
            "message": message,
            "summary": summary,
            "timestamp": timestamp
        });

        let line = format!("{}\n", entry);
        let file_path = msg_dir.join(format!("{to}.jsonl"));

        // Append to the recipient's message file.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(ToolError::Io)?;
        file.write_all(line.as_bytes()).map_err(ToolError::Io)?;

        Ok(ToolResult {
            content: format!("Message sent to '{to}' in team '{team_name}'."),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_helpers::{cleanup, test_ctx};
    use crate::TeamCreateTool;

    async fn setup_team(ctx: &ToolContext) {
        TeamCreateTool
            .execute(
                json!({"team_name": "msg-test", "description": "Message testing"}),
                ctx,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_send_basic() {
        let (ctx, base) = test_ctx("send_basic");
        setup_team(&ctx).await;

        let result = SendMessageTool
            .execute(
                json!({"to": "agent-a", "message": "Hello there", "summary": "Greeting"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("agent-a"));

        let file = base.join("teams/msg-test/messages/agent-a.jsonl");
        assert!(file.exists());
        let content = std::fs::read_to_string(&file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed["to"], "agent-a");
        assert_eq!(parsed["message"], "Hello there");
        assert_eq!(parsed["summary"], "Greeting");

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_send_multiple_appends() {
        let (ctx, base) = test_ctx("send_multi");
        setup_team(&ctx).await;

        SendMessageTool
            .execute(json!({"to": "bot", "message": "First"}), &ctx)
            .await
            .unwrap();
        SendMessageTool
            .execute(json!({"to": "bot", "message": "Second"}), &ctx)
            .await
            .unwrap();

        let file = base.join("teams/msg-test/messages/bot.jsonl");
        let content = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_send_no_team() {
        let (ctx, base) = test_ctx("send_no_team");
        let err = SendMessageTool
            .execute(json!({"to": "x", "message": "y"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
        cleanup(&base);
    }

    #[tokio::test]
    async fn test_send_missing_to() {
        let (ctx, base) = test_ctx("send_no_to");
        let err = SendMessageTool
            .execute(json!({"message": "y"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = SendMessageTool;
        assert_eq!(tool.name(), "SendMessage");
        let schema = tool.input_schema();
        let req = schema["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "to"));
        assert!(req.iter().any(|v| v == "message"));
    }
}

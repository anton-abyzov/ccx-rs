use std::time::Duration;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return its output"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default 120000)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("command is required".into()))?;
        let timeout_ms = input["timeout"].as_u64().unwrap_or(120_000);

        let output = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .current_dir(&ctx.working_dir)
                .output(),
        )
        .await
        .map_err(|_| ToolError::Timeout(timeout_ms))?
        .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let is_error = !output.status.success();

        let content = if is_error {
            format!(
                "Exit code {}\n{stdout}{stderr}",
                output.status.code().unwrap_or(-1)
            )
        } else {
            format!("{stdout}{stderr}")
        };

        Ok(ToolResult { content, is_error })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_bash_echo() {
        let tool = BashTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"command": "echo hello"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.trim().contains("hello"));
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_bash_error() {
        let tool = BashTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"command": "false"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_bash_missing_command() {
        let tool = BashTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let err = tool.execute(json!({}), &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }
}

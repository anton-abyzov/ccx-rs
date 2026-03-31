use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        "Launch a sub-agent to handle a complex task autonomously"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "description": {
                    "type": "string",
                    "description": "Short (3-5 word) description of the task"
                }
            },
            "required": ["prompt", "description"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let prompt = input["prompt"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("prompt is required".into()))?;
        let description = input["description"]
            .as_str()
            .unwrap_or("sub-agent task");

        // Build a sub-agent with its own API client and limited tools.
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| ToolError::Execution("ANTHROPIC_API_KEY not set".into()))?;

        let model = std::env::var("CCX_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".into());

        let client = ccx_api::ClaudeClient::new(&api_key, &model);

        // Sub-agent gets a limited tool set: Bash, Read, Write, Glob, Grep.
        let mut registry = ccx_core::ToolRegistry::new();
        registry.register(Box::new(crate::BashTool));
        registry.register(Box::new(crate::FileReadTool));
        registry.register(Box::new(crate::FileWriteTool));
        registry.register(Box::new(crate::GlobTool));
        registry.register(Box::new(crate::GrepTool));

        let system = format!(
            "You are a sub-agent performing a focused task. \
             Working directory: {}. Task description: {description}. \
             Complete the task and return a concise result.",
            ctx.working_dir.display()
        );

        let sub_ctx = ccx_core::ToolContext::new(ctx.working_dir.clone());
        let mut agent = ccx_core::AgentLoop::new(client, registry, sub_ctx, system);
        agent.set_max_turns(20);

        let mut cb = ccx_core::NoopCallback;
        match agent.send_message(prompt, &mut cb).await {
            Ok(result) => Ok(ToolResult {
                content: result,
                is_error: false,
            }),
            Err(e) => Ok(ToolResult {
                content: format!("Sub-agent error: {e}"),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("/tmp"))
    }

    #[test]
    fn test_agent_tool_schema() {
        let tool = AgentTool;
        assert_eq!(tool.name(), "Agent");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "prompt"));
        assert!(required.iter().any(|v| v == "description"));
    }

    #[tokio::test]
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let err = tool.execute(json!({}), &test_ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_agent_tool_no_api_key() {
        // Remove the API key for this test.
        let had_key = std::env::var("ANTHROPIC_API_KEY").ok();
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        let tool = AgentTool;
        let result = tool
            .execute(
                json!({"prompt": "test", "description": "test"}),
                &test_ctx(),
            )
            .await;

        // Restore key if it was set.
        if let Some(key) = had_key {
            unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
        }

        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}

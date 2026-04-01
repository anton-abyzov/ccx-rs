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
        "Launch a sub-agent to handle a complex task autonomously. \
         Spawns a separate process for context isolation when possible."
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
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "If true and tmux available, spawn in a new pane",
                    "default": false
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
        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        // Try process-based agent first.
        match run_process_agent(prompt, description, ctx, run_in_background).await {
            Ok(result) => return Ok(result),
            Err(_) => {
                // Fall back to in-process agent.
            }
        }

        run_inprocess_agent(prompt, description, ctx).await
    }
}

/// Try to spawn a sub-agent as a separate `ccx chat` process.
async fn run_process_agent(
    prompt: &str,
    description: &str,
    ctx: &ToolContext,
    run_in_background: bool,
) -> Result<ToolResult, ToolError> {
    let ccx_bin = find_ccx_binary()?;

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| ToolError::Execution("ANTHROPIC_API_KEY not set".into()))?;

    let model = std::env::var("CCX_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-6".into());

    // If background mode and tmux is available, spawn in a new pane.
    if run_in_background && std::env::var("TMUX").is_ok() {
        return spawn_tmux_agent(&ccx_bin, prompt, &api_key, &model, ctx, description).await;
    }

    // Foreground: spawn process, capture output.
    let output = tokio::process::Command::new(&ccx_bin)
        .arg("chat")
        .arg("--prompt")
        .arg(prompt)
        .arg("--dangerously-skip-permissions")
        .arg("--model")
        .arg(&model)
        .env("ANTHROPIC_API_KEY", &api_key)
        .current_dir(&ctx.working_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to spawn ccx process: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let content = if stdout.trim().is_empty() {
            format!("[Agent '{description}' completed]\n{stderr}")
        } else {
            stdout
        };
        Ok(ToolResult {
            content,
            is_error: false,
        })
    } else {
        Ok(ToolResult {
            content: format!(
                "Sub-agent process exited with {}\nstdout: {}\nstderr: {}",
                output.status, stdout, stderr
            ),
            is_error: true,
        })
    }
}

/// Spawn the agent in a tmux pane for visual monitoring.
async fn spawn_tmux_agent(
    ccx_bin: &str,
    prompt: &str,
    api_key: &str,
    model: &str,
    ctx: &ToolContext,
    description: &str,
) -> Result<ToolResult, ToolError> {
    // Escape single quotes in prompt for shell.
    let escaped_prompt = prompt.replace('\'', "'\\''");
    let cmd = format!(
        "cd '{}' && ANTHROPIC_API_KEY='{}' '{}' chat --prompt '{}' --dangerously-skip-permissions --model '{}'",
        ctx.working_dir.display(),
        api_key.replace('\'', "'\\''"),
        ccx_bin,
        escaped_prompt,
        model
    );

    let status = tokio::process::Command::new("tmux")
        .args(["split-window", "-h", &cmd])
        .status()
        .await
        .map_err(|e| ToolError::Execution(format!("tmux split-window failed: {e}")))?;

    if status.success() {
        Ok(ToolResult {
            content: format!("Agent '{description}' launched in tmux pane."),
            is_error: false,
        })
    } else {
        Err(ToolError::Execution(
            "tmux split-window failed".into(),
        ))
    }
}

/// Find the ccx binary: same directory as current executable, or PATH.
fn find_ccx_binary() -> Result<String, ToolError> {
    // Check next to the current executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let ccx = dir.join("ccx");
            if ccx.exists() {
                return Ok(ccx.to_string_lossy().to_string());
            }
        }
    }

    // Check PATH via `which`.
    let output = std::process::Command::new("which")
        .arg("ccx")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
        _ => {}
    }

    Err(ToolError::Execution(
        "ccx binary not found — falling back to in-process agent".into(),
    ))
}

/// Fall back to running the agent in-process (same as original implementation).
async fn run_inprocess_agent(
    prompt: &str,
    description: &str,
    ctx: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| ToolError::Execution("ANTHROPIC_API_KEY not set".into()))?;

    let model = std::env::var("CCX_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-6".into());

    let client = ccx_api::ApiClient::Claude(ccx_api::ClaudeClient::new(&api_key, &model));

    // Sub-agent gets a broad tool set.
    let mut registry = ccx_core::ToolRegistry::new();
    registry.register(Box::new(crate::BashTool));
    registry.register(Box::new(crate::FileReadTool));
    registry.register(Box::new(crate::FileWriteTool));
    registry.register(Box::new(crate::FileEditTool));
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
    agent.set_max_turns(100);

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
        // New field.
        assert!(schema["properties"]["run_in_background"].is_object());
    }

    #[tokio::test]
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let err = tool.execute(json!({}), &test_ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[test]
    fn test_find_ccx_binary() {
        // Just test that it doesn't panic. It may or may not find ccx.
        let _ = find_ccx_binary();
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

        // Should fail (process-based fails without key, in-process also fails).
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}

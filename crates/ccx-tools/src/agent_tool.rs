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
                "subagent_type": {
                    "type": "string",
                    "description": "Specialized agent type: general-purpose, Explore, Plan"
                },
                "model": {
                    "type": "string",
                    "description": "Override model for this agent (e.g. claude-opus-4-6)"
                },
                "name": {
                    "type": "string",
                    "description": "Named agent, addressable via SendMessage"
                },
                "team_name": {
                    "type": "string",
                    "description": "Team name for the spawned agent to join"
                },
                "mode": {
                    "type": "string",
                    "description": "Permission mode: bypassPermissions, plan, default, etc."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "If true, spawn async and return agent ID immediately",
                    "default": false
                },
                "isolation": {
                    "type": "string",
                    "description": "Isolation mode: 'worktree' for git worktree isolation"
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
        let description = input["description"].as_str().unwrap_or("sub-agent task");
        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);
        let model_override = input["model"].as_str().map(|s| s.to_string());
        let agent_name = input["name"].as_str().map(|s| s.to_string());
        let _subagent_type = input["subagent_type"].as_str().unwrap_or("general-purpose");
        let _team_name = input["team_name"].as_str().map(|s| s.to_string());
        let _mode = input["mode"].as_str().map(|s| s.to_string());
        let _isolation = input["isolation"].as_str().map(|s| s.to_string());

        let opts = AgentOpts {
            model_override,
            agent_name,
        };

        // Try process-based agent first.
        match run_process_agent(prompt, description, ctx, run_in_background, &opts).await {
            Ok(result) => return Ok(result),
            Err(_) => {
                // Fall back to in-process agent.
            }
        }

        run_inprocess_agent(prompt, description, ctx, &opts).await
    }
}

/// Options parsed from the Agent tool input.
struct AgentOpts {
    model_override: Option<String>,
    agent_name: Option<String>,
}

/// Try to spawn a sub-agent as a separate `ccx chat` process.
async fn run_process_agent(
    prompt: &str,
    description: &str,
    ctx: &ToolContext,
    run_in_background: bool,
    opts: &AgentOpts,
) -> Result<ToolResult, ToolError> {
    let ccx_bin = find_ccx_binary()?;

    let api_key = resolve_api_key(ctx)?;
    let provider = &ctx.provider;

    let model = opts
        .model_override
        .clone()
        .or_else(|| {
            if !ctx.model.is_empty() {
                Some(ctx.model.clone())
            } else {
                None
            }
        })
        .or_else(|| std::env::var("CCX_MODEL").ok())
        .unwrap_or_else(|| "claude-sonnet-4-6".into());

    // If background mode and tmux is available, spawn in a new pane.
    if run_in_background && std::env::var("TMUX").is_ok() {
        return spawn_tmux_agent(&ccx_bin, prompt, &api_key, &model, provider, ctx, description)
            .await;
    }

    // Background mode without tmux: spawn async and return agent ID.
    if run_in_background {
        let agent_id = opts
            .agent_name
            .clone()
            .unwrap_or_else(|| format!("agent-{}", std::process::id()));
        let ccx_bin = ccx_bin.clone();
        let prompt = prompt.to_string();
        let model = model.clone();
        let api_key = api_key.clone();
        let provider = provider.clone();
        let working_dir = ctx.working_dir.clone();

        tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new(&ccx_bin);
            cmd.arg("chat")
                .arg("--prompt")
                .arg(&prompt)
                .arg("--dangerously-skip-permissions")
                .arg("--model")
                .arg(&model)
                .current_dir(&working_dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            apply_provider_env(&mut cmd, &provider, &api_key);
            let _ = cmd.output().await;
        });

        return Ok(ToolResult {
            content: format!("Agent '{description}' launched in background (id: {agent_id})"),
            is_error: false,
        });
    }

    // Foreground: spawn process, capture output.
    let mut cmd = tokio::process::Command::new(&ccx_bin);
    cmd.arg("chat")
        .arg("--prompt")
        .arg(prompt)
        .arg("--dangerously-skip-permissions")
        .arg("--model")
        .arg(&model)
        .current_dir(&ctx.working_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    apply_provider_env(&mut cmd, provider, &api_key);

    let output = cmd
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
    provider: &str,
    ctx: &ToolContext,
    description: &str,
) -> Result<ToolResult, ToolError> {
    // Escape single quotes in prompt for shell.
    let escaped_prompt = prompt.replace('\'', "'\\''");
    let escaped_key = api_key.replace('\'', "'\\''");
    let (env_var, provider_args) = match provider {
        "openrouter" => (
            format!("OPENROUTER_API_KEY='{escaped_key}'"),
            format!(" --provider openrouter"),
        ),
        _ => (format!("ANTHROPIC_API_KEY='{escaped_key}'"), String::new()),
    };
    let cmd = format!(
        "cd '{}' && {} '{}' chat --prompt '{}' --dangerously-skip-permissions --model '{}'{}",
        ctx.working_dir.display(),
        env_var,
        ccx_bin,
        escaped_prompt,
        model,
        provider_args
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
        Err(ToolError::Execution("tmux split-window failed".into()))
    }
}

/// Find the ccx binary: same directory as current executable, or PATH.
fn find_ccx_binary() -> Result<String, ToolError> {
    // Check next to the current executable.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let ccx = dir.join("ccx");
        if ccx.exists() {
            return Ok(ccx.to_string_lossy().to_string());
        }
    }

    // Check PATH via `which`.
    let output = std::process::Command::new("which").arg("ccx").output();

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
    opts: &AgentOpts,
) -> Result<ToolResult, ToolError> {
    let api_key = resolve_api_key(ctx)?;

    let model = opts
        .model_override
        .clone()
        .or_else(|| {
            if !ctx.model.is_empty() {
                Some(ctx.model.clone())
            } else {
                None
            }
        })
        .or_else(|| std::env::var("CCX_MODEL").ok())
        .unwrap_or_else(|| "claude-sonnet-4-6".into());

    let client = match ctx.provider.as_str() {
        "openrouter" => {
            ccx_api::ApiClient::OpenAi(ccx_api::OpenAiClient::openrouter(&api_key, &model))
        }
        _ => ccx_api::ApiClient::Claude(ccx_api::ClaudeClient::new(&api_key, &model)),
    };

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

/// Resolve the API key from ToolContext or environment, respecting the provider.
fn resolve_api_key(ctx: &ToolContext) -> Result<String, ToolError> {
    // Prefer key from context (set by parent session).
    if !ctx.api_key.is_empty() {
        return Ok(ctx.api_key.clone());
    }
    // Fall back to environment variables based on provider.
    match ctx.provider.as_str() {
        "openrouter" => std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| ToolError::Execution("OPENROUTER_API_KEY not set".into())),
        _ => std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| ToolError::Execution("ANTHROPIC_API_KEY not set".into())),
    }
}

/// Set the correct env var and provider flag on a Command based on provider.
fn apply_provider_env(cmd: &mut tokio::process::Command, provider: &str, api_key: &str) {
    match provider {
        "openrouter" => {
            cmd.env("OPENROUTER_API_KEY", api_key);
            cmd.arg("--provider").arg("openrouter");
        }
        _ => {
            cmd.env("ANTHROPIC_API_KEY", api_key);
        }
    };
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
        let props = &schema["properties"];
        assert!(props["run_in_background"].is_object());
        assert!(props["subagent_type"].is_object());
        assert!(props["model"].is_object());
        assert!(props["name"].is_object());
        assert!(props["team_name"].is_object());
        assert!(props["mode"].is_object());
        assert!(props["isolation"].is_object());
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
        // Remove both API keys for this test.
        let had_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let had_or_key = std::env::var("OPENROUTER_API_KEY").ok();
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENROUTER_API_KEY");
        }

        let tool = AgentTool;
        let result = tool
            .execute(
                json!({"prompt": "test", "description": "test"}),
                &test_ctx(),
            )
            .await;

        // Restore keys if they were set.
        if let Some(key) = had_key {
            unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
        }
        if let Some(key) = had_or_key {
            unsafe { std::env::set_var("OPENROUTER_API_KEY", key) };
        }

        // Should fail (process-based fails without key, in-process also fails).
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }

    #[test]
    fn test_resolve_api_key_from_context() {
        let mut ctx = test_ctx();
        ctx.provider = "openrouter".to_string();
        ctx.api_key = "test-key-123".to_string();
        let key = resolve_api_key(&ctx).unwrap();
        assert_eq!(key, "test-key-123");
    }

    #[test]
    fn test_resolve_api_key_anthropic_fallback() {
        let had_key = std::env::var("ANTHROPIC_API_KEY").ok();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "env-key-456") };

        let ctx = test_ctx(); // provider defaults to "anthropic", api_key empty
        let key = resolve_api_key(&ctx).unwrap();
        assert_eq!(key, "env-key-456");

        if let Some(k) = had_key {
            unsafe { std::env::set_var("ANTHROPIC_API_KEY", k) };
        } else {
            unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        }
    }
}

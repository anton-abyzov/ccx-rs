use std::collections::HashMap;
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
                "description": {
                    "type": "string",
                    "description": "Short description of what the command does"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default 120000, max 600000)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Run in background and return immediately"
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

        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(120_000)
            .min(600_000);

        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        // Build the shell command, optionally wrapped in a sandbox.
        let (program, cmd_args) = if ctx.sandboxed {
            let sandbox = ccx_sandbox::create_sandbox();
            let config = ccx_sandbox::SandboxConfig {
                enabled: true,
                allow_read: vec!["/".into()],
                allow_write: vec![
                    ctx.working_dir.to_string_lossy().into_owned(),
                    "/tmp".into(),
                    std::env::var("HOME").unwrap_or_default(),
                ],
                allow_network: true,
            };
            match sandbox.wrap_command(command, &ctx.working_dir, &config) {
                Ok(wrapped) if !wrapped.is_empty() => {
                    let prog = wrapped[0].clone();
                    let args = wrapped[1..].to_vec();
                    (prog, args)
                }
                _ => ("bash".into(), vec!["-c".into(), command.into()]),
            }
        } else {
            ("bash".into(), vec!["-c".into(), command.into()])
        };

        let mut cmd = tokio::process::Command::new(&program);
        for arg in &cmd_args {
            cmd.arg(arg);
        }
        cmd.current_dir(&ctx.working_dir)
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into()));

        // Propagate common environment variables if set.
        for var in &[
            "LANG",
            "LC_ALL",
            "SHELL",
            "USER",
            "EDITOR",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "GOPATH",
            "NODE_PATH",
            "ANTHROPIC_API_KEY",
        ] {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        // Merge any environment overrides from the context.
        for (k, v) in &ctx.env_vars {
            cmd.env(k, v);
        }

        // Don't inherit stdin — prevent interactive commands from hanging.
        cmd.stdin(std::process::Stdio::null());

        if run_in_background {
            // Spawn and return immediately.
            let child = cmd
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(ToolError::Io)?;

            let pid = child.id().unwrap_or(0);
            return Ok(ToolResult {
                content: format!("Background process started (pid: {pid})"),
                is_error: false,
            });
        }

        // Execute with timeout.
        let output = tokio::time::timeout(Duration::from_millis(timeout_ms), cmd.output())
            .await
            .map_err(|_| {
                ToolError::Timeout(timeout_ms)
            })?
            .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        let is_error = !output.status.success();

        // Build structured output with clear separation.
        let content = format_output(&stdout, &stderr, exit_code, is_error);

        Ok(ToolResult { content, is_error })
    }
}

/// Format command output with clear stdout/stderr separation.
fn format_output(stdout: &str, stderr: &str, exit_code: i32, is_error: bool) -> String {
    let mut parts = Vec::new();

    if is_error {
        parts.push(format!("Exit code: {exit_code}"));
    }

    if !stdout.is_empty() {
        parts.push(stdout.to_string());
    }

    if !stderr.is_empty() {
        if !stdout.is_empty() || is_error {
            parts.push(format!("stderr:\n{stderr}"));
        } else {
            // stderr-only output (warnings, progress, etc.)
            parts.push(stderr.to_string());
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("\n")
    }
}

/// Parse key=value environment variables from a JSON value.
pub fn parse_env_vars(value: &serde_json::Value) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                vars.insert(k.clone(), s.to_string());
            }
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("/tmp"))
    }

    #[tokio::test]
    async fn test_bash_echo() {
        let tool = BashTool;
        let result = tool
            .execute(json!({"command": "echo hello"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.trim().contains("hello"));
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_bash_error_exit_code() {
        let tool = BashTool;
        let result = tool
            .execute(json!({"command": "exit 42"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("42"));
    }

    #[tokio::test]
    async fn test_bash_stderr() {
        let tool = BashTool;
        let result = tool
            .execute(
                json!({"command": "echo out; echo err >&2"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("out"));
        assert!(result.content.contains("err"));
    }

    #[tokio::test]
    async fn test_bash_working_dir() {
        let dir = std::env::temp_dir().join("ccx_test_bash_cwd");
        let _ = std::fs::create_dir_all(&dir);

        let tool = BashTool;
        let ctx = ToolContext::new(dir.clone());
        let result = tool
            .execute(json!({"command": "pwd"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains(&dir.to_string_lossy().to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_bash_timeout() {
        let tool = BashTool;
        let err = tool
            .execute(
                json!({"command": "sleep 10", "timeout": 100}),
                &test_ctx(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Timeout(_)));
    }

    #[tokio::test]
    async fn test_bash_missing_command() {
        let tool = BashTool;
        let err = tool.execute(json!({}), &test_ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_bash_multiline_output() {
        let tool = BashTool;
        let result = tool
            .execute(
                json!({"command": "echo line1; echo line2; echo line3"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        let lines: Vec<&str> = result.content.trim().lines().collect();
        assert!(lines.len() >= 3);
    }

    #[tokio::test]
    async fn test_bash_pipe_command() {
        let tool = BashTool;
        let result = tool
            .execute(
                json!({"command": "echo hello world | wc -w"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.trim().contains("2"));
    }

    #[tokio::test]
    async fn test_bash_env_propagation() {
        let tool = BashTool;
        let result = tool
            .execute(
                json!({"command": "echo $HOME"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        // HOME should be set and non-empty.
        assert!(!result.content.trim().is_empty());
    }

    #[test]
    fn test_format_output_success() {
        let out = format_output("hello\n", "", 0, false);
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn test_format_output_error() {
        let out = format_output("", "error msg\n", 1, true);
        assert!(out.contains("Exit code: 1"));
        assert!(out.contains("error msg"));
    }

    #[test]
    fn test_format_output_both_streams() {
        let out = format_output("stdout data\n", "stderr data\n", 0, false);
        assert!(out.contains("stdout data"));
        assert!(out.contains("stderr"));
    }

    #[test]
    fn test_parse_env_vars() {
        let value = json!({"FOO": "bar", "BAZ": "qux"});
        let vars = parse_env_vars(&value);
        assert_eq!(vars.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(vars.get("BAZ"), Some(&"qux".to_string()));
    }

    #[test]
    fn test_parse_env_vars_empty() {
        let vars = parse_env_vars(&json!(null));
        assert!(vars.is_empty());
    }
}

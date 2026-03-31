use std::time::Duration;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using ripgrep"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search"
                },
                "glob": {
                    "type": "string",
                    "description": "File glob filter (e.g. *.rs)"
                },
                "output_mode": {
                    "type": "string",
                    "description": "content, files_with_matches, or count"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("pattern is required".into()))?;

        let search_path = input["path"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| ctx.working_dir.to_string_lossy().to_string());

        let output_mode = input["output_mode"].as_str().unwrap_or("files_with_matches");

        let mut cmd = tokio::process::Command::new("rg");
        cmd.arg("--no-heading");

        match output_mode {
            "files_with_matches" => {
                cmd.arg("-l");
            }
            "count" => {
                cmd.arg("-c");
            }
            _ => {
                cmd.arg("-n");
            }
        }

        if let Some(file_glob) = input["glob"].as_str() {
            cmd.arg("--glob").arg(file_glob);
        }

        cmd.arg(pattern).arg(&search_path);

        let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .map_err(|_| ToolError::Timeout(30_000))?
            .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // rg exits 1 for no matches (not an error), 2 for actual errors.
        if output.status.code() == Some(2) {
            return Err(ToolError::Execution(format!("rg error: {stderr}")));
        }

        let content = if stdout.is_empty() {
            "No matches found".to_string()
        } else {
            stdout
        };

        Ok(ToolResult {
            content,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn rg_available() -> bool {
        std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn test_grep_find_pattern() {
        if !rg_available() {
            eprintln!("skipping: rg not available");
            return;
        }
        let dir = std::env::temp_dir().join("ccx_test_grep");
        let _ = fs::create_dir_all(&dir);
        fs::write(
            dir.join("test.txt"),
            "hello world\nfoo bar\nhello again\n",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(dir.clone());
        let result = tool
            .execute(
                json!({"pattern": "hello", "path": dir.to_str().unwrap()}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("hello"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        if !rg_available() {
            eprintln!("skipping: rg not available");
            return;
        }
        let dir = std::env::temp_dir().join("ccx_test_grep_none");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("test.txt"), "hello world\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(dir.clone());
        let result = tool
            .execute(
                json!({"pattern": "zzz_nonexistent", "path": dir.to_str().unwrap()}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.content, "No matches found");

        let _ = fs::remove_dir_all(&dir);
    }
}

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g. **/*.rs)"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in (defaults to working dir)"
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

        let base = input["path"]
            .as_str()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        let full_pattern = base.join(pattern);
        let full_pattern_str = full_pattern.to_string_lossy();

        let entries: Vec<String> = glob::glob(&full_pattern_str)
            .map_err(|e| ToolError::Execution(format!("invalid glob pattern: {e}")))?
            .filter_map(|entry| entry.ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        if entries.is_empty() {
            Ok(ToolResult {
                content: "No files matched".to_string(),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: entries.join("\n"),
                is_error: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = std::env::temp_dir().join("ccx_test_glob");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("a.txt"), "").unwrap();
        fs::write(dir.join("b.txt"), "").unwrap();
        fs::write(dir.join("c.rs"), "").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(dir.clone());
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("a.txt"));
        assert!(result.content.contains("b.txt"));
        assert!(!result.content.contains("c.rs"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_glob_no_match() {
        let tool = GlobTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"pattern": "*.nonexistent_extension_xyz"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.content, "No files matched");
    }
}

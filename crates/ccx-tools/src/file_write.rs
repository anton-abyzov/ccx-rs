use std::fs;
use std::path::Path;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating it if necessary"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("file_path is required".into()))?;
        let content = input["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("content is required".into()))?;

        if let Some(parent) = Path::new(file_path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ToolError::Execution(format!("failed to create dirs: {e}")))?;
        }

        fs::write(file_path, content)
            .map_err(|e| ToolError::Execution(format!("failed to write {file_path}: {e}")))?;

        let lines = content.lines().count();
        Ok(ToolResult {
            content: format!("Wrote {lines} lines to {file_path}"),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_file_write() {
        let dir = std::env::temp_dir().join("ccx_test_write");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("output.txt");

        let tool = FileWriteTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({"file_path": path.to_str().unwrap(), "content": "hello\nworld\n"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\nworld\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_write_creates_dirs() {
        let dir = std::env::temp_dir().join("ccx_test_write_dirs/nested/path");
        let path = dir.join("file.txt");

        let tool = FileWriteTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({"file_path": path.to_str().unwrap(), "content": "test"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(path.exists());

        let _ = fs::remove_dir_all(std::env::temp_dir().join("ccx_test_write_dirs"));
    }
}

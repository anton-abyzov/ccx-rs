use std::fs;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read a file from the filesystem with optional offset and limit"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start from (0-based)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of lines to read"
                }
            },
            "required": ["file_path"]
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
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(2000) as usize;

        let content = fs::read_to_string(file_path)
            .map_err(|e| ToolError::Execution(format!("failed to read {file_path}: {e}")))?;

        let lines: Vec<&str> = content.lines().collect();
        let start = offset.min(lines.len());
        let end = (offset + limit).min(lines.len());
        let selected = &lines[start..end];

        let numbered: String = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{line}", offset + i + 1))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            content: numbered,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_file_read() {
        let dir = std::env::temp_dir().join("ccx_test_read");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("1\tline1"));
        assert!(result.content.contains("2\tline2"));
        assert!(!result.is_error);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_read_with_offset() {
        let dir = std::env::temp_dir().join("ccx_test_read_offset");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({"file_path": path.to_str().unwrap(), "offset": 2, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("3\tc"));
        assert!(result.content.contains("4\td"));
        assert!(!result.content.contains("1\ta"));

        let _ = fs::remove_dir_all(&dir);
    }
}

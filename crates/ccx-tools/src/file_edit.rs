use std::fs;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacement in a file"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
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
        let old_string = input["old_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("old_string is required".into()))?;
        let new_string = input["new_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("new_string is required".into()))?;
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

        let content = fs::read_to_string(file_path)
            .map_err(|e| ToolError::Execution(format!("failed to read {file_path}: {e}")))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(ToolError::Execution(format!(
                "old_string not found in {file_path}"
            )));
        }
        if !replace_all && count > 1 {
            return Err(ToolError::Execution(format!(
                "old_string found {count} times — use replace_all or provide more context"
            )));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        fs::write(file_path, &new_content)
            .map_err(|e| ToolError::Execution(format!("failed to write {file_path}: {e}")))?;

        Ok(ToolResult {
            content: format!("Replaced {count} occurrence(s) in {file_path}"),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[tokio::test]
    async fn test_file_edit_single() {
        let dir = std::env::temp_dir().join("ccx_test_edit");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "hello",
                    "new_string": "goodbye"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(fs::read_to_string(&path).unwrap(), "goodbye world");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_edit_not_unique() {
        let dir = std::env::temp_dir().join("ccx_test_edit_dup");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "aa bb aa").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let err = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "aa",
                    "new_string": "cc"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_edit_replace_all() {
        let dir = std::env::temp_dir().join("ccx_test_edit_all");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "aa bb aa").unwrap();

        let tool = FileEditTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "aa",
                    "new_string": "cc",
                    "replace_all": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(fs::read_to_string(&path).unwrap(), "cc bb cc");

        let _ = fs::remove_dir_all(&dir);
    }
}

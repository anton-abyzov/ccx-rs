use std::fs;
use std::path::Path;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::path_validation::validate_path;

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

        // Validate old_string != new_string.
        if old_string == new_string {
            return Err(ToolError::InvalidInput(
                "old_string and new_string must be different".into(),
            ));
        }

        let path = Path::new(file_path);

        // Path traversal protection (skipped in bypass mode).
        if path.exists() {
            validate_path(path, &_ctx.working_dir, _ctx.bypass_permissions)?;
        }

        if !path.exists() {
            return Err(ToolError::Execution(format!("file not found: {file_path}")));
        }

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
                "old_string found {count} times in {file_path} — provide more context to make it \
                 unique, or set replace_all to true"
            )));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Atomic write: write to temp file in same directory, then rename.
        // This prevents data loss if the process is interrupted during write.
        let tmp_path = format!("{file_path}.ccx_tmp_{}", std::process::id());
        fs::write(&tmp_path, &new_content)
            .map_err(|e| ToolError::Execution(format!("failed to write temp file: {e}")))?;

        if let Err(e) = fs::rename(&tmp_path, file_path) {
            // Clean up temp file on rename failure.
            let _ = fs::remove_file(&tmp_path);
            return Err(ToolError::Execution(format!(
                "failed to replace {file_path}: {e}"
            )));
        }

        // Find the line number where the replacement occurred for context.
        let line_info = find_replacement_line(&content, old_string);

        Ok(ToolResult {
            content: format!("Replaced {count} occurrence(s) in {file_path}{line_info}"),
            is_error: false,
        })
    }
}

/// Find the line number(s) where the old_string starts.
fn find_replacement_line(content: &str, old_string: &str) -> String {
    let mut lines = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = content[search_from..].find(old_string) {
        let abs_pos = search_from + pos;
        // Line number = newlines before position + 1 (1-based).
        let line_num = content[..abs_pos].chars().filter(|c| *c == '\n').count() + 1;
        lines.push(line_num);
        search_from = abs_pos + old_string.len();
    }

    if lines.is_empty() {
        String::new()
    } else if lines.len() == 1 {
        format!(" (line {})", lines[0])
    } else if lines.len() <= 5 {
        format!(
            " (lines {})",
            lines
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            " (lines {}, ... and {} more)",
            lines[..3]
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            lines.len() - 3
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_ctx() -> ToolContext {
        let mut ctx = ToolContext::new(PathBuf::from("/tmp"));
        ctx.bypass_permissions = true;
        ctx
    }

    #[tokio::test]
    async fn test_file_edit_single() {
        let dir = std::env::temp_dir().join("ccx_test_edit");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let tool = FileEditTool;
        let result = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "hello",
                    "new_string": "goodbye"
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("1 occurrence"));
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
        let err = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "aa",
                    "new_string": "cc"
                }),
                &test_ctx(),
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
        let result = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "aa",
                    "new_string": "cc",
                    "replace_all": true
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(fs::read_to_string(&path).unwrap(), "cc bb cc");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_edit_not_found() {
        let tool = FileEditTool;
        let err = tool
            .execute(
                json!({
                    "file_path": "/nonexistent/file.txt",
                    "old_string": "a",
                    "new_string": "b"
                }),
                &test_ctx(),
            )
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(msg) => assert!(msg.contains("not found")),
            _ => panic!("expected Execution error"),
        }
    }

    #[tokio::test]
    async fn test_file_edit_same_string() {
        let tool = FileEditTool;
        let dir = std::env::temp_dir().join("ccx_test_edit_same");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "hello").unwrap();

        let err = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "hello",
                    "new_string": "hello"
                }),
                &test_ctx(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_edit_old_string_missing() {
        let dir = std::env::temp_dir().join("ccx_test_edit_miss");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let tool = FileEditTool;
        let err = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "xyz",
                    "new_string": "abc"
                }),
                &test_ctx(),
            )
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(msg) => assert!(msg.contains("not found")),
            _ => panic!("expected Execution error"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_edit_atomic_no_data_loss() {
        let dir = std::env::temp_dir().join("ccx_test_edit_atomic");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        let original = "important data here";
        fs::write(&path, original).unwrap();

        let tool = FileEditTool;
        let result = tool
            .execute(
                json!({
                    "file_path": path.to_str().unwrap(),
                    "old_string": "important",
                    "new_string": "critical"
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!result.is_error);

        // Verify the file was updated and no temp file remains.
        assert_eq!(fs::read_to_string(&path).unwrap(), "critical data here");
        let tmp_pattern = format!("{}.ccx_tmp_*", path.to_str().unwrap());
        assert!(
            glob::glob(&tmp_pattern)
                .unwrap()
                .filter_map(|e| e.ok())
                .count()
                == 0
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_replacement_line_single() {
        let content = "line 1\nline 2\ntarget here\nline 4\n";
        let info = find_replacement_line(content, "target");
        assert!(info.contains("3"));
    }

    #[test]
    fn test_find_replacement_line_multiple() {
        let content = "aa\nbb\naa\ncc\n";
        let info = find_replacement_line(content, "aa");
        assert!(info.contains("1"));
        assert!(info.contains("3"));
    }
}

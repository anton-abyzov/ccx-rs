use std::path::PathBuf;

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
        "Find files matching a glob pattern, sorted by modification time"
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
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());

        let full_pattern = base.join(pattern);
        let full_pattern_str = full_pattern.to_string_lossy();

        let mut entries: Vec<(PathBuf, std::time::SystemTime)> =
            glob::glob(&full_pattern_str)
                .map_err(|e| ToolError::Execution(format!("invalid glob pattern: {e}")))?
                .filter_map(|entry| entry.ok())
                .filter_map(|path| {
                    let mtime = path
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    Some((path, mtime))
                })
                .collect();

        // Sort by modification time, most recent first.
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        if entries.is_empty() {
            Ok(ToolResult {
                content: "No files matched".to_string(),
                is_error: false,
            })
        } else {
            let count = entries.len();
            let paths: Vec<String> = entries
                .iter()
                .map(|(p, _)| p.to_string_lossy().to_string())
                .collect();
            let mut content = paths.join("\n");
            if count > 1 {
                content.push_str(&format!("\n\n{count} files matched"));
            }
            Ok(ToolResult {
                content,
                is_error: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn test_ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf())
    }

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = std::env::temp_dir().join("ccx_test_glob");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("a.txt"), "").unwrap();
        fs::write(dir.join("b.txt"), "").unwrap();
        fs::write(dir.join("c.rs"), "").unwrap();

        let tool = GlobTool;
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &test_ctx(&dir))
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

    #[tokio::test]
    async fn test_glob_sorted_by_mtime() {
        let dir = std::env::temp_dir().join("ccx_test_glob_sort");
        let _ = fs::create_dir_all(&dir);

        // Create files with slight time gaps.
        fs::write(dir.join("old.txt"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(dir.join("new.txt"), "new").unwrap();

        let tool = GlobTool;
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &test_ctx(&dir))
            .await
            .unwrap();

        // new.txt should appear before old.txt (most recent first).
        let new_pos = result.content.find("new.txt").unwrap();
        let old_pos = result.content.find("old.txt").unwrap();
        assert!(new_pos < old_pos, "Expected new.txt before old.txt");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_glob_with_explicit_path() {
        let dir = std::env::temp_dir().join("ccx_test_glob_path");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("target.txt"), "").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(
                json!({"pattern": "*.txt", "path": dir.to_str().unwrap()}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("target.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let dir = std::env::temp_dir().join("ccx_test_glob_recursive");
        let sub = dir.join("subdir");
        let _ = fs::create_dir_all(&sub);
        fs::write(dir.join("top.rs"), "").unwrap();
        fs::write(sub.join("nested.rs"), "").unwrap();

        let tool = GlobTool;
        let result = tool
            .execute(json!({"pattern": "**/*.rs"}), &test_ctx(&dir))
            .await
            .unwrap();
        assert!(result.content.contains("top.rs"));
        assert!(result.content.contains("nested.rs"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_glob_count_display() {
        let dir = std::env::temp_dir().join("ccx_test_glob_count");
        let _ = fs::create_dir_all(&dir);
        for i in 0..5 {
            fs::write(dir.join(format!("f{i}.txt")), "").unwrap();
        }

        let tool = GlobTool;
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &test_ctx(&dir))
            .await
            .unwrap();
        assert!(result.content.contains("5 files matched"));

        let _ = fs::remove_dir_all(&dir);
    }
}

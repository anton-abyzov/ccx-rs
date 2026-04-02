use std::time::Duration;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

pub struct GrepTool;

/// Find ripgrep binary — check PATH first, then common install locations.
fn which_rg() -> String {
    if let Ok(output) = std::process::Command::new("which").arg("rg").output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }
    // Common fallback paths
    for path in &["/opt/homebrew/bin/rg", "/usr/local/bin/rg", "/usr/bin/rg"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    "rg".to_string() // last resort
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using ripgrep with full context and filtering support"
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
                "type": {
                    "type": "string",
                    "description": "File type filter (e.g. rust, py, js)"
                },
                "output_mode": {
                    "type": "string",
                    "description": "content, files_with_matches, or count"
                },
                "-A": {
                    "type": "integer",
                    "description": "Lines of context after each match"
                },
                "-B": {
                    "type": "integer",
                    "description": "Lines of context before each match"
                },
                "-C": {
                    "type": "integer",
                    "description": "Lines of context before and after each match"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers (default true for content mode)"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline matching (patterns can span lines)"
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Limit output to first N lines/entries (default 250)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip first N lines/entries before applying head_limit"
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

        let output_mode = input["output_mode"]
            .as_str()
            .unwrap_or("files_with_matches");
        let head_limit = input["head_limit"].as_u64().unwrap_or(250) as usize;
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;

        // Try system rg first, fall back to common paths
        let rg_path = which_rg();
        let mut cmd = tokio::process::Command::new(&rg_path);
        cmd.arg("--no-heading").arg("--color=never");

        // Output mode flags.
        match output_mode {
            "files_with_matches" => {
                cmd.arg("-l");
            }
            "count" => {
                cmd.arg("-c");
            }
            _ => {
                // Content mode: show line numbers by default.
                let show_line_numbers = input["-n"].as_bool().unwrap_or(true);
                if show_line_numbers {
                    cmd.arg("-n");
                }
            }
        }

        // Context lines: -C takes priority over separate -A/-B.
        if let Some(c) = input["-C"].as_u64().or(input["context"].as_u64()) {
            cmd.arg("-C").arg(c.to_string());
        } else {
            if let Some(a) = input["-A"].as_u64() {
                cmd.arg("-A").arg(a.to_string());
            }
            if let Some(b) = input["-B"].as_u64() {
                cmd.arg("-B").arg(b.to_string());
            }
        }

        // Case insensitive.
        if input["-i"].as_bool().unwrap_or(false) {
            cmd.arg("-i");
        }

        // Multiline mode.
        if input["multiline"].as_bool().unwrap_or(false) {
            cmd.arg("-U").arg("--multiline-dotall");
        }

        // File glob filter.
        if let Some(file_glob) = input["glob"].as_str() {
            cmd.arg("--glob").arg(file_glob);
        }

        // File type filter.
        if let Some(type_filter) = input["type"].as_str() {
            cmd.arg("--type").arg(type_filter);
        }

        cmd.arg("--").arg(pattern).arg(&search_path);
        cmd.current_dir(&ctx.working_dir);

        let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .map_err(|_| ToolError::Timeout(30_000))?
            .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // rg exit codes: 0 = matches found, 1 = no matches, 2 = error.
        match output.status.code() {
            Some(2) => {
                return Err(ToolError::Execution(format!("rg error: {stderr}")));
            }
            Some(1) | None if stdout.is_empty() => {
                return Ok(ToolResult {
                    content: "No matches found".to_string(),
                    is_error: false,
                });
            }
            _ => {}
        }

        // Apply offset and head_limit.
        let lines: Vec<&str> = stdout.lines().collect();
        let total = lines.len();
        let start = offset.min(total);
        let end = if head_limit > 0 {
            (start + head_limit).min(total)
        } else {
            total
        };
        let selected = &lines[start..end];

        let mut content = selected.join("\n");
        if end < total {
            content.push_str(&format!(
                "\n\n... ({} more lines, {total} total)",
                total - end
            ));
        }

        Ok(ToolResult {
            content,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn rg_available() -> bool {
        std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn test_ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf())
    }

    fn setup_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_grep_find_pattern() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep");
        fs::write(dir.join("test.txt"), "hello world\nfoo bar\nhello again\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({"pattern": "hello", "path": dir.to_str().unwrap()}),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("test.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_no_match() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_none");
        fs::write(dir.join("test.txt"), "hello world\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({"pattern": "zzz_nonexistent", "path": dir.to_str().unwrap()}),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert_eq!(result.content, "No matches found");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_content_mode() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_content");
        fs::write(dir.join("test.txt"), "line1\ntarget line\nline3\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "target",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content"
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("target line"));
        assert!(result.content.contains("2:")); // line number

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_count");
        fs::write(dir.join("test.txt"), "hello\nhello\nhello\nworld\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "hello",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "count"
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("3"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_context_lines() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_context");
        fs::write(dir.join("test.txt"), "line1\nline2\nTARGET\nline4\nline5\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "TARGET",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "-C": 1
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("line2"));
        assert!(result.content.contains("TARGET"));
        assert!(result.content.contains("line4"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_case");
        fs::write(dir.join("test.txt"), "Hello World\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "hello",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "-i": true
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("Hello World"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_glob_filter() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_glob");
        fs::write(dir.join("code.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.join("readme.md"), "fn readme() {}\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "fn",
                    "path": dir.to_str().unwrap(),
                    "glob": "*.rs"
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("code.rs"));
        assert!(!result.content.contains("readme.md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_head_limit() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_limit");
        let content: String = (0..50).map(|i| format!("match_{i}\n")).collect();
        fs::write(dir.join("test.txt"), &content).unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "match_",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "head_limit": 5
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        let match_lines: Vec<&str> = result
            .content
            .lines()
            .filter(|l| l.contains("match_"))
            .collect();
        assert_eq!(match_lines.len(), 5);
        assert!(result.content.contains("more lines"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_offset() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_offset");
        let content: String = (0..10).map(|i| format!("line_{i}\n")).collect();
        fs::write(dir.join("test.txt"), &content).unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "line_",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "offset": 5,
                    "head_limit": 3
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        // Should show lines 5-7 (0-indexed: lines starting from offset 5).
        assert!(result.content.contains("line_5"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_after_context() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_after");
        fs::write(
            dir.join("test.txt"),
            "before\nTARGET\nafter1\nafter2\nafter3\n",
        )
        .unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "TARGET",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "-A": 2
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("TARGET"));
        assert!(result.content.contains("after1"));
        assert!(result.content.contains("after2"));
        assert!(!result.content.contains("before"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_before_context() {
        if !rg_available() {
            return;
        }
        let dir = setup_test_dir("ccx_test_grep_before");
        fs::write(dir.join("test.txt"), "before1\nbefore2\nTARGET\nafter\n").unwrap();

        let tool = GrepTool;
        let result = tool
            .execute(
                json!({
                    "pattern": "TARGET",
                    "path": dir.to_str().unwrap(),
                    "output_mode": "content",
                    "-B": 2
                }),
                &test_ctx(&dir),
            )
            .await
            .unwrap();
        assert!(result.content.contains("before1"));
        assert!(result.content.contains("before2"));
        assert!(result.content.contains("TARGET"));
        assert!(!result.content.contains("after"));

        let _ = fs::remove_dir_all(&dir);
    }
}

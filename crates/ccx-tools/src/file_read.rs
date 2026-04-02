use std::fs;
use std::path::Path;

use async_trait::async_trait;
use base64::Engine;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::path_validation::validate_path;

/// Maximum bytes to scan for binary detection.
const BINARY_CHECK_SIZE: usize = 8192;

/// Default number of lines to read.
const DEFAULT_LINE_LIMIT: usize = 2000;

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
                    "description": "Number of lines to read (default 2000)"
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
        let limit = input["limit"].as_u64().unwrap_or(DEFAULT_LINE_LIMIT as u64) as usize;

        let path = Path::new(file_path);

        // Check existence with specific error messages.
        if !path.exists() {
            return Err(ToolError::Execution(format!("file not found: {file_path}")));
        }

        if path.is_dir() {
            return Err(ToolError::Execution(format!(
                "{file_path} is a directory, not a file. Use Bash with 'ls' to list directory contents."
            )));
        }

        // Path traversal protection (skipped in bypass mode).
        validate_path(path, &_ctx.working_dir, _ctx.bypass_permissions)?;

        // Check for image/PDF extensions before reading — return base64 for these.
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "pdf"
        ) {
            let bytes = fs::read(file_path)
                .map_err(|e| ToolError::Execution(format!("failed to read {file_path}: {e}")))?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "svg" => "image/svg+xml",
                "pdf" => "application/pdf",
                _ => "application/octet-stream",
            };
            return Ok(ToolResult {
                content: format!("data:{mime};base64,{b64}"),
                is_error: false,
            });
        }

        // Read raw bytes first for binary detection and size check.
        let raw_bytes = fs::read(file_path).map_err(|e| {
            let msg = match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!("permission denied: {file_path}")
                }
                _ => format!("failed to read {file_path}: {e}"),
            };
            ToolError::Execution(msg)
        })?;

        let file_size = raw_bytes.len();

        // Binary file detection: check for null bytes in first chunk.
        if is_binary(&raw_bytes) {
            return Ok(ToolResult {
                content: format!("{file_path}: binary file ({})", format_file_size(file_size)),
                is_error: false,
            });
        }

        // Convert to string.
        let content = String::from_utf8_lossy(&raw_bytes);

        // Handle empty files.
        if content.is_empty() {
            return Ok(ToolResult {
                content: format!("{file_path}: empty file (0 bytes)"),
                is_error: false,
            });
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start = offset.min(total_lines);
        let end = (offset + limit).min(total_lines);
        let selected = &lines[start..end];

        // Format with line numbers (1-based, matching `cat -n`).
        let numbered: String = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{line}", start + i + 1))
            .collect::<Vec<_>>()
            .join("\n");

        // Add truncation notice if not showing all lines.
        let content = if end < total_lines {
            format!(
                "{numbered}\n\n... ({} more lines, {total_lines} total)",
                total_lines - end
            )
        } else {
            numbered
        };

        Ok(ToolResult {
            content,
            is_error: false,
        })
    }
}

/// Check if file content appears to be binary by scanning for null bytes.
fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(BINARY_CHECK_SIZE);
    bytes[..check_len].contains(&0)
}

/// Format file size in human-readable form.
fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
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
    async fn test_file_read() {
        let dir = std::env::temp_dir().join("ccx_test_read");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.txt");
        fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &test_ctx())
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
        let result = tool
            .execute(
                json!({"file_path": path.to_str().unwrap(), "offset": 2, "limit": 2}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("3\tc"));
        assert!(result.content.contains("4\td"));
        assert!(!result.content.contains("1\ta"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let tool = FileReadTool;
        let err = tool
            .execute(json!({"file_path": "/nonexistent/file.txt"}), &test_ctx())
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(msg) => assert!(msg.contains("not found")),
            _ => panic!("expected Execution error"),
        }
    }

    #[tokio::test]
    async fn test_file_read_directory() {
        let tool = FileReadTool;
        let err = tool
            .execute(json!({"file_path": "/tmp"}), &test_ctx())
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(msg) => assert!(msg.contains("directory")),
            _ => panic!("expected Execution error"),
        }
    }

    #[tokio::test]
    async fn test_file_read_binary() {
        let dir = std::env::temp_dir().join("ccx_test_read_binary");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("binary.dat");
        fs::write(&path, b"hello\x00world").unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("binary file"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_read_empty() {
        let dir = std::env::temp_dir().join("ccx_test_read_empty");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("empty.txt");
        fs::write(&path, "").unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("empty file"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_read_truncation_notice() {
        let dir = std::env::temp_dir().join("ccx_test_read_trunc");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("long.txt");
        let content: String = (0..100).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, &content).unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(
                json!({"file_path": path.to_str().unwrap(), "limit": 10}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("more lines"));
        assert!(result.content.contains("100 total"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_read_image_png() {
        let dir = std::env::temp_dir().join("ccx_test_read_img");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.png");
        // Write a minimal PNG-like binary payload.
        fs::write(&path, b"\x89PNG\r\n\x1a\n\x00\x00").unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.starts_with("data:image/png;base64,"));
        assert!(!result.is_error);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_read_pdf() {
        let dir = std::env::temp_dir().join("ccx_test_read_pdf");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.pdf");
        fs::write(&path, b"%PDF-1.4 fake").unwrap();

        let tool = FileReadTool;
        let result = tool
            .execute(json!({"file_path": path.to_str().unwrap()}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.starts_with("data:application/pdf;base64,"));
        assert!(!result.is_error);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_binary() {
        assert!(is_binary(b"hello\x00world"));
        assert!(!is_binary(b"hello world"));
        assert!(!is_binary(b""));
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(500), "500 bytes");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(2 * 1024 * 1024), "2.0 MB");
    }
}

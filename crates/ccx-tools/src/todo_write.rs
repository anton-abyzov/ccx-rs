use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

/// File name for persistent todo storage.
const TODO_FILE: &str = ".ccx-todos.json";

pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Create or update a todo list for tracking tasks"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "List of todo items",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "Todo item content"
                            },
                            "status": {
                                "type": "string",
                                "description": "Status: pending, in_progress, or completed",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let todos = input["todos"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidInput("todos array is required".into()))?;

        let mut items: Vec<TodoItem> = Vec::new();
        for todo in todos {
            let content = todo["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidInput("content is required for each todo".into()))?
                .to_string();
            let status = todo["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();

            if !["pending", "in_progress", "completed"].contains(&status.as_str()) {
                return Err(ToolError::InvalidInput(format!(
                    "invalid status '{status}': must be pending, in_progress, or completed"
                )));
            }

            items.push(TodoItem { content, status });
        }

        // Write to file.
        let path = ctx.working_dir.join(TODO_FILE);
        let json = serde_json::to_string_pretty(&items)
            .map_err(|e| ToolError::Execution(format!("failed to serialize: {e}")))?;
        std::fs::write(&path, &json)
            .map_err(|e| ToolError::Io(e))?;

        // Format summary.
        let total = items.len();
        let completed = items.iter().filter(|t| t.status == "completed").count();
        let in_progress = items.iter().filter(|t| t.status == "in_progress").count();
        let pending = total - completed - in_progress;

        let mut summary = format!(
            "Updated {total} todos ({completed} completed, {in_progress} in progress, {pending} pending)\n\n"
        );

        for (i, item) in items.iter().enumerate() {
            let icon = match item.status.as_str() {
                "completed" => "[x]",
                "in_progress" => "[-]",
                _ => "[ ]",
            };
            summary.push_str(&format!("{icon} {}. {}\n", i + 1, item.content));
        }

        Ok(ToolResult {
            content: summary,
            is_error: false,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TodoItem {
    content: String,
    status: String,
}

/// Load existing todos from the working directory.
#[cfg(test)]
fn load_todos(working_dir: &std::path::PathBuf) -> Vec<TodoItem> {
    let path = working_dir.join(TODO_FILE);
    if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_ctx(suffix: &str) -> ToolContext {
        let dir = std::env::temp_dir().join(format!("ccx_test_todo_{suffix}"));
        let _ = std::fs::create_dir_all(&dir);
        ToolContext::new(dir)
    }

    #[test]
    fn test_todo_schema() {
        let tool = TodoWriteTool;
        assert_eq!(tool.name(), "TodoWrite");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "todos"));
    }

    #[tokio::test]
    async fn test_todo_write_basic() {
        let ctx = test_ctx("basic");
        let tool = TodoWriteTool;
        let result = tool
            .execute(
                json!({
                    "todos": [
                        {"content": "Write tests", "status": "pending"},
                        {"content": "Review code", "status": "completed"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("2 todos"));
        assert!(result.content.contains("1 completed"));
        assert!(result.content.contains("Write tests"));

        // Verify file was written.
        let items = load_todos(&ctx.working_dir);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "Write tests");
        assert_eq!(items[1].status, "completed");

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_todo_write_invalid_status() {
        let ctx = test_ctx("invalid");
        let tool = TodoWriteTool;
        let err = tool
            .execute(
                json!({
                    "todos": [
                        {"content": "Task", "status": "invalid"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_todo_write_missing_content() {
        let ctx = test_ctx("nocontent");
        let tool = TodoWriteTool;
        let err = tool
            .execute(
                json!({
                    "todos": [
                        {"status": "pending"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_todo_write_empty_list() {
        let ctx = test_ctx("empty");
        let tool = TodoWriteTool;
        let result = tool
            .execute(json!({"todos": []}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("0 todos"));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_todo_write_missing_array() {
        let tool = TodoWriteTool;
        let err = tool.execute(json!({}), &test_ctx("noarr")).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[test]
    fn test_load_todos_nonexistent() {
        let items = load_todos(&PathBuf::from("/nonexistent/path"));
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_todo_write_all_statuses() {
        let ctx = test_ctx("allstatus");
        let tool = TodoWriteTool;
        let result = tool
            .execute(
                json!({
                    "todos": [
                        {"content": "Done", "status": "completed"},
                        {"content": "Doing", "status": "in_progress"},
                        {"content": "Todo", "status": "pending"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.content.contains("[x]"));
        assert!(result.content.contains("[-]"));
        assert!(result.content.contains("[ ]"));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }
}

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;
use crate::task_create::TaskRecord;

pub struct TaskListTool;

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "TaskList"
    }

    fn description(&self) -> &str {
        "List all tasks for the current team with their status"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(
        &self,
        _input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let base = meta_helpers::resolve_base_dir(ctx)?;
        let team_name = meta_helpers::current_team(&base)?;
        let tasks_path = meta_helpers::tasks_dir(&base, &team_name);

        if !tasks_path.exists() {
            return Ok(ToolResult {
                content: format!("No tasks for team '{team_name}'."),
                is_error: false,
            });
        }

        let mut tasks: Vec<TaskRecord> = Vec::new();
        for entry in std::fs::read_dir(&tasks_path).map_err(ToolError::Io)? {
            let entry = entry.map_err(ToolError::Io)?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.ends_with(".json") {
                continue;
            }
            let raw = std::fs::read_to_string(entry.path()).map_err(ToolError::Io)?;
            match serde_json::from_str::<TaskRecord>(&raw) {
                Ok(task) => tasks.push(task),
                Err(_) => continue, // skip corrupt files
            }
        }

        // Sort by ID.
        tasks.sort_by(|a, b| {
            let a_id: u32 = a.id.parse().unwrap_or(0);
            let b_id: u32 = b.id.parse().unwrap_or(0);
            a_id.cmp(&b_id)
        });

        if tasks.is_empty() {
            return Ok(ToolResult {
                content: format!("No tasks for team '{team_name}'."),
                is_error: false,
            });
        }

        // Build summary.
        let total = tasks.len();
        let completed = tasks.iter().filter(|t| t.status == "completed").count();
        let in_progress = tasks.iter().filter(|t| t.status == "in_progress").count();
        let pending = total - completed - in_progress;

        let mut output = format!(
            "Team '{team_name}': {total} tasks ({completed} completed, {in_progress} in progress, {pending} pending)\n\n"
        );

        for task in &tasks {
            let icon = match task.status.as_str() {
                "completed" => "[x]",
                "in_progress" => "[-]",
                _ => "[ ]",
            };
            let owner = task.owner.as_deref().unwrap_or("unassigned");
            output.push_str(&format!(
                "{icon} #{}: {} ({})\n",
                task.id, task.subject, owner
            ));
        }

        // Also include JSON array for programmatic use.
        let json_arr = serde_json::to_string(&tasks)
            .map_err(|e| ToolError::Execution(format!("serialize error: {e}")))?;
        output.push_str(&format!("\n<json>\n{json_arr}\n</json>"));

        Ok(ToolResult {
            content: output,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_helpers::{cleanup, test_ctx};
    use crate::{TaskCreateTool, TeamCreateTool};

    async fn setup(ctx: &ToolContext) {
        TeamCreateTool
            .execute(
                json!({"team_name": "list-test", "description": "List testing"}),
                ctx,
            )
            .await
            .unwrap();
        TaskCreateTool
            .execute(json!({"subject": "Task A", "status": "completed"}), ctx)
            .await
            .unwrap();
        TaskCreateTool
            .execute(
                json!({"subject": "Task B", "status": "in_progress", "owner": "bob"}),
                ctx,
            )
            .await
            .unwrap();
        TaskCreateTool
            .execute(json!({"subject": "Task C"}), ctx)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_list_basic() {
        let (ctx, base) = test_ctx("list_basic");
        setup(&ctx).await;

        let result = TaskListTool.execute(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("3 tasks"));
        assert!(result.content.contains("1 completed"));
        assert!(result.content.contains("Task A"));
        assert!(result.content.contains("Task B"));
        assert!(result.content.contains("Task C"));
        assert!(result.content.contains("bob"));
        assert!(result.content.contains("<json>"));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_list_empty() {
        let (ctx, base) = test_ctx("list_empty");
        TeamCreateTool
            .execute(
                json!({"team_name": "empty-team", "description": "Empty"}),
                &ctx,
            )
            .await
            .unwrap();

        let result = TaskListTool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("No tasks"));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_list_no_team() {
        let (ctx, base) = test_ctx("list_no_team");
        let err = TaskListTool.execute(json!({}), &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = TaskListTool;
        assert_eq!(tool.name(), "TaskList");
    }
}

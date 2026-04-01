use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;
use crate::task_create::TaskRecord;

pub struct TaskUpdateTool;

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "TaskUpdate"
    }

    fn description(&self) -> &str {
        "Update the status or owner of an existing task"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "taskId": {
                    "type": "string",
                    "description": "The task ID to update"
                },
                "status": {
                    "type": "string",
                    "description": "New status: pending, in_progress, or completed",
                    "enum": ["pending", "in_progress", "completed"]
                },
                "owner": {
                    "type": "string",
                    "description": "New owner for the task"
                }
            },
            "required": ["taskId"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let task_id = input["taskId"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("'taskId' is required".into()))?
            .trim()
            .to_string();

        if task_id.is_empty() {
            return Err(ToolError::InvalidInput(
                "'taskId' must not be empty".into(),
            ));
        }

        let base = meta_helpers::resolve_base_dir(ctx)?;
        let team_name = meta_helpers::current_team(&base)?;
        let tasks_path = meta_helpers::tasks_dir(&base, &team_name);
        let file_path = tasks_path.join(format!("{task_id}.json"));

        if !file_path.exists() {
            return Err(ToolError::InvalidInput(format!(
                "task #{task_id} not found"
            )));
        }

        let raw = std::fs::read_to_string(&file_path).map_err(ToolError::Io)?;
        let mut task: TaskRecord = serde_json::from_str(&raw)
            .map_err(|e| ToolError::Execution(format!("corrupt task file: {e}")))?;

        let mut changes = Vec::new();

        if let Some(status) = input["status"].as_str() {
            if !["pending", "in_progress", "completed"].contains(&status) {
                return Err(ToolError::InvalidInput(format!(
                    "invalid status '{status}'"
                )));
            }
            let old = task.status.clone();
            task.status = status.to_string();
            changes.push(format!("status: {old} -> {status}"));
        }

        if let Some(owner) = input["owner"].as_str() {
            let old = task.owner.clone().unwrap_or_else(|| "none".into());
            task.owner = Some(owner.to_string());
            changes.push(format!("owner: {old} -> {owner}"));
        }

        if changes.is_empty() {
            return Ok(ToolResult {
                content: format!("Task #{task_id}: no changes specified."),
                is_error: false,
            });
        }

        let task_json = serde_json::to_string_pretty(&task)
            .map_err(|e| ToolError::Execution(format!("serialize error: {e}")))?;
        std::fs::write(&file_path, &task_json).map_err(ToolError::Io)?;

        Ok(ToolResult {
            content: format!(
                "Task #{task_id} updated: {}",
                changes.join(", ")
            ),
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
                json!({"team_name": "upd-test", "description": "Update testing"}),
                ctx,
            )
            .await
            .unwrap();
        TaskCreateTool
            .execute(json!({"subject": "Initial task"}), ctx)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_update_status() {
        let (ctx, base) = test_ctx("upd_status");
        setup(&ctx).await;

        let result = TaskUpdateTool
            .execute(json!({"taskId": "1", "status": "completed"}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("completed"));

        let raw = std::fs::read_to_string(base.join("tasks/upd-test/1.json")).unwrap();
        let task: TaskRecord = serde_json::from_str(&raw).unwrap();
        assert_eq!(task.status, "completed");

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_update_owner() {
        let (ctx, base) = test_ctx("upd_owner");
        setup(&ctx).await;

        let result = TaskUpdateTool
            .execute(json!({"taskId": "1", "owner": "agent-b"}), &ctx)
            .await
            .unwrap();

        assert!(result.content.contains("agent-b"));
        cleanup(&base);
    }

    #[tokio::test]
    async fn test_update_not_found() {
        let (ctx, base) = test_ctx("upd_notfound");
        setup(&ctx).await;
        let err = TaskUpdateTool
            .execute(json!({"taskId": "999"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        cleanup(&base);
    }

    #[tokio::test]
    async fn test_update_no_changes() {
        let (ctx, base) = test_ctx("upd_noop");
        setup(&ctx).await;
        let result = TaskUpdateTool
            .execute(json!({"taskId": "1"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("no changes"));
        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = TaskUpdateTool;
        assert_eq!(tool.name(), "TaskUpdate");
        let schema = tool.input_schema();
        let req = schema["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "taskId"));
    }
}

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::meta_helpers;

pub struct TaskCreateTool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: String,
    pub owner: Option<String>,
    #[serde(rename = "blockedBy", default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "TaskCreate"
    }

    fn description(&self) -> &str {
        "Create a new task for tracking work in the current team"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "Short title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed description of what needs to be done"
                },
                "status": {
                    "type": "string",
                    "description": "Initial status: pending, in_progress, or completed",
                    "enum": ["pending", "in_progress", "completed"],
                    "default": "pending"
                },
                "owner": {
                    "type": "string",
                    "description": "Optional agent/person assigned to this task"
                }
            },
            "required": ["subject"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let subject = input["subject"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("'subject' is required".into()))?
            .trim()
            .to_string();

        if subject.is_empty() {
            return Err(ToolError::InvalidInput(
                "'subject' must not be empty".into(),
            ));
        }

        let description = input["description"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let status = input["status"]
            .as_str()
            .unwrap_or("pending")
            .to_string();
        let owner = input["owner"].as_str().map(|s| s.to_string());

        if !["pending", "in_progress", "completed"].contains(&status.as_str()) {
            return Err(ToolError::InvalidInput(format!(
                "invalid status '{}': must be pending, in_progress, or completed",
                status
            )));
        }

        let base = meta_helpers::resolve_base_dir(ctx)?;
        let team_name = meta_helpers::current_team(&base)?;
        let tasks_path = meta_helpers::tasks_dir(&base, &team_name);
        std::fs::create_dir_all(&tasks_path).map_err(ToolError::Io)?;

        let id = meta_helpers::next_task_id(&tasks_path)?;

        let task = TaskRecord {
            id: id.to_string(),
            subject: subject.clone(),
            description,
            status: status.clone(),
            owner: owner.clone(),
            blocked_by: Vec::new(),
            blocks: Vec::new(),
        };

        let task_json = serde_json::to_string_pretty(&task)
            .map_err(|e| ToolError::Execution(format!("serialize error: {e}")))?;
        std::fs::write(tasks_path.join(format!("{id}.json")), &task_json)
            .map_err(ToolError::Io)?;

        let owner_str = owner.as_deref().unwrap_or("unassigned");
        Ok(ToolResult {
            content: format!(
                "Task #{id} created: {subject}\nStatus: {status} | Owner: {owner_str}"
            ),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_helpers::{cleanup, test_ctx};
    use crate::TeamCreateTool;

    async fn setup_team(ctx: &ToolContext) {
        TeamCreateTool
            .execute(
                json!({"team_name": "task-test", "description": "Task testing"}),
                ctx,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_create_basic() {
        let (ctx, base) = test_ctx("task_basic");
        setup_team(&ctx).await;

        let result = TaskCreateTool
            .execute(
                json!({"subject": "Write tests", "description": "Unit tests for auth"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("#1"));
        assert!(result.content.contains("Write tests"));

        let file = base.join("tasks/task-test/1.json");
        assert!(file.exists());
        let data: TaskRecord = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(data.id, "1");
        assert_eq!(data.subject, "Write tests");
        assert_eq!(data.status, "pending");

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_auto_increment() {
        let (ctx, base) = test_ctx("task_incr");
        setup_team(&ctx).await;

        TaskCreateTool
            .execute(json!({"subject": "First"}), &ctx)
            .await
            .unwrap();
        let r2 = TaskCreateTool
            .execute(json!({"subject": "Second", "status": "in_progress", "owner": "agent-a"}), &ctx)
            .await
            .unwrap();

        assert!(r2.content.contains("#2"));
        assert!(r2.content.contains("agent-a"));

        assert!(base.join("tasks/task-test/1.json").exists());
        assert!(base.join("tasks/task-test/2.json").exists());

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_missing_subject() {
        let (ctx, base) = test_ctx("task_no_subj");
        setup_team(&ctx).await;
        let err = TaskCreateTool
            .execute(json!({"description": "no subject"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_invalid_status() {
        let (ctx, base) = test_ctx("task_bad_status");
        setup_team(&ctx).await;
        let err = TaskCreateTool
            .execute(json!({"subject": "X", "status": "nope"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = TaskCreateTool;
        assert_eq!(tool.name(), "TaskCreate");
        let schema = tool.input_schema();
        let req = schema["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "subject"));
    }
}

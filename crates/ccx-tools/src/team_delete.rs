use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;

pub struct TeamDeleteTool;

#[async_trait]
impl Tool for TeamDeleteTool {
    fn name(&self) -> &str {
        "TeamDelete"
    }

    fn description(&self) -> &str {
        "Delete the current team and its associated task and message directories"
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

        let team_path = meta_helpers::team_dir(&base, &team_name);
        let tasks_path = meta_helpers::tasks_dir(&base, &team_name);

        if !team_path.exists() {
            return Err(ToolError::Execution(format!(
                "team directory does not exist: {}",
                team_path.display()
            )));
        }

        // Remove team directory (config + messages).
        std::fs::remove_dir_all(&team_path).map_err(ToolError::Io)?;

        // Remove tasks directory if it exists.
        if tasks_path.exists() {
            std::fs::remove_dir_all(&tasks_path).map_err(ToolError::Io)?;
        }

        // Clear current team pointer.
        let current_file = base.join("teams").join(".current");
        if current_file.exists() {
            let current = std::fs::read_to_string(&current_file).unwrap_or_default();
            if current.trim() == team_name {
                let _ = std::fs::remove_file(&current_file);
            }
        }

        Ok(ToolResult {
            content: format!("Team '{}' deleted.", team_name),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TeamCreateTool;
    use crate::meta_helpers::{cleanup, test_ctx};

    #[tokio::test]
    async fn test_delete_basic() {
        let (ctx, base) = test_ctx("del_basic");

        // Create a team first.
        TeamCreateTool
            .execute(
                json!({"team_name": "doomed", "description": "Will be deleted"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(base.join("teams/doomed/config.json").exists());
        assert!(base.join("tasks/doomed").exists());

        // Delete it.
        let result = TeamDeleteTool.execute(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("doomed"));

        assert!(!base.join("teams/doomed").exists());
        assert!(!base.join("tasks/doomed").exists());
        assert!(!base.join("teams/.current").exists());

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_delete_no_team() {
        let (ctx, base) = test_ctx("del_no_team");
        let err = TeamDeleteTool.execute(json!({}), &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = TeamDeleteTool;
        assert_eq!(tool.name(), "TeamDelete");
    }
}

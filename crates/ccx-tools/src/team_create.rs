use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::meta_helpers;

pub struct TeamCreateTool;

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "TeamCreate"
    }

    fn description(&self) -> &str {
        "Create a named team for coordinating parallel agent work"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Unique team identifier (slug-style, e.g. 'feature-xyz')"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of the team's purpose"
                }
            },
            "required": ["team_name", "description"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let team_name = input["team_name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("team_name is required".into()))?
            .trim()
            .to_string();

        if team_name.is_empty() {
            return Err(ToolError::InvalidInput(
                "team_name must not be empty".into(),
            ));
        }

        let description = input["description"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("description is required".into()))?
            .trim()
            .to_string();

        if description.is_empty() {
            return Err(ToolError::InvalidInput(
                "description must not be empty".into(),
            ));
        }

        let base = meta_helpers::resolve_base_dir(ctx)?;
        let team_path = meta_helpers::team_dir(&base, &team_name);
        let tasks_path = meta_helpers::tasks_dir(&base, &team_name);

        // Reject duplicates.
        if team_path.exists() {
            return Err(ToolError::InvalidInput(format!(
                "team '{}' already exists",
                team_name
            )));
        }

        // Create directories: team config, tasks, messages.
        std::fs::create_dir_all(&team_path).map_err(ToolError::Io)?;
        std::fs::create_dir_all(&tasks_path).map_err(ToolError::Io)?;
        std::fs::create_dir_all(meta_helpers::messages_dir(&base, &team_name))
            .map_err(ToolError::Io)?;

        // Write config.json.
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let config = TeamConfig {
            team_name: team_name.clone(),
            description: description.clone(),
            created,
            members: Vec::new(),
        };

        let config_json = serde_json::to_string_pretty(&config)
            .map_err(|e| ToolError::Execution(format!("serialize error: {e}")))?;
        std::fs::write(team_path.join("config.json"), &config_json).map_err(ToolError::Io)?;

        // Set as current team.
        meta_helpers::set_current_team(&base, &team_name)?;

        Ok(ToolResult {
            content: format!(
                "Team '{}' created.\n\nConfig: {}/config.json\nTasks:  {}\n\nSet as active team.",
                team_name,
                team_path.display(),
                tasks_path.display()
            ),
            is_error: false,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub team_name: String,
    pub description: String,
    pub created: u64,
    pub members: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_helpers::{cleanup, test_ctx};

    #[test]
    fn test_schema() {
        let tool = TeamCreateTool;
        assert_eq!(tool.name(), "TeamCreate");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "team_name"));
        assert!(required.iter().any(|v| v == "description"));
    }

    #[tokio::test]
    async fn test_create_basic() {
        let (ctx, base) = test_ctx("tc_basic");
        let tool = TeamCreateTool;

        let result = tool
            .execute(
                json!({"team_name": "alpha", "description": "Alpha team"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("alpha"));

        // Verify directories.
        assert!(base.join("teams/alpha/config.json").exists());
        assert!(base.join("tasks/alpha").exists());
        assert!(base.join("teams/alpha/messages").exists());

        // Verify config content.
        let raw = std::fs::read_to_string(base.join("teams/alpha/config.json")).unwrap();
        let config: TeamConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(config.team_name, "alpha");
        assert_eq!(config.description, "Alpha team");
        assert!(config.members.is_empty());

        // Verify current team.
        let current = std::fs::read_to_string(base.join("teams/.current")).unwrap();
        assert_eq!(current, "alpha");

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_duplicate() {
        let (ctx, base) = test_ctx("tc_dup");
        let tool = TeamCreateTool;

        tool.execute(json!({"team_name": "dup", "description": "First"}), &ctx)
            .await
            .unwrap();

        let err = tool
            .execute(json!({"team_name": "dup", "description": "Second"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_empty_name() {
        let (ctx, base) = test_ctx("tc_empty");
        let err = TeamCreateTool
            .execute(json!({"team_name": " ", "description": "d"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_missing_fields() {
        let (ctx, base) = test_ctx("tc_missing");
        let err = TeamCreateTool
            .execute(json!({"description": "no name"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        let err = TeamCreateTool
            .execute(json!({"team_name": "x"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_create_multiple_teams() {
        let (ctx, base) = test_ctx("tc_multi");

        TeamCreateTool
            .execute(json!({"team_name": "t1", "description": "d1"}), &ctx)
            .await
            .unwrap();
        TeamCreateTool
            .execute(json!({"team_name": "t2", "description": "d2"}), &ctx)
            .await
            .unwrap();

        // Last created is current.
        let current = std::fs::read_to_string(base.join("teams/.current")).unwrap();
        assert_eq!(current, "t2");

        assert!(base.join("teams/t1/config.json").exists());
        assert!(base.join("teams/t2/config.json").exists());

        cleanup(&base);
    }
}

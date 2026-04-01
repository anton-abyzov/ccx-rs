use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// File name for persistent team storage.
const TEAMS_FILE: &str = ".ccx-teams.json";

pub struct TeamCreateTool;

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "TeamCreate"
    }

    fn description(&self) -> &str {
        "Create a named team with a description and persist it to the workspace"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique team identifier (slug-style name)"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of the team's purpose"
                }
            },
            "required": ["name", "description"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let name = input["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("name is required".into()))?
            .trim()
            .to_string();

        if name.is_empty() {
            return Err(ToolError::InvalidInput("name must not be empty".into()));
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

        // Load existing teams.
        let path = ctx.working_dir.join(TEAMS_FILE);
        let mut teams: Vec<Team> = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| ToolError::Io(e))?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Reject duplicate names.
        if teams.iter().any(|t| t.name == name) {
            return Err(ToolError::InvalidInput(format!(
                "team '{name}' already exists"
            )));
        }

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let team = Team {
            name: name.clone(),
            description: description.clone(),
            created_at,
        };

        teams.push(team);

        // Persist.
        let json = serde_json::to_string_pretty(&teams)
            .map_err(|e| ToolError::Execution(format!("failed to serialize: {e}")))?;
        std::fs::write(&path, &json).map_err(|e| ToolError::Io(e))?;

        let content = format!(
            "Team created successfully.\n\nName:        {name}\nDescription: {description}\nCreated at:  {created_at} (unix)\nTotal teams: {}\n",
            teams.len()
        );

        Ok(ToolResult {
            content,
            is_error: false,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Team {
    name: String,
    description: String,
    created_at: u64,
}

/// Load teams from a directory (used in tests).
#[cfg(test)]
fn load_teams(working_dir: &std::path::PathBuf) -> Vec<Team> {
    let path = working_dir.join(TEAMS_FILE);
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
        let dir = std::env::temp_dir().join(format!("ccx_test_team_{suffix}"));
        let _ = std::fs::create_dir_all(&dir);
        ToolContext::new(dir)
    }

    #[test]
    fn test_team_create_schema() {
        let tool = TeamCreateTool;
        assert_eq!(tool.name(), "TeamCreate");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "name"));
        assert!(required.iter().any(|v| v == "description"));
    }

    #[tokio::test]
    async fn test_team_create_basic() {
        let ctx = test_ctx("basic");
        let tool = TeamCreateTool;
        let result = tool
            .execute(
                json!({
                    "name": "research-ts-to-rust-migration",
                    "description": "Research: TypeScript to Rust migration quality analysis for ccx-rs"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Team created successfully"));
        assert!(result.content.contains("research-ts-to-rust-migration"));

        let teams = load_teams(&ctx.working_dir);
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].name, "research-ts-to-rust-migration");
        assert_eq!(
            teams[0].description,
            "Research: TypeScript to Rust migration quality analysis for ccx-rs"
        );

        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_team_create_duplicate_rejected() {
        let ctx = test_ctx("dup");
        let tool = TeamCreateTool;

        tool.execute(
            json!({"name": "alpha", "description": "First"}),
            &ctx,
        )
        .await
        .unwrap();

        let err = tool
            .execute(
                json!({"name": "alpha", "description": "Second"}),
                &ctx,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_team_create_multiple() {
        let ctx = test_ctx("multi");
        let tool = TeamCreateTool;

        tool.execute(
            json!({"name": "team-a", "description": "Alpha team"}),
            &ctx,
        )
        .await
        .unwrap();

        tool.execute(
            json!({"name": "team-b", "description": "Beta team"}),
            &ctx,
        )
        .await
        .unwrap();

        let teams = load_teams(&ctx.working_dir);
        assert_eq!(teams.len(), 2);
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_team_create_missing_name() {
        let ctx = test_ctx("noname");
        let tool = TeamCreateTool;
        let err = tool
            .execute(json!({"description": "No name"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_team_create_missing_description() {
        let ctx = test_ctx("nodesc");
        let tool = TeamCreateTool;
        let err = tool
            .execute(json!({"name": "my-team"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[tokio::test]
    async fn test_team_create_empty_name() {
        let ctx = test_ctx("emptyname");
        let tool = TeamCreateTool;
        let err = tool
            .execute(json!({"name": "  ", "description": "desc"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        let _ = std::fs::remove_dir_all(&ctx.working_dir);
    }

    #[test]
    fn test_load_teams_nonexistent() {
        let teams = load_teams(&PathBuf::from("/nonexistent/path"));
        assert!(teams.is_empty());
    }
}

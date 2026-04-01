use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;

const PLAN_MODE_FLAG: &str = ".plan_mode";

pub struct ExitPlanModeTool;

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Exit plan mode — re-enables write operations for implementation"
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
        let flag = base.join(PLAN_MODE_FLAG);

        if !flag.exists() {
            return Ok(ToolResult {
                content: "Not in plan mode.".into(),
                is_error: false,
            });
        }

        std::fs::remove_file(&flag).map_err(ToolError::Io)?;

        Ok(ToolResult {
            content: "Exited plan mode. You can now make changes.".into(),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enter_plan_mode::{is_plan_mode, EnterPlanModeTool};
    use crate::meta_helpers::{cleanup, test_ctx};

    #[tokio::test]
    async fn test_exit_plan_mode() {
        let (ctx, base) = test_ctx("exit_plan");

        // Enter first.
        EnterPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        assert!(is_plan_mode(&ctx));

        // Exit.
        let result = ExitPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Exited plan mode"));
        assert!(!is_plan_mode(&ctx));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_exit_when_not_in_plan() {
        let (ctx, base) = test_ctx("exit_plan_noop");

        let result = ExitPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("Not in plan mode"));

        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = ExitPlanModeTool;
        assert_eq!(tool.name(), "ExitPlanMode");
    }
}

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

use crate::meta_helpers;

/// Flag file name within the base dir.
const PLAN_MODE_FLAG: &str = ".plan_mode";

pub struct EnterPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Enter plan mode — restricts the agent to read-only exploration and design"
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
        std::fs::create_dir_all(&base).map_err(ToolError::Io)?;

        let flag = base.join(PLAN_MODE_FLAG);
        if flag.exists() {
            return Ok(ToolResult {
                content: "Already in plan mode.".into(),
                is_error: false,
            });
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        std::fs::write(&flag, timestamp.to_string()).map_err(ToolError::Io)?;

        Ok(ToolResult {
            content: "Entered plan mode. You can now explore and design. Use ExitPlanMode when ready to implement.".into(),
            is_error: false,
        })
    }
}

/// Check if plan mode is active.
pub fn is_plan_mode(ctx: &ToolContext) -> bool {
    meta_helpers::resolve_base_dir(ctx)
        .map(|base| base.join(PLAN_MODE_FLAG).exists())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_helpers::{cleanup, test_ctx};

    #[tokio::test]
    async fn test_enter_plan_mode() {
        let (ctx, base) = test_ctx("enter_plan");

        let result = EnterPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Entered plan mode"));
        assert!(base.join(PLAN_MODE_FLAG).exists());
        assert!(is_plan_mode(&ctx));

        cleanup(&base);
    }

    #[tokio::test]
    async fn test_enter_plan_mode_idempotent() {
        let (ctx, base) = test_ctx("enter_plan_idem");

        EnterPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        let result = EnterPlanModeTool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("Already in plan mode"));

        cleanup(&base);
    }

    #[test]
    fn test_schema() {
        let tool = EnterPlanModeTool;
        assert_eq!(tool.name(), "EnterPlanMode");
    }
}

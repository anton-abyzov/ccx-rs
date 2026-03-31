use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Context available to tools during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    /// Working directory for tool execution.
    pub working_dir: PathBuf,
    /// Whether the tool is running in a sandbox.
    pub sandboxed: bool,
}

impl ToolContext {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            sandboxed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_context_new() {
        let ctx = ToolContext::new(PathBuf::from("/tmp"));
        assert_eq!(ctx.working_dir, PathBuf::from("/tmp"));
        assert!(!ctx.sandboxed);
    }
}

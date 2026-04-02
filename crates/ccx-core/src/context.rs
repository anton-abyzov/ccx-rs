use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Context available to tools during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    /// Working directory for tool execution.
    pub working_dir: PathBuf,
    /// Whether the tool is running in a sandbox.
    pub sandboxed: bool,
    /// Extra environment variables to set for child processes.
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// API provider: "anthropic" or "openrouter".
    #[serde(default = "default_provider")]
    pub provider: String,
    /// API key for the current provider.
    #[serde(default)]
    pub api_key: String,
    /// Model name for the current session.
    #[serde(default)]
    pub model: String,
}

fn default_provider() -> String {
    "anthropic".to_string()
}

impl ToolContext {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            sandboxed: false,
            env_vars: HashMap::new(),
            provider: "anthropic".to_string(),
            api_key: String::new(),
            model: String::new(),
        }
    }

    /// Create a context with additional environment variables.
    pub fn with_env(mut self, vars: HashMap<String, String>) -> Self {
        self.env_vars = vars;
        self
    }

    /// Set a single environment variable.
    pub fn set_env(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.env_vars.insert(key.into(), value.into());
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
        assert!(ctx.env_vars.is_empty());
    }

    #[test]
    fn test_tool_context_with_env() {
        let mut vars = HashMap::new();
        vars.insert("FOO".into(), "bar".into());
        let ctx = ToolContext::new(PathBuf::from("/tmp")).with_env(vars);
        assert_eq!(ctx.env_vars.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_tool_context_set_env() {
        let mut ctx = ToolContext::new(PathBuf::from("/tmp"));
        ctx.set_env("KEY", "value");
        assert_eq!(ctx.env_vars.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_tool_context_serde() {
        let mut ctx = ToolContext::new(PathBuf::from("/tmp"));
        ctx.set_env("A", "1");
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: ToolContext = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.working_dir, PathBuf::from("/tmp"));
        assert_eq!(parsed.env_vars.get("A"), Some(&"1".to_string()));
    }
}

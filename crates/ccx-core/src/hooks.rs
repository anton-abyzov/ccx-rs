use serde::{Deserialize, Serialize};

/// A hook that runs a shell command before or after a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub event: HookEvent,
    pub pattern: Option<String>,
    pub command: String,
}

/// When a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    PreTool,
    PostTool,
    PreMessage,
    PostMessage,
}

/// Result of executing a hook.
#[derive(Debug, Clone)]
pub struct HookResult {
    pub hook_command: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// A set of registered hooks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookRegistry {
    pub hooks: Vec<Hook>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn add(&mut self, hook: Hook) {
        self.hooks.push(hook);
    }

    /// Get hooks that match the given event and optional tool name.
    pub fn matching(&self, event: HookEvent, tool_name: Option<&str>) -> Vec<&Hook> {
        self.hooks
            .iter()
            .filter(|h| {
                if h.event != event {
                    return false;
                }
                match (&h.pattern, tool_name) {
                    (Some(pattern), Some(name)) => glob::Pattern::new(pattern)
                        .map(|p| p.matches(name))
                        .unwrap_or(false),
                    (Some(_), None) => false,
                    (None, _) => true,
                }
            })
            .collect()
    }
}

/// Execute a hook command synchronously.
pub async fn run_hook(
    hook: &Hook,
    working_dir: &std::path::Path,
) -> Result<HookResult, std::io::Error> {
    let output = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(&hook.command)
        .current_dir(working_dir)
        .output()
        .await?;

    Ok(HookResult {
        hook_command: hook.command.clone(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_matching() {
        let mut registry = HookRegistry::new();
        registry.add(Hook {
            event: HookEvent::PreTool,
            pattern: Some("Bash*".into()),
            command: "echo pre-bash".into(),
        });
        registry.add(Hook {
            event: HookEvent::PostTool,
            pattern: None,
            command: "echo post-all".into(),
        });

        let pre_bash = registry.matching(HookEvent::PreTool, Some("Bash"));
        assert_eq!(pre_bash.len(), 1);

        let pre_read = registry.matching(HookEvent::PreTool, Some("Read"));
        assert_eq!(pre_read.len(), 0);

        let post_any = registry.matching(HookEvent::PostTool, Some("Read"));
        assert_eq!(post_any.len(), 1);
    }

    #[test]
    fn test_hook_serde() {
        let hook = Hook {
            event: HookEvent::PreTool,
            pattern: Some("Bash*".into()),
            command: "echo hi".into(),
        };
        let json = serde_json::to_string(&hook).unwrap();
        let parsed: Hook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event, HookEvent::PreTool);
        assert_eq!(parsed.command, "echo hi");
    }
}

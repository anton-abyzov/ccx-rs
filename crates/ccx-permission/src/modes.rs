use serde::{Deserialize, Serialize};

/// Permission modes controlling how tool calls are authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default: prompt for unrecognized tools.
    Default,
    /// Plan mode: read-only tools allowed, writes require approval.
    Plan,
    /// Bypass all permission checks.
    BypassPermissions,
    /// Never ask; deny anything not explicitly allowed.
    DontAsk,
    /// Auto-accept file edits.
    AcceptEdits,
    /// Full auto mode: allow everything.
    Auto,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Default
    }
}

impl PermissionMode {
    /// Whether this mode auto-allows read-only tools.
    pub fn allows_reads(&self) -> bool {
        !matches!(self, Self::DontAsk)
    }

    /// Whether this mode auto-allows write tools without prompting.
    pub fn allows_writes(&self) -> bool {
        matches!(self, Self::BypassPermissions | Self::Auto)
    }

    /// Whether this mode auto-allows file edits without prompting.
    pub fn allows_edits(&self) -> bool {
        matches!(self, Self::BypassPermissions | Self::AcceptEdits | Self::Auto)
    }

    /// Whether this mode auto-allows bash commands without prompting.
    pub fn allows_bash(&self) -> bool {
        matches!(self, Self::BypassPermissions | Self::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode() {
        let mode = PermissionMode::default();
        assert_eq!(mode, PermissionMode::Default);
        assert!(mode.allows_reads());
        assert!(!mode.allows_writes());
    }

    #[test]
    fn test_bypass_mode() {
        let mode = PermissionMode::BypassPermissions;
        assert!(mode.allows_reads());
        assert!(mode.allows_writes());
        assert!(mode.allows_edits());
        assert!(mode.allows_bash());
    }

    #[test]
    fn test_accept_edits_mode() {
        let mode = PermissionMode::AcceptEdits;
        assert!(mode.allows_edits());
        assert!(!mode.allows_bash());
    }

    #[test]
    fn test_plan_mode() {
        let mode = PermissionMode::Plan;
        assert!(mode.allows_reads());
        assert!(!mode.allows_writes());
        assert!(!mode.allows_bash());
    }

    #[test]
    fn test_serde_roundtrip() {
        let mode = PermissionMode::AcceptEdits;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"acceptEdits\"");
        let parsed: PermissionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, mode);
    }
}

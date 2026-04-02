use crate::modes::PermissionMode;
use crate::rules::{PermissionDecision, RuleSet};

/// Tool category for permission classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    ReadOnly,
    FileEdit,
    FileWrite,
    Bash,
    Network,
    Agent,
    Other,
}

/// Classify a tool name into its category.
pub fn classify_tool(tool_name: &str) -> ToolCategory {
    match tool_name {
        "Read" | "Glob" | "Grep" => ToolCategory::ReadOnly,
        "Edit" => ToolCategory::FileEdit,
        "Write" => ToolCategory::FileWrite,
        "Bash" => ToolCategory::Bash,
        "WebFetch" => ToolCategory::Network,
        "Agent" => ToolCategory::Agent,
        _ => ToolCategory::Other,
    }
}

/// Decide whether a tool call should be allowed, denied, or needs prompting.
pub fn decide(
    mode: PermissionMode,
    rules: &RuleSet,
    tool_name: &str,
    tool_call_str: &str,
) -> PermissionDecision {
    // Explicit rules always take precedence.
    let rule_decision = rules.evaluate(tool_call_str);
    if rule_decision != PermissionDecision::Ask {
        return rule_decision;
    }

    // Fall back to mode-based classification.
    let category = classify_tool(tool_name);
    match category {
        ToolCategory::ReadOnly if mode.allows_reads() => PermissionDecision::Allow,
        ToolCategory::FileEdit if mode.allows_edits() => PermissionDecision::Allow,
        ToolCategory::FileWrite if mode.allows_writes() => PermissionDecision::Allow,
        ToolCategory::Bash if mode.allows_bash() => PermissionDecision::Allow,
        ToolCategory::Network if mode.allows_writes() => PermissionDecision::Allow,
        ToolCategory::Agent if mode.allows_writes() => PermissionDecision::Allow,
        _ if matches!(
            mode,
            PermissionMode::BypassPermissions | PermissionMode::Auto
        ) =>
        {
            PermissionDecision::Allow
        }
        _ if matches!(mode, PermissionMode::DontAsk) => PermissionDecision::Deny,
        _ => PermissionDecision::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tools() {
        assert_eq!(classify_tool("Read"), ToolCategory::ReadOnly);
        assert_eq!(classify_tool("Bash"), ToolCategory::Bash);
        assert_eq!(classify_tool("Edit"), ToolCategory::FileEdit);
        assert_eq!(classify_tool("Write"), ToolCategory::FileWrite);
        assert_eq!(classify_tool("Unknown"), ToolCategory::Other);
    }

    #[test]
    fn test_default_mode_allows_reads() {
        let rules = RuleSet::new();
        let decision = decide(PermissionMode::Default, &rules, "Read", "Read(/tmp/f)");
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn test_default_mode_asks_for_bash() {
        let rules = RuleSet::new();
        let decision = decide(PermissionMode::Default, &rules, "Bash", "Bash(rm -rf /)");
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn test_explicit_rule_overrides_mode() {
        let mut rules = RuleSet::new();
        rules.add("Bash(git *)", crate::rules::RuleEffect::Allow);
        let decision = decide(PermissionMode::Default, &rules, "Bash", "Bash(git status)");
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn test_bypass_allows_everything() {
        let rules = RuleSet::new();
        let decision = decide(
            PermissionMode::BypassPermissions,
            &rules,
            "Bash",
            "Bash(rm -rf /)",
        );
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn test_dont_ask_denies_unknown() {
        let rules = RuleSet::new();
        let decision = decide(PermissionMode::DontAsk, &rules, "Bash", "Bash(echo hi)");
        assert_eq!(decision, PermissionDecision::Deny);
    }
}

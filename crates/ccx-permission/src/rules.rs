use serde::{Deserialize, Serialize};

/// A permission rule matching tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Glob pattern to match against tool call strings (e.g. "Bash(git *)").
    pub pattern: String,
    /// Whether this rule allows or denies the matched call.
    pub effect: RuleEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleEffect {
    Allow,
    Deny,
}

/// Result of evaluating permissions for a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

/// A set of permission rules evaluated in order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleSet {
    pub rules: Vec<PermissionRule>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add(&mut self, pattern: impl Into<String>, effect: RuleEffect) {
        self.rules.push(PermissionRule {
            pattern: pattern.into(),
            effect,
        });
    }

    /// Evaluate a tool call string against the rules. First match wins.
    pub fn evaluate(&self, tool_call: &str) -> PermissionDecision {
        for rule in &self.rules {
            if matches_pattern(&rule.pattern, tool_call) {
                return match rule.effect {
                    RuleEffect::Allow => PermissionDecision::Allow,
                    RuleEffect::Deny => PermissionDecision::Deny,
                };
            }
        }
        PermissionDecision::Ask
    }
}

/// Simple glob-style matching: `*` matches any sequence of chars.
fn matches_pattern(pattern: &str, input: &str) -> bool {
    let glob_pat = glob::Pattern::new(pattern);
    match glob_pat {
        Ok(p) => p.matches(input),
        Err(_) => pattern == input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_allow() {
        let mut rules = RuleSet::new();
        rules.add("Bash(git *)", RuleEffect::Allow);
        assert_eq!(
            rules.evaluate("Bash(git status)"),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn test_rule_deny() {
        let mut rules = RuleSet::new();
        rules.add("Bash(rm *)", RuleEffect::Deny);
        assert_eq!(rules.evaluate("Bash(rm -rf /)"), PermissionDecision::Deny);
    }

    #[test]
    fn test_rule_ask_no_match() {
        let rules = RuleSet::new();
        assert_eq!(rules.evaluate("Bash(echo hello)"), PermissionDecision::Ask);
    }

    #[test]
    fn test_first_match_wins() {
        let mut rules = RuleSet::new();
        rules.add("Bash(git *)", RuleEffect::Allow);
        rules.add("Bash(*)", RuleEffect::Deny);
        assert_eq!(rules.evaluate("Bash(git push)"), PermissionDecision::Allow);
        assert_eq!(rules.evaluate("Bash(ls)"), PermissionDecision::Deny);
    }
}

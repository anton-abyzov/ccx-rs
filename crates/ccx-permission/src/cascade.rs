use crate::modes::PermissionMode;
use crate::rules::{RuleEffect, RuleSet};
use serde::{Deserialize, Serialize};

/// A layer in the settings cascade.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Merge settings layers: CLI > session > project > user > defaults.
/// Earlier layers (higher priority) override later ones.
pub fn merge_cascade(layers: &[PermissionSettings]) -> (PermissionMode, RuleSet) {
    let mut mode = PermissionMode::Default;
    let mut rules = RuleSet::new();

    // First non-None mode wins.
    for layer in layers {
        if let Some(m) = layer.mode {
            mode = m;
            break;
        }
    }

    // Rules accumulate from all layers (higher priority first).
    for layer in layers {
        for pattern in &layer.deny {
            rules.add(pattern.clone(), RuleEffect::Deny);
        }
        for pattern in &layer.allow {
            rules.add(pattern.clone(), RuleEffect::Allow);
        }
    }

    (mode, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::PermissionDecision;

    #[test]
    fn test_empty_cascade() {
        let (mode, rules) = merge_cascade(&[]);
        assert_eq!(mode, PermissionMode::Default);
        assert_eq!(rules.evaluate("anything"), PermissionDecision::Ask);
    }

    #[test]
    fn test_mode_from_first_layer() {
        let layers = vec![
            PermissionSettings {
                mode: Some(PermissionMode::Auto),
                ..Default::default()
            },
            PermissionSettings {
                mode: Some(PermissionMode::Plan),
                ..Default::default()
            },
        ];
        let (mode, _) = merge_cascade(&layers);
        assert_eq!(mode, PermissionMode::Auto);
    }

    #[test]
    fn test_mode_falls_through() {
        let layers = vec![
            PermissionSettings::default(),
            PermissionSettings {
                mode: Some(PermissionMode::Plan),
                ..Default::default()
            },
        ];
        let (mode, _) = merge_cascade(&layers);
        assert_eq!(mode, PermissionMode::Plan);
    }

    #[test]
    fn test_rules_accumulate() {
        let layers = vec![
            PermissionSettings {
                allow: vec!["Bash(git *)".into()],
                ..Default::default()
            },
            PermissionSettings {
                deny: vec!["Bash(rm *)".into()],
                ..Default::default()
            },
        ];
        let (_, rules) = merge_cascade(&layers);
        // Deny rules from earlier layer come first; but here deny is from layer 2.
        // The actual ordering: deny "Bash(rm *)" then allow "Bash(git *)".
        assert_eq!(rules.evaluate("Bash(rm -rf /)"), PermissionDecision::Deny);
    }
}

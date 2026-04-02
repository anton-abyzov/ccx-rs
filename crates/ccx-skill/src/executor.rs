use crate::loader::{Skill, SkillMode};

/// Result of executing a skill.
#[derive(Debug, Clone)]
pub struct SkillResult {
    pub skill_name: String,
    pub expanded_prompt: String,
    pub mode: SkillMode,
}

/// Expand a skill into a prompt for inline execution.
pub fn expand_skill(skill: &Skill, args: Option<&str>) -> SkillResult {
    let expanded = if let Some(args) = args {
        format!("{}\n\nArguments: {args}", skill.prompt)
    } else {
        skill.prompt.clone()
    };

    SkillResult {
        skill_name: skill.name.clone(),
        expanded_prompt: expanded,
        mode: skill.mode,
    }
}

/// Find a skill by name or trigger keyword.
pub fn find_skill<'a>(skills: &'a [Skill], query: &str) -> Option<&'a Skill> {
    // First try exact name match.
    if let Some(skill) = skills.iter().find(|s| s.name == query) {
        return Some(skill);
    }
    // Then try trigger keywords.
    skills.iter().find(|s| s.trigger.iter().any(|t| t == query))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn make_skill(name: &str, prompt: &str, triggers: Vec<&str>) -> Skill {
        Skill {
            name: name.into(),
            description: String::new(),
            trigger: triggers.into_iter().map(String::from).collect(),
            mode: SkillMode::Inline,
            prompt: prompt.into(),
            source_path: PathBuf::from("test.md"),
        }
    }

    #[test]
    fn test_expand_skill_no_args() {
        let skill = make_skill("test", "Do the thing", vec![]);
        let result = expand_skill(&skill, None);
        assert_eq!(result.expanded_prompt, "Do the thing");
        assert_eq!(result.skill_name, "test");
    }

    #[test]
    fn test_expand_skill_with_args() {
        let skill = make_skill("test", "Do the thing", vec![]);
        let result = expand_skill(&skill, Some("--verbose"));
        assert!(result.expanded_prompt.contains("--verbose"));
    }

    #[test]
    fn test_find_skill_by_name() {
        let skills = vec![
            make_skill("commit", "Commit changes", vec!["git-commit"]),
            make_skill("review", "Review PR", vec!["pr-review"]),
        ];
        let found = find_skill(&skills, "commit");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "commit");
    }

    #[test]
    fn test_find_skill_by_trigger() {
        let skills = vec![make_skill("commit", "Commit", vec!["git-commit"])];
        let found = find_skill(&skills, "git-commit");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "commit");
    }

    #[test]
    fn test_find_skill_not_found() {
        let skills = vec![make_skill("commit", "Commit", vec![])];
        assert!(find_skill(&skills, "nonexistent").is_none());
    }
}

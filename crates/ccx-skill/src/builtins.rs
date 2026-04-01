//! Built-in skills bundled directly in the binary.
//!
//! These mirror Claude Code's built-in slash-command skills that aren't
//! loaded from disk — they're part of the binary itself.

use std::path::PathBuf;

use crate::loader::{Skill, SkillMode};

/// A statically-defined built-in skill.
pub struct BuiltinSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub content: &'static str,
}

pub const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "simplify",
        description: "Review changed code for reuse, quality, and efficiency, then fix any issues found",
        content: r#"Review the code changes in the current working directory for:
1. **Code reuse** — Are there duplicate patterns that could be extracted into shared functions?
2. **Quality** — Are there obvious bugs, missing error handling, or anti-patterns?
3. **Efficiency** — Are there unnecessary allocations, redundant computations, or N+1 queries?
4. **Readability** — Are variable names clear? Is the code self-documenting?

For each issue found:
- Explain what's wrong
- Show the fix
- Apply the fix immediately

Use `git diff` to see what changed, then review each changed file."#,
    },
    BuiltinSkill {
        name: "batch",
        description: "Decompose work into parallel agents with worktree isolation, each producing its own PR",
        content: r#"Decompose the given task into independent sub-tasks that can be executed in parallel.

For each sub-task:
1. Create a descriptive name
2. Define the scope (which files/modules)
3. Spawn an Agent to execute it

Use the Agent tool to spawn each sub-task in parallel. Each agent should:
- Work on its assigned files only
- Run tests after making changes
- Report completion

Coordinate the results and merge when all agents complete."#,
    },
    BuiltinSkill {
        name: "commit",
        description: "Create a git commit with an auto-generated message based on the changes",
        content: r#"1. Run `git status` and `git diff --staged` to see what's being committed
2. If nothing is staged, run `git add -A` to stage all changes
3. Analyze the changes and generate a concise commit message:
   - One line, under 72 characters
   - Describe WHAT changed and WHY
   - Use imperative mood ("add", "fix", "update", not "added", "fixed")
4. Run `git commit -m "<message>"`
5. Show the result"#,
    },
    BuiltinSkill {
        name: "review",
        description: "Review code changes for bugs, security issues, and best practices",
        content: r#"Review the current code changes for:
1. **Bugs** — Logic errors, off-by-one, null pointer risks
2. **Security** — SQL injection, XSS, command injection, secrets in code
3. **Performance** — Unnecessary allocations, O(n²) where O(n) is possible
4. **Style** — Consistent naming, idiomatic patterns for the language
5. **Tests** — Are changes covered by tests?

Use `git diff` to see changes. For each issue, explain the problem and suggest a fix."#,
    },
    BuiltinSkill {
        name: "test",
        description: "Run the project's test suite and report results",
        content: r#"Detect the project type and run the appropriate test command:
- Rust: `cargo test`
- Go: `go test ./...`
- Python: `pytest` or `python -m pytest`
- Node.js: `npm test` or `npx vitest run`
- .NET: `dotnet test`

Report: total tests, passed, failed, and any error details for failures."#,
    },
];

/// Convert built-in skills into `Skill` structs for discovery.
pub fn builtin_skills() -> Vec<Skill> {
    BUILTIN_SKILLS
        .iter()
        .map(|b| Skill {
            name: b.name.to_string(),
            description: b.description.to_string(),
            trigger: Vec::new(),
            mode: SkillMode::Inline,
            prompt: b.content.to_string(),
            source_path: PathBuf::from("<builtin>"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_skills_count() {
        assert_eq!(BUILTIN_SKILLS.len(), 5);
    }

    #[test]
    fn test_builtin_skills_conversion() {
        let skills = builtin_skills();
        assert_eq!(skills.len(), 5);
        assert_eq!(skills[0].name, "simplify");
        assert_eq!(skills[4].name, "test");
        assert!(skills.iter().all(|s| s.source_path.to_str() == Some("<builtin>")));
    }

    #[test]
    fn test_builtin_simplify_has_content() {
        let skills = builtin_skills();
        let simplify = skills.iter().find(|s| s.name == "simplify").unwrap();
        assert!(simplify.prompt.contains("Code reuse"));
        assert!(simplify.prompt.contains("git diff"));
    }
}

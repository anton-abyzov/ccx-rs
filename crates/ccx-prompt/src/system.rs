use crate::ClaudeMdFile;

/// Build a comprehensive system prompt for the Claude agent.
///
/// Components:
/// 1. Role description and capabilities
/// 2. Environment info (working dir, OS, shell)
/// 3. Available tools with their schemas
/// 4. Available skills with routing hints
/// 5. CLAUDE.md content injection
/// 6. Behavioral guidelines
pub fn build_full_system_prompt(
    claude_md_files: &[ClaudeMdFile],
    working_dir: &str,
    tool_schemas: &[ToolSchema],
    skills: &[SkillInfo],
) -> String {
    let mut parts = Vec::new();

    // 1. Role description.
    parts.push(ROLE_DESCRIPTION.to_string());

    // 2. Environment.
    parts.push(build_environment_section(working_dir));

    // 3. Tool descriptions.
    if !tool_schemas.is_empty() {
        parts.push(build_tools_section(tool_schemas));
    }

    // 4. Skills with routing hints.
    if !skills.is_empty() {
        parts.push(build_skills_section(skills));
    }

    // 5. CLAUDE.md content.
    if !claude_md_files.is_empty() {
        parts.push(build_claude_md_section(claude_md_files));
    }

    // 6. Behavioral guidelines.
    parts.push(GUIDELINES.to_string());

    parts.join("\n")
}

/// Tool schema for inclusion in the system prompt, including the full JSON input schema.
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
}

/// Lightweight skill info for system prompt injection (no full prompt content).
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

const ROLE_DESCRIPTION: &str = "\
You are an AI coding assistant built with ccx (Claude Code Extended). \
You help users with software engineering tasks including writing code, debugging, \
refactoring, explaining code, running commands, and navigating codebases.

You have access to tools that let you read and write files, execute bash commands, \
search codebases, fetch web content, and more. Use them to accomplish tasks effectively.

# Key Principles
- Read before writing: always read a file before modifying it
- Be precise: make targeted changes, don't rewrite entire files unnecessarily
- Verify: run tests and check results after making changes
- Be safe: avoid destructive operations without confirmation
- Be concise: lead with the answer, not the reasoning";

const GUIDELINES: &str = "
# Output Guidelines
- Keep responses concise and direct
- Use markdown formatting when helpful
- Show code changes as diffs when the context is clear
- Report errors clearly with actionable next steps
- When exploring the codebase, summarize findings concisely. Do not dump entire file contents to the user — provide a brief summary of what you found.

# Action-Oriented Behavior
When working on a task, do NOT explain your thought process out loud. Just act.
Do NOT say \"I need to...\", \"Let me...\", \"I'll use...\" — just call the tool directly.
Be concise. Output only the final result, not your reasoning steps.
Do NOT narrate what you are about to do — just do it.

# Safety
- Never execute destructive commands (rm -rf, force push) without explicit confirmation
- Don't expose secrets, API keys, or credentials in output
- Validate file paths before writing
- Respect .gitignore and don't commit sensitive files";

fn build_environment_section(working_dir: &str) -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());

    let is_git = std::path::Path::new(working_dir).join(".git").exists();

    let mut section = format!(
        "\n# Environment\n\
         - Working directory: {working_dir}\n\
         - Platform: {os} ({arch})\n\
         - Shell: {shell}\n"
    );

    if is_git {
        section.push_str("- Git repository: yes\n");
    }

    section
}

fn build_tools_section(tools: &[ToolSchema]) -> String {
    let mut section = String::from("\n# Available Tools\n\n");
    for tool in tools {
        section.push_str(&format!("### {}\n{}\n\n", tool.name, tool.description));
        if let Some(ref schema) = tool.input_schema
            && let Ok(pretty) = serde_json::to_string_pretty(schema)
        {
            section.push_str(&format!("Input schema:\n```json\n{pretty}\n```\n\n"));
        }
    }
    section
}

fn build_skills_section(skills: &[SkillInfo]) -> String {
    let mut s = String::from("\n## Available Skills (invoke via slash command)\n\n");
    s += "When the user asks for something that matches a skill, suggest using the skill command instead of doing it manually.\n\n";

    // Group skills by category.
    let builtins: Vec<_> = skills.iter().filter(|sk| !sk.name.contains(':')).collect();
    let sw_skills: Vec<_> = skills
        .iter()
        .filter(|sk| sk.name.starts_with("sw:"))
        .collect();
    let other_skills: Vec<_> = skills
        .iter()
        .filter(|sk| sk.name.contains(':') && !sk.name.starts_with("sw:"))
        .collect();

    if !builtins.is_empty() {
        s += "### Built-in:\n";
        for sk in &builtins {
            s += &format!(
                "- `/{name}` — {desc}\n",
                name = sk.name,
                desc = truncate_desc(&sk.description, 80)
            );
        }
        s += "\n";
    }

    if !sw_skills.is_empty() {
        s += "### SpecWeave (project management):\n";
        // Only include the most important ones to save tokens.
        const IMPORTANT: &[&str] = &[
            "sw:increment",
            "sw:do",
            "sw:done",
            "sw:team-lead",
            "sw:auto",
            "sw:grill",
            "sw:architect",
            "sw:pm",
            "sw:progress",
            "sw:brainstorm",
            "sw:help",
            "sw:validate",
            "sw:code-reviewer",
        ];
        for sk in &sw_skills {
            if IMPORTANT.contains(&sk.name.as_str()) {
                s += &format!(
                    "- `/{name}` — {desc}\n",
                    name = sk.name,
                    desc = truncate_desc(&sk.description, 80)
                );
            }
        }
        s += &format!(
            "\n{} more SpecWeave skills available. Type `/help` to see all.\n\n",
            sw_skills.len()
        );
    }

    if !other_skills.is_empty() {
        s += "### Other plugins:\n";
        for sk in &other_skills {
            s += &format!(
                "- `/{name}` — {desc}\n",
                name = sk.name,
                desc = truncate_desc(&sk.description, 80)
            );
        }
        s += "\n";
    }

    s += "### Routing rules:\n";
    s += "- \"create an increment\" / \"plan feature\" → suggest `/sw:increment`\n";
    s += "- \"simplify code\" / \"review changes\" → suggest `/simplify` or `/review`\n";
    s += "- \"run tests\" → suggest `/test`\n";
    s += "- \"commit changes\" → suggest `/commit`\n";
    s += "- \"what's next\" / \"status\" → suggest `/sw:progress`\n";
    s += "- \"team\" / \"parallel agents\" → suggest `/sw:team-lead`\n";
    s += "\n";
    s += "### IMPORTANT — Skill invocation rules:\n";
    s += "Skills (commands starting with /) are invoked by the USER in the chat input, NOT by you.\n";
    s += "Do NOT try to run skills via the Bash tool. If the user asks for something that matches a skill,\n";
    s += "tell them to type the slash command. For example:\n";
    s += "  - \"Create an increment\" → Tell user: \"Type `/sw:increment` to create an increment\"\n";
    s += "  - \"Simplify the code\" → Tell user: \"Type `/simplify` to run code simplification\"\n";
    s += "You can only use the tools listed in the Available Tools section above.\n";

    s
}

/// Truncate a description to a max length with "..." if needed.
fn truncate_desc(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max.saturating_sub(3);
        // Ensure we don't split a multi-byte char.
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

fn build_claude_md_section(files: &[ClaudeMdFile]) -> String {
    let mut section = String::from("\n# User Instructions\n");
    for file in files {
        section.push_str(&format!(
            "\nContents of {}:\n\n{}\n",
            file.path.display(),
            file.content
        ));
    }
    section
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_build_full_system_prompt_empty() {
        let prompt = build_full_system_prompt(&[], "/tmp", &[], &[]);
        assert!(prompt.contains("AI coding assistant"));
        assert!(prompt.contains("/tmp"));
        assert!(prompt.contains("Safety"));
    }

    #[test]
    fn test_build_full_system_prompt_with_tools() {
        let tools = vec![
            ToolSchema {
                name: "Bash".into(),
                description: "Execute bash commands".into(),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                })),
            },
            ToolSchema {
                name: "Read".into(),
                description: "Read files".into(),
                input_schema: None,
            },
        ];
        let prompt = build_full_system_prompt(&[], "/tmp", &tools, &[]);
        assert!(prompt.contains("Available Tools"));
        assert!(prompt.contains("### Bash"));
        assert!(prompt.contains("### Read"));
        // Bash has schema — verify it appears.
        assert!(prompt.contains("Input schema:"));
        assert!(prompt.contains("\"command\""));
    }

    #[test]
    fn test_build_full_system_prompt_with_claude_md() {
        let files = vec![ClaudeMdFile {
            path: PathBuf::from("/project/CLAUDE.md"),
            content: "# My Rules\nAlways use TypeScript.".into(),
        }];
        let prompt = build_full_system_prompt(&files, "/project", &[], &[]);
        assert!(prompt.contains("User Instructions"));
        assert!(prompt.contains("My Rules"));
        assert!(prompt.contains("TypeScript"));
    }

    #[test]
    fn test_build_full_system_prompt_all_components() {
        let files = vec![ClaudeMdFile {
            path: PathBuf::from("/home/CLAUDE.md"),
            content: "Be helpful.".into(),
        }];
        let tools = vec![ToolSchema {
            name: "Grep".into(),
            description: "Search code".into(),
            input_schema: None,
        }];
        let prompt = build_full_system_prompt(&files, "/home/project", &tools, &[]);

        // All sections present.
        assert!(prompt.contains("AI coding assistant"));
        assert!(prompt.contains("Environment"));
        assert!(prompt.contains("Available Tools"));
        assert!(prompt.contains("User Instructions"));
        assert!(prompt.contains("Safety"));
    }

    #[test]
    fn test_build_environment_section() {
        let section = build_environment_section("/tmp/test");
        assert!(section.contains("/tmp/test"));
        assert!(section.contains("Platform:"));
    }

    #[test]
    fn test_build_tools_section() {
        let tools = vec![ToolSchema {
            name: "Test".into(),
            description: "A test tool".into(),
            input_schema: Some(serde_json::json!({"type": "object"})),
        }];
        let section = build_tools_section(&tools);
        assert!(section.contains("### Test"));
        assert!(section.contains("A test tool"));
        assert!(section.contains("Input schema:"));
    }

    #[test]
    fn test_build_claude_md_section() {
        let files = vec![
            ClaudeMdFile {
                path: PathBuf::from("/a/CLAUDE.md"),
                content: "Rule A".into(),
            },
            ClaudeMdFile {
                path: PathBuf::from("/b/CLAUDE.md"),
                content: "Rule B".into(),
            },
        ];
        let section = build_claude_md_section(&files);
        assert!(section.contains("Rule A"));
        assert!(section.contains("Rule B"));
        assert!(section.contains("/a/CLAUDE.md"));
    }

    #[test]
    fn test_build_skills_section() {
        let skills = vec![
            SkillInfo {
                name: "commit".into(),
                description: "Auto-generated git commits".into(),
            },
            SkillInfo {
                name: "sw:increment".into(),
                description: "Plan and create SpecWeave increments".into(),
            },
            SkillInfo {
                name: "sw:do".into(),
                description: "Execute increment tasks".into(),
            },
            SkillInfo {
                name: "fro:frontend-design".into(),
                description: "Create frontend interfaces".into(),
            },
        ];
        let section = build_skills_section(&skills);
        assert!(section.contains("Available Skills"));
        assert!(section.contains("/commit"));
        assert!(section.contains("/sw:increment"));
        assert!(section.contains("Built-in:"));
        assert!(section.contains("SpecWeave"));
        assert!(section.contains("Other plugins:"));
        assert!(section.contains("Routing rules:"));
    }

    #[test]
    fn test_build_full_system_prompt_with_skills() {
        let skills = vec![SkillInfo {
            name: "sw:increment".into(),
            description: "Plan features".into(),
        }];
        let prompt = build_full_system_prompt(&[], "/tmp", &[], &skills);
        assert!(prompt.contains("Available Skills"));
        assert!(prompt.contains("/sw:increment"));
    }

    #[test]
    fn test_truncate_desc() {
        assert_eq!(truncate_desc("short", 10), "short");
        assert_eq!(
            truncate_desc("this is a long description", 15),
            "this is a lo..."
        );
    }
}

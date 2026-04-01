use crate::ClaudeMdFile;

/// Build a comprehensive system prompt for the Claude agent.
///
/// Components:
/// 1. Role description and capabilities
/// 2. Environment info (working dir, OS, shell)
/// 3. Available tools with their schemas
/// 4. CLAUDE.md content injection
pub fn build_full_system_prompt(
    claude_md_files: &[ClaudeMdFile],
    working_dir: &str,
    tool_schemas: &[ToolSchema],
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

    // 4. CLAUDE.md content.
    if !claude_md_files.is_empty() {
        parts.push(build_claude_md_section(claude_md_files));
    }

    // 5. Behavioral guidelines.
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

const ROLE_DESCRIPTION: &str = "\
You are an AI coding assistant built with ccx (Claude Code in Rust). \
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
        if let Some(ref schema) = tool.input_schema {
            if let Ok(pretty) = serde_json::to_string_pretty(schema) {
                section.push_str(&format!("Input schema:\n```json\n{pretty}\n```\n\n"));
            }
        }
    }
    section
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
        let prompt = build_full_system_prompt(&[], "/tmp", &[]);
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
        let prompt = build_full_system_prompt(&[], "/tmp", &tools);
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
        let prompt = build_full_system_prompt(&files, "/project", &[]);
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
        let prompt = build_full_system_prompt(&files, "/home/project", &tools);

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
}

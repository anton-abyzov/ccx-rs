pub mod claudemd;
pub mod system;

pub use claudemd::{discover_claude_md, ClaudeMdFile};
pub use system::{build_full_system_prompt, SkillInfo, ToolSchema};

/// Build the system prompt from components (legacy simple version).
pub fn build_system_prompt(claude_md_files: &[ClaudeMdFile], working_dir: &str) -> String {
    build_full_system_prompt(claude_md_files, working_dir, &[], &[])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_build_system_prompt_empty() {
        let prompt = build_system_prompt(&[], "/tmp");
        assert!(prompt.contains("AI coding assistant"));
        assert!(prompt.contains("/tmp"));
    }

    #[test]
    fn test_build_system_prompt_with_claude_md() {
        let files = vec![ClaudeMdFile {
            path: PathBuf::from("/home/user/CLAUDE.md"),
            content: "# My Rules\nBe helpful.".to_string(),
        }];
        let prompt = build_system_prompt(&files, "/home/user/project");
        assert!(prompt.contains("My Rules"));
        assert!(prompt.contains("Be helpful"));
    }
}

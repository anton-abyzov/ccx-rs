pub mod claudemd;

pub use claudemd::{discover_claude_md, ClaudeMdFile};

/// Build the system prompt from components.
pub fn build_system_prompt(claude_md_files: &[ClaudeMdFile], working_dir: &str) -> String {
    let mut parts = Vec::new();

    parts.push(
        "You are an AI coding assistant. You help users with software engineering tasks."
            .to_string(),
    );
    parts.push(format!(
        "\n\n# Environment\n- Working directory: {working_dir}\n- Platform: {}\n",
        std::env::consts::OS
    ));

    if !claude_md_files.is_empty() {
        parts.push("\n# User Instructions\n".to_string());
        for file in claude_md_files {
            parts.push(format!(
                "Contents of {}:\n\n{}\n",
                file.path.display(),
                file.content
            ));
        }
    }

    parts.join("")
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

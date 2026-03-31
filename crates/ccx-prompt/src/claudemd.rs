use std::fs;
use std::path::{Path, PathBuf};

/// A discovered CLAUDE.md file with its content.
#[derive(Debug, Clone)]
pub struct ClaudeMdFile {
    pub path: PathBuf,
    pub content: String,
}

/// Discover CLAUDE.md files by walking from the given directory up to root.
/// Also checks ~/.claude/CLAUDE.md for the global config.
/// Returns files ordered: global first, project-specific last.
pub fn discover_claude_md(start_dir: &Path) -> Vec<ClaudeMdFile> {
    let mut files = Vec::new();
    let mut current = Some(start_dir.to_path_buf());

    while let Some(dir) = current {
        let candidate = dir.join("CLAUDE.md");
        if candidate.is_file() {
            if let Ok(content) = fs::read_to_string(&candidate) {
                files.push(ClaudeMdFile {
                    path: candidate,
                    content,
                });
            }
        }
        current = dir.parent().map(|p| p.to_path_buf());
    }

    // Also check ~/.claude/CLAUDE.md
    if let Some(home) = home_dir() {
        let global = home.join(".claude").join("CLAUDE.md");
        if global.is_file() && !files.iter().any(|f| f.path == global) {
            if let Ok(content) = fs::read_to_string(&global) {
                files.push(ClaudeMdFile {
                    path: global,
                    content,
                });
            }
        }
    }

    // Reverse: global first, project-specific last.
    files.reverse();
    files
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_from_nonexistent() {
        let files = discover_claude_md(Path::new("/nonexistent/path"));
        // Should not panic — may find global CLAUDE.md or be empty.
        let _ = files;
    }

    #[test]
    fn test_discover_in_temp() {
        let dir = std::env::temp_dir().join("ccx_test_claudemd");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("CLAUDE.md"), "# Test\nHello").unwrap();

        let files = discover_claude_md(&dir);
        assert!(files.iter().any(|f| f.content.contains("# Test")));

        let _ = fs::remove_dir_all(&dir);
    }
}

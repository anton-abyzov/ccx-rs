use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A loaded skill definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub trigger: Vec<String>,
    #[serde(default)]
    pub mode: SkillMode,
    pub prompt: String,
    pub source_path: PathBuf,
}

/// How the skill is executed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillMode {
    #[default]
    Inline,
    Agent,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("skill not found: {0}")]
    NotFound(String),
}

/// Load a skill from a markdown file with YAML frontmatter.
pub fn load_skill(path: &Path) -> Result<Skill, SkillError> {
    let raw = fs::read_to_string(path)?;
    parse_skill_content(&raw, path)
}

/// Parse skill content from markdown with YAML frontmatter.
///
/// The name is derived from the directory path if not present in frontmatter,
/// since SpecWeave SKILL.md files don't include a `name:` field.
fn parse_skill_content(raw: &str, source_path: &Path) -> Result<Skill, SkillError> {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return Err(SkillError::Parse("missing frontmatter".into()));
    };

    let Some(end) = rest.find("\n---") else {
        return Err(SkillError::Parse("unterminated frontmatter".into()));
    };

    let frontmatter = &rest[..end];
    let prompt = rest[end + 4..].trim().to_string();

    let mut name = extract_field(frontmatter, "name");
    let description = extract_field(frontmatter, "description");
    let mode_str = extract_field(frontmatter, "mode");
    let trigger_str = extract_field(frontmatter, "trigger");

    // Derive name from directory path when frontmatter has no name field.
    if name.is_empty() {
        name = derive_skill_name(source_path).unwrap_or_default();
    }

    if name.is_empty() {
        return Err(SkillError::Parse("skill must have a name".into()));
    }

    let mode = match mode_str.as_str() {
        "agent" => SkillMode::Agent,
        _ => SkillMode::Inline,
    };

    let trigger: Vec<String> = if trigger_str.is_empty() {
        Vec::new()
    } else {
        trigger_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    Ok(Skill {
        name,
        description,
        trigger,
        mode,
        prompt,
        source_path: source_path.to_path_buf(),
    })
}

/// Derive a skill name from the file path.
///
/// - `plugins/specweave/skills/team-lead/SKILL.md` → `sw:team-lead`
/// - `plugins/{other}/skills/do/SKILL.md` → `{prefix}:do`
/// - `~/.claude/skills/nanobanana/SKILL.md` → `nanobanana`
/// - `.claude/skills/slack-messaging/SKILL.md` → `slack-messaging`
/// - `~/.claude/skills/foo.md` → `foo` (flat file)
fn derive_skill_name(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();

    // For SKILL.md in a subdirectory, use the parent directory name
    if path.file_name().and_then(|f| f.to_str()) == Some("SKILL.md") {
        let parent = path.parent()?;
        let dir_name = parent.file_name()?.to_str()?;

        if path_str.contains("plugins/specweave/skills/") {
            return Some(format!("sw:{}", dir_name));
        }
        if let Some(prefix) = extract_plugin_prefix(&path_str) {
            return Some(format!("{}:{}", prefix, dir_name));
        }
        return Some(dir_name.to_string());
    }

    // For flat .md files, use the filename without extension
    path.file_stem()?.to_str().map(|s| s.to_string())
}

/// Extract a short prefix from a plugin path.
/// `plugins/{name}/skills/` → first 3 chars of `{name}` as prefix.
fn extract_plugin_prefix(path: &str) -> Option<String> {
    let idx = path.find("plugins/")?;
    let after = &path[idx + 8..];
    let slash = after.find('/')?;
    let plugin_name = &after[..slash];
    if plugin_name == "specweave" {
        return None;
    }
    Some(plugin_name.chars().take(3).collect())
}

/// Load all skills from a directory.
pub fn load_skills_from_dir(dir: &Path) -> Result<Vec<Skill>, SkillError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            match load_skill(&path) {
                Ok(skill) => skills.push(skill),
                Err(_) => continue,
            }
        }
    }
    Ok(skills)
}

/// Discover all skills from standard locations:
/// - `~/.claude/skills/` (user-level, flat .md + subdirs with SKILL.md)
/// - `.claude/skills/` (project-level, flat .md + subdirs with SKILL.md)
/// - specweave plugin skills via nvm
pub fn discover_all_skills() -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // Add built-in skills first (simplify, batch, commit, review, test).
    for skill in crate::builtins::builtin_skills() {
        seen_names.insert(skill.name.clone());
        skills.push(skill);
    }

    let mut dirs: Vec<PathBuf> = Vec::new();

    // User-level skills
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".claude/skills"));
    }

    // Project-level skills
    dirs.push(PathBuf::from(".claude/skills"));

    for dir in &dirs {
        // Flat .md files
        if let Ok(loaded) = load_skills_from_dir(dir) {
            for skill in loaded {
                if seen_names.insert(skill.name.clone()) {
                    skills.push(skill);
                }
            }
        }
        // Subdirectory pattern: skill-name/SKILL.md
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let subdir = entry.path();
                if subdir.is_dir() {
                    let skill_file = subdir.join("SKILL.md");
                    if skill_file.exists() {
                        if let Ok(skill) = load_skill(&skill_file) {
                            if seen_names.insert(skill.name.clone()) {
                                skills.push(skill);
                            }
                        }
                    }
                }
            }
        }
    }

    // Specweave plugin skills via nvm
    if let Some(home) = dirs::home_dir() {
        let nvm_base = home.join(".nvm/versions/node");
        if let Ok(versions) = fs::read_dir(&nvm_base) {
            for ver in versions.flatten() {
                let plugins_dir = ver.path().join("lib/node_modules/specweave/plugins");
                scan_plugin_skills(&plugins_dir, &mut skills, &mut seen_names);
            }
        }
    }

    skills
}

fn scan_plugin_skills(
    plugins_dir: &Path,
    skills: &mut Vec<Skill>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(plugins) = fs::read_dir(plugins_dir) else {
        return;
    };
    for plugin in plugins.flatten() {
        let skills_dir = plugin.path().join("skills");
        if !skills_dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&skills_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let skill_file = entry.path().join("SKILL.md");
            if skill_file.exists() {
                if let Ok(skill) = load_skill(&skill_file) {
                    if seen.insert(skill.name.clone()) {
                        skills.push(skill);
                    }
                }
            }
        }
    }
}

fn extract_field(text: &str, key: &str) -> String {
    let prefix = format!("{key}: ");
    for line in text.lines() {
        if let Some(value) = line.strip_prefix(&prefix) {
            return value.trim().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill() {
        let content = "---\nname: test-skill\ndescription: A test\nmode: inline\ntrigger: foo, bar\n---\n\nDo the thing.\n";
        let skill = parse_skill_content(content, Path::new("test.md")).unwrap();
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test");
        assert_eq!(skill.mode, SkillMode::Inline);
        assert_eq!(skill.trigger, vec!["foo", "bar"]);
        assert_eq!(skill.prompt, "Do the thing.");
    }

    #[test]
    fn test_parse_skill_name_from_directory() {
        // SpecWeave SKILL.md files have no name: field — derive from path
        let content = "---\ndescription: Phase orchestrator\n---\n\nDo the thing.\n";
        let skill = parse_skill_content(
            content,
            Path::new("/home/user/.nvm/versions/node/v22/lib/node_modules/specweave/plugins/specweave/skills/team-lead/SKILL.md"),
        )
        .unwrap();
        assert_eq!(skill.name, "sw:team-lead");
        assert_eq!(skill.description, "Phase orchestrator");
    }

    #[test]
    fn test_parse_skill_name_user_skill() {
        let content = "---\ndescription: My custom skill\n---\n\nprompt\n";
        let skill = parse_skill_content(
            content,
            Path::new("/home/user/.claude/skills/nanobanana/SKILL.md"),
        )
        .unwrap();
        assert_eq!(skill.name, "nanobanana");
    }

    #[test]
    fn test_parse_skill_name_other_plugin() {
        let content = "---\ndescription: Frontend design\n---\n\nprompt\n";
        let skill = parse_skill_content(
            content,
            Path::new("/home/user/.nvm/versions/node/v22/lib/node_modules/specweave/plugins/frontend-design/skills/frontend-design/SKILL.md"),
        )
        .unwrap();
        assert_eq!(skill.name, "fro:frontend-design");
    }

    #[test]
    fn test_parse_skill_missing_name_no_path() {
        // Still fails if no name in frontmatter AND path can't derive one
        let content = "---\ndescription: no name\n---\n\nprompt\n";
        let err = parse_skill_content(content, Path::new("/")).unwrap_err();
        assert!(matches!(err, SkillError::Parse(_)));
    }

    #[test]
    fn test_parse_skill_explicit_name_wins() {
        // If frontmatter has name:, it takes precedence over path
        let content = "---\nname: custom-name\ndescription: A test\n---\n\nprompt\n";
        let skill = parse_skill_content(
            content,
            Path::new("/home/user/.claude/skills/other-name/SKILL.md"),
        )
        .unwrap();
        assert_eq!(skill.name, "custom-name");
    }

    #[test]
    fn test_parse_skill_no_frontmatter() {
        let content = "Just text, no frontmatter";
        let err = parse_skill_content(content, Path::new("test.md")).unwrap_err();
        assert!(matches!(err, SkillError::Parse(_)));
    }

    #[test]
    fn test_load_from_dir() {
        let dir = std::env::temp_dir().join("ccx_test_skills");
        let _ = fs::create_dir_all(&dir);
        // Flat .md files derive name from filename
        fs::write(
            dir.join("skill1.md"),
            "---\ndescription: first\n---\n\nPrompt 1\n",
        )
        .unwrap();
        fs::write(
            dir.join("skill2.md"),
            "---\nname: s2\ndescription: second\n---\n\nPrompt 2\n",
        )
        .unwrap();

        let skills = load_skills_from_dir(&dir).unwrap();
        assert_eq!(skills.len(), 2);
        // skill1.md has no name: field, so name derived from filename
        assert!(skills.iter().any(|s| s.name == "skill1"));
        // skill2.md has explicit name
        assert!(skills.iter().any(|s| s.name == "s2"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_derive_skill_name() {
        assert_eq!(
            derive_skill_name(Path::new("/a/plugins/specweave/skills/do/SKILL.md")),
            Some("sw:do".to_string())
        );
        assert_eq!(
            derive_skill_name(Path::new("/a/plugins/specweave/skills/increment/SKILL.md")),
            Some("sw:increment".to_string())
        );
        assert_eq!(
            derive_skill_name(Path::new("/a/.claude/skills/nanobanana/SKILL.md")),
            Some("nanobanana".to_string())
        );
        assert_eq!(
            derive_skill_name(Path::new("/a/plugins/myplug/skills/foo/SKILL.md")),
            Some("myp:foo".to_string())
        );
        assert_eq!(
            derive_skill_name(Path::new("/a/skills/bar.md")),
            Some("bar".to_string())
        );
    }
}

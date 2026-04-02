use std::fs;
use std::path::{Path, PathBuf};

use crate::types::{MemoryEntry, MemoryType};

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

/// File-based memory store.
pub struct MemoryStore {
    dir: PathBuf,
}

impl MemoryStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Ensure the memory directory exists.
    pub fn init(&self) -> Result<(), MemoryError> {
        fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    /// Save a memory entry as a markdown file with frontmatter.
    pub fn save(&self, entry: &MemoryEntry) -> Result<PathBuf, MemoryError> {
        self.init()?;
        let path = self.dir.join(&entry.filename);
        let content = format!(
            "---\nname: {}\ndescription: {}\ntype: {}\n---\n\n{}",
            entry.name, entry.description, entry.memory_type, entry.content
        );
        fs::write(&path, content)?;
        self.update_index()?;
        Ok(path)
    }

    /// List all memory entries.
    pub fn list(&self) -> Result<Vec<MemoryEntry>, MemoryError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md")
                && path.file_name().is_some_and(|n| n != "MEMORY.md")
                && let Ok(mem) = self.parse_file(&path)
            {
                entries.push(mem);
            }
        }
        Ok(entries)
    }

    /// Delete a memory entry by filename.
    pub fn delete(&self, filename: &str) -> Result<(), MemoryError> {
        let path = self.dir.join(filename);
        if path.exists() {
            fs::remove_file(path)?;
            self.update_index()?;
        }
        Ok(())
    }

    /// Load MEMORY.md index content.
    pub fn load_index(&self) -> Result<String, MemoryError> {
        let index_path = self.dir.join("MEMORY.md");
        if index_path.exists() {
            Ok(fs::read_to_string(index_path)?)
        } else {
            Ok(String::new())
        }
    }

    /// Regenerate MEMORY.md index from existing files.
    fn update_index(&self) -> Result<(), MemoryError> {
        let entries = self.list()?;
        let mut lines = vec!["# Memory Index".to_string(), String::new()];
        for entry in &entries {
            lines.push(format!(
                "- [{}]({}) -- {}",
                entry.name, entry.filename, entry.description
            ));
        }
        lines.push(String::new());
        let index_path = self.dir.join("MEMORY.md");
        fs::write(index_path, lines.join("\n"))?;
        Ok(())
    }

    /// Parse a memory markdown file with frontmatter.
    fn parse_file(&self, path: &Path) -> Result<MemoryEntry, MemoryError> {
        let raw = fs::read_to_string(path)?;
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Parse YAML frontmatter between --- delimiters.
        if let Some(rest) = raw.strip_prefix("---\n")
            && let Some(end) = rest.find("\n---")
        {
            let frontmatter = &rest[..end];
            let content = rest[end + 4..].trim().to_string();

            let name = extract_field(frontmatter, "name");
            let description = extract_field(frontmatter, "description");
            let type_str = extract_field(frontmatter, "type");

            let memory_type = match type_str.as_str() {
                "user" => MemoryType::User,
                "feedback" => MemoryType::Feedback,
                "project" => MemoryType::Project,
                "reference" => MemoryType::Reference,
                _ => {
                    return Err(MemoryError::Parse(format!(
                        "unknown memory type: {type_str}"
                    )));
                }
            };

            return Ok(MemoryEntry {
                name,
                description,
                memory_type,
                content,
                filename,
            });
        }

        Err(MemoryError::Parse("missing frontmatter".into()))
    }
}

/// Extract a simple "key: value" field from YAML-like text.
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
    fn test_save_and_list() {
        let dir = std::env::temp_dir().join("ccx_test_memory_save");
        let _ = fs::remove_dir_all(&dir);
        let store = MemoryStore::new(dir.clone());
        let entry = MemoryEntry {
            name: "test-memory".into(),
            description: "A test memory".into(),
            memory_type: MemoryType::Feedback,
            content: "Don't do X.\n\n**Why:** It broke before.".into(),
            filename: "feedback_test.md".into(),
        };
        store.save(&entry).unwrap();

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test-memory");
        assert_eq!(entries[0].memory_type, MemoryType::Feedback);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_index_generated() {
        let dir = std::env::temp_dir().join("ccx_test_memory_idx");
        let _ = fs::remove_dir_all(&dir);
        let store = MemoryStore::new(dir.clone());
        let entry = MemoryEntry {
            name: "idx-test".into(),
            description: "Index test".into(),
            memory_type: MemoryType::User,
            content: "content".into(),
            filename: "user_idx.md".into(),
        };
        store.save(&entry).unwrap();

        let index = store.load_index().unwrap();
        assert!(index.contains("idx-test"));
        assert!(index.contains("user_idx.md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_delete() {
        let dir = std::env::temp_dir().join("ccx_test_memory_del");
        let _ = fs::remove_dir_all(&dir);
        let store = MemoryStore::new(dir.clone());
        let entry = MemoryEntry {
            name: "to-delete".into(),
            description: "Will be deleted".into(),
            memory_type: MemoryType::Project,
            content: "tmp".into(),
            filename: "project_del.md".into(),
        };
        store.save(&entry).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);

        store.delete("project_del.md").unwrap();
        assert_eq!(store.list().unwrap().len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }
}

/// Session persistence: save/load conversation sessions to ~/.claude/sessions/.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A persisted session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub model: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<ccx_api::InputMessage>,
    /// First user message preview (for listing).
    pub preview: String,
    pub total_turns: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

/// Directory for session files.
fn sessions_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join(".claude").join("sessions"))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Create a new session ID.
pub fn new_session_id() -> String {
    let ts = now_epoch();
    let rand: u32 = (ts as u32).wrapping_mul(2654435761); // simple hash
    format!("{ts:x}-{rand:08x}")
}

/// Save a session to disk.
pub fn save_session(session: &Session) -> Result<(), String> {
    let dir = sessions_dir().ok_or("cannot determine home directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", session.id));
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Load a session by ID.
pub fn load_session(id: &str) -> Result<Session, String> {
    let dir = sessions_dir().ok_or("cannot determine home directory")?;
    let path = dir.join(format!("{id}.json"));
    if !path.exists() {
        return Err(format!("session not found: {id}"));
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

/// List all sessions, sorted by updated_at descending.
pub fn list_sessions() -> Vec<Session> {
    let dir = match sessions_dir() {
        Some(d) if d.exists() => d,
        _ => return Vec::new(),
    };
    let mut sessions: Vec<Session> = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let content = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

/// Find the most recent session for a given working directory.
pub fn find_latest_for_cwd(cwd: &str) -> Option<Session> {
    list_sessions().into_iter().find(|s| s.cwd == cwd)
}

/// Extract a preview from user text (first line, truncated).
pub fn make_preview(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text);
    if line.len() > 80 {
        format!("{}...", &line[..77])
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_id() {
        let id = new_session_id();
        assert!(!id.is_empty());
        assert!(id.contains('-'));
    }

    #[test]
    fn test_make_preview_short() {
        assert_eq!(make_preview("hello world"), "hello world");
    }

    #[test]
    fn test_make_preview_long() {
        let long = "a".repeat(100);
        let preview = make_preview(&long);
        assert!(preview.len() <= 83);
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_make_preview_multiline() {
        assert_eq!(make_preview("first\nsecond\nthird"), "first");
    }
}

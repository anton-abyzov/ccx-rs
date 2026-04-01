/// Session persistence matching Claude Code's layout.
///
/// Storage:
///   ~/.claude/projects/{project-hash}/sessions/{session-id}.jsonl
///   ~/.claude/projects/{project-hash}/sessions/{session-id}.meta.json
///
/// Project hash: hex of hashed absolute working directory path.
/// JSONL: one `InputMessage` per line (full API message format).
/// Meta: lightweight metadata for listing without loading messages.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Session metadata stored alongside the JSONL transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub cwd: String,
    pub model: String,
    pub created: u64,
    #[serde(rename = "lastActive")]
    pub last_active: u64,
    pub preview: String,
    pub name: Option<String>,
    pub turns: usize,
    #[serde(rename = "totalTokens")]
    pub total_tokens: u64,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Deterministic hash of a project path for directory naming.
fn project_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Sessions directory for a given project working directory.
fn project_sessions_dir(cwd: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(
        home.join(".claude")
            .join("projects")
            .join(project_hash(cwd))
            .join("sessions"),
    )
}

// ---------------------------------------------------------------------------
// ID & time helpers
// ---------------------------------------------------------------------------

/// Current time as epoch seconds.
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Generate a UUID v4-formatted session ID from system randomness.
pub fn new_session_id() -> String {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut buf);
    } else {
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        buf[..8].copy_from_slice(&ns.to_le_bytes()[..8]);
        buf[8..].copy_from_slice(&ns.wrapping_mul(6364136223846793005).to_le_bytes()[..8]);
    }
    buf[6] = (buf[6] & 0x0f) | 0x40;
    buf[8] = (buf[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        buf[0], buf[1], buf[2], buf[3],
        buf[4], buf[5], buf[6], buf[7],
        buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
    )
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// Write (or overwrite) the JSONL transcript for a session.
pub fn save_session_messages(
    cwd: &str,
    session_id: &str,
    messages: &[ccx_api::InputMessage],
) -> Result<(), String> {
    let dir = project_sessions_dir(cwd).ok_or("cannot determine home directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join(format!("{session_id}.jsonl"));
    let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
    for msg in messages {
        let line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Write (or update) session metadata.
pub fn save_session_meta(meta: &SessionMeta) -> Result<(), String> {
    let dir = project_sessions_dir(&meta.cwd).ok_or("cannot determine home directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join(format!("{}.meta.json", meta.id));
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

/// Load session messages from JSONL, stripping thinking blocks.
pub fn load_session_messages(
    cwd: &str,
    session_id: &str,
) -> Result<Vec<ccx_api::InputMessage>, String> {
    let dir = project_sessions_dir(cwd).ok_or("cannot determine home directory")?;
    let path = dir.join(format!("{session_id}.jsonl"));
    if !path.exists() {
        return Err(format!("session not found: {session_id}"));
    }

    let file = fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = io::BufReader::new(file);

    let mut messages = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let mut msg: ccx_api::InputMessage =
            serde_json::from_str(&line).map_err(|e| format!("invalid session entry: {e}"))?;

        // Strip thinking blocks (signatures are session-bound, waste tokens on resume).
        if let ccx_api::MessageContent::Blocks(blocks) = &mut msg.content {
            blocks.retain(|b| !matches!(b, ccx_api::ContentBlock::Thinking { .. }));
            if blocks.is_empty() && msg.role == ccx_api::Role::Assistant {
                continue;
            }
        }

        messages.push(msg);
    }

    Ok(messages)
}

/// Find session metadata by ID within a project.
pub fn find_session_meta(cwd: &str, session_id: &str) -> Option<SessionMeta> {
    let dir = project_sessions_dir(cwd)?;
    let path = dir.join(format!("{session_id}.meta.json"));
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

/// List sessions for a specific project, sorted by lastActive descending.
pub fn list_sessions_for_project(cwd: &str) -> Vec<SessionMeta> {
    let dir = match project_sessions_dir(cwd) {
        Some(d) if d.exists() => d,
        _ => return Vec::new(),
    };

    let mut sessions: Vec<SessionMeta> = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.to_string_lossy().ends_with(".meta.json") {
                return None;
            }
            let content = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect();

    sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    sessions
}

/// Find the most recent session for a given working directory.
pub fn find_latest_for_cwd(cwd: &str) -> Option<SessionMeta> {
    list_sessions_for_project(cwd).into_iter().next()
}

// ---------------------------------------------------------------------------
// Cleanup
// ---------------------------------------------------------------------------

/// Delete oldest sessions exceeding `max_sessions` for a project.
pub fn cleanup_sessions(cwd: &str, max_sessions: usize) {
    let sessions = list_sessions_for_project(cwd);
    if sessions.len() <= max_sessions {
        return;
    }
    let dir = match project_sessions_dir(cwd) {
        Some(d) => d,
        None => return,
    };
    for old in &sessions[max_sessions..] {
        let _ = fs::remove_file(dir.join(format!("{}.jsonl", old.id)));
        let _ = fs::remove_file(dir.join(format!("{}.meta.json", old.id)));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a preview from user text (first line, truncated to 80 chars).
pub fn make_preview(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text);
    if line.len() > 80 {
        format!("{}...", &line[..77])
    } else {
        line.to_string()
    }
}

/// Format epoch seconds as `YYYY-MM-DD HH:MM`.
pub fn format_epoch(epoch_secs: u64) -> String {
    let hours = (epoch_secs % 86400) / 3600;
    let minutes = (epoch_secs % 3600) / 60;

    let mut remaining_days = (epoch_secs / 86400) as i64;
    let mut year: i64 = 1970;
    loop {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days_in_year: i64 = if leap { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_lengths: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month: i64 = 1;
    for &ml in &month_lengths {
        if remaining_days < ml {
            break;
        }
        remaining_days -= ml;
        month += 1;
    }
    let day = remaining_days + 1;

    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_id_uuid_v4_format() {
        let id = new_session_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn test_project_hash_deterministic() {
        let h1 = project_hash("/Users/test/project");
        let h2 = project_hash("/Users/test/project");
        assert_eq!(h1, h2);
        assert_ne!(h1, project_hash("/Users/test/other"));
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

    #[test]
    fn test_format_epoch_zero() {
        assert_eq!(format_epoch(0), "1970-01-01 00:00");
    }

    #[test]
    fn test_format_epoch_one_day() {
        assert_eq!(format_epoch(86400), "1970-01-02 00:00");
    }

    #[test]
    fn test_format_epoch_current_era() {
        let s = format_epoch(now_epoch());
        assert!(s.starts_with("202"), "expected year 202x, got: {s}");
    }
}

use std::path::{Path, PathBuf};

use ccx_core::{ToolContext, ToolError};

/// Resolve the base directory for ccx meta-tool storage.
/// Priority: ctx.env_vars["CCX_HOME"] > $CCX_HOME > ~/.claude
pub fn resolve_base_dir(ctx: &ToolContext) -> Result<PathBuf, ToolError> {
    if let Some(dir) = ctx.env_vars.get("CCX_HOME") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    if let Ok(dir) = std::env::var("CCX_HOME") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME")
        .map_err(|_| ToolError::Execution("HOME environment variable not set".into()))?;
    Ok(PathBuf::from(home).join(".claude"))
}

/// Get the current team name from CCX_TEAM env var or .current file.
pub fn current_team(base: &Path) -> Result<String, ToolError> {
    if let Ok(team) = std::env::var("CCX_TEAM") {
        if !team.is_empty() {
            return Ok(team);
        }
    }
    let current_file = base.join("teams").join(".current");
    let name = std::fs::read_to_string(&current_file)
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            ToolError::Execution(
                "No active team. Create one with TeamCreate or set CCX_TEAM.".into(),
            )
        })?;
    if name.is_empty() {
        return Err(ToolError::Execution(
            "No active team. Create one with TeamCreate or set CCX_TEAM.".into(),
        ));
    }
    Ok(name)
}

/// Set the current team.
pub fn set_current_team(base: &Path, team_name: &str) -> Result<(), ToolError> {
    let teams_dir = base.join("teams");
    std::fs::create_dir_all(&teams_dir).map_err(ToolError::Io)?;
    std::fs::write(teams_dir.join(".current"), team_name).map_err(ToolError::Io)
}

/// Team config directory.
pub fn team_dir(base: &Path, name: &str) -> PathBuf {
    base.join("teams").join(name)
}

/// Task storage directory for a team.
pub fn tasks_dir(base: &Path, name: &str) -> PathBuf {
    base.join("tasks").join(name)
}

/// Message storage directory for a team.
pub fn messages_dir(base: &Path, name: &str) -> PathBuf {
    team_dir(base, name).join("messages")
}

/// Find the next available task ID by scanning existing files.
pub fn next_task_id(tasks_path: &Path) -> Result<u32, ToolError> {
    if !tasks_path.exists() {
        return Ok(1);
    }
    let mut max_id: u32 = 0;
    for entry in std::fs::read_dir(tasks_path).map_err(ToolError::Io)? {
        let entry = entry.map_err(ToolError::Io)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(id_str) = name_str.strip_suffix(".json") {
            if let Ok(id) = id_str.parse::<u32>() {
                max_id = max_id.max(id);
            }
        }
    }
    Ok(max_id + 1)
}

#[cfg(test)]
pub fn test_ctx(suffix: &str) -> (ToolContext, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ccx_meta_test_{suffix}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ctx = ToolContext::new(PathBuf::from("/tmp"));
    ctx.set_env("CCX_HOME", dir.to_str().unwrap());
    (ctx, dir)
}

#[cfg(test)]
pub fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_base_dir_from_ctx() {
        let (ctx, base) = test_ctx("resolve");
        let resolved = resolve_base_dir(&ctx).unwrap();
        assert_eq!(resolved, base);
        cleanup(&base);
    }

    #[test]
    fn test_current_team_none() {
        let (ctx, base) = test_ctx("no_team");
        let err = current_team(&resolve_base_dir(&ctx).unwrap());
        assert!(err.is_err());
        cleanup(&base);
    }

    #[test]
    fn test_set_and_get_current_team() {
        let (ctx, base) = test_ctx("set_get");
        let b = resolve_base_dir(&ctx).unwrap();
        set_current_team(&b, "my-team").unwrap();
        let name = current_team(&b).unwrap();
        assert_eq!(name, "my-team");
        cleanup(&base);
    }

    #[test]
    fn test_next_task_id_empty() {
        let (_, base) = test_ctx("next_id_empty");
        let id = next_task_id(&base.join("tasks/t")).unwrap();
        assert_eq!(id, 1);
        cleanup(&base);
    }

    #[test]
    fn test_next_task_id_with_files() {
        let (_, base) = test_ctx("next_id_files");
        let dir = base.join("tasks/t");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.json"), "{}").unwrap();
        std::fs::write(dir.join("3.json"), "{}").unwrap();
        let id = next_task_id(&dir).unwrap();
        assert_eq!(id, 4);
        cleanup(&base);
    }
}

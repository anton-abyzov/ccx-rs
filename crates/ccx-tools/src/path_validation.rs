use std::path::Path;

use ccx_core::ToolError;

/// Validate that a path is within the working directory.
/// Skipped when `bypass` is true (bypass permissions mode).
pub fn validate_path(path: &Path, working_dir: &Path, bypass: bool) -> Result<(), ToolError> {
    if bypass {
        return Ok(());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| ToolError::InvalidInput(format!("path not found: {}", path.display())))?;
    let wd_canonical = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.to_path_buf());
    if !canonical.starts_with(&wd_canonical) {
        return Err(ToolError::PermissionDenied(format!(
            "path {} is outside working directory",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_bypass() {
        assert!(validate_path(Path::new("/etc/passwd"), Path::new("/tmp"), true).is_ok());
    }

    #[test]
    fn test_validate_path_inside_working_dir() {
        let dir = std::env::temp_dir().join("ccx_test_pathval");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("test.txt");
        std::fs::write(&file, "hi").unwrap();

        assert!(validate_path(&file, &dir, false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_path_outside_working_dir() {
        let dir = std::env::temp_dir().join("ccx_test_pathval_outside");
        let _ = std::fs::create_dir_all(&dir);
        let result = validate_path(Path::new("/etc/hosts"), &dir, false);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

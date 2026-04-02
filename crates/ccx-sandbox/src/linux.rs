use std::path::Path;

use crate::{Sandbox, SandboxConfig, SandboxError};

/// Linux sandbox using Landlock (stub implementation).
pub struct LandlockSandbox;

impl Sandbox for LandlockSandbox {
    fn wrap_command(
        &self,
        command: &str,
        _working_dir: &Path,
        config: &SandboxConfig,
    ) -> Result<Vec<String>, SandboxError> {
        if !config.enabled {
            return Ok(vec!["bash".into(), "-c".into(), command.into()]);
        }

        // Landlock is not yet implemented — warn the user and run unsandboxed.
        eprintln!(
            "\x1b[33m\u{26a0} Sandbox not available on Linux (Landlock not implemented). \
             Commands run unsandboxed.\x1b[0m"
        );
        Ok(vec!["bash".into(), "-c".into(), command.into()])
    }

    fn name(&self) -> &str {
        "landlock-stub"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "linux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_landlock_stub() {
        let sb = LandlockSandbox;
        let config = SandboxConfig::default();
        let cmd = sb
            .wrap_command("echo hello", Path::new("/tmp"), &config)
            .unwrap();
        assert_eq!(cmd, vec!["bash", "-c", "echo hello"]);
    }
}

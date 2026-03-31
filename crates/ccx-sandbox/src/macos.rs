use std::path::Path;

use crate::{Sandbox, SandboxConfig, SandboxError};

/// macOS sandbox using sandbox-exec (Seatbelt).
pub struct SeatbeltSandbox;

impl SeatbeltSandbox {
    /// Generate a Seatbelt profile from config.
    fn generate_profile(config: &SandboxConfig, working_dir: &Path) -> String {
        let mut rules = vec!["(version 1)".to_string(), "(deny default)".to_string()];

        // Always allow basic process operations.
        rules.push("(allow process-exec)".into());
        rules.push("(allow process-fork)".into());
        rules.push("(allow sysctl-read)".into());

        // Allow reading specified paths.
        for path in &config.allow_read {
            rules.push(format!(
                "(allow file-read* (subpath \"{path}\"))"
            ));
        }

        // Allow writing to specified paths.
        for path in &config.allow_write {
            rules.push(format!(
                "(allow file-write* (subpath \"{path}\"))"
            ));
        }

        // Always allow read/write in working directory.
        let wd = working_dir.to_string_lossy();
        rules.push(format!("(allow file-read* (subpath \"{wd}\"))"));
        rules.push(format!("(allow file-write* (subpath \"{wd}\"))"));

        // Network access.
        if config.allow_network {
            rules.push("(allow network*)".into());
        }

        rules.join("\n")
    }
}

impl Sandbox for SeatbeltSandbox {
    fn wrap_command(
        &self,
        command: &str,
        working_dir: &Path,
        config: &SandboxConfig,
    ) -> Result<Vec<String>, SandboxError> {
        if !config.enabled {
            return Ok(vec!["bash".into(), "-c".into(), command.into()]);
        }

        let profile = Self::generate_profile(config, working_dir);
        Ok(vec![
            "sandbox-exec".into(),
            "-p".into(),
            profile,
            "bash".into(),
            "-c".into(),
            command.into(),
        ])
    }

    fn name(&self) -> &str {
        "seatbelt"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_profile() {
        let config = SandboxConfig {
            enabled: true,
            allow_read: vec!["/usr".into()],
            allow_write: vec!["/tmp".into()],
            allow_network: false,
        };
        let profile = SeatbeltSandbox::generate_profile(&config, Path::new("/home/user"));
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("/usr"));
        assert!(profile.contains("/tmp"));
        assert!(profile.contains("/home/user"));
        assert!(!profile.contains("network"));
    }

    #[test]
    fn test_wrap_command() {
        let sb = SeatbeltSandbox;
        let config = SandboxConfig::default();
        let cmd = sb
            .wrap_command("echo hello", Path::new("/tmp"), &config)
            .unwrap();
        assert!(cmd.contains(&"sandbox-exec".to_string()) || !sb.is_available());
    }

    #[test]
    fn test_disabled_sandbox() {
        let sb = SeatbeltSandbox;
        let config = SandboxConfig {
            enabled: false,
            ..Default::default()
        };
        let cmd = sb
            .wrap_command("echo hello", Path::new("/tmp"), &config)
            .unwrap();
        assert_eq!(cmd, vec!["bash", "-c", "echo hello"]);
    }
}

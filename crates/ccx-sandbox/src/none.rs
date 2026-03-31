use std::path::Path;

use crate::{Sandbox, SandboxConfig, SandboxError};

/// No-op sandbox fallback for unsupported platforms.
pub struct NoopSandbox;

impl Sandbox for NoopSandbox {
    fn wrap_command(
        &self,
        command: &str,
        _working_dir: &Path,
        _config: &SandboxConfig,
    ) -> Result<Vec<String>, SandboxError> {
        Ok(vec!["bash".into(), "-c".into(), command.into()])
    }

    fn name(&self) -> &str {
        "none"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_sandbox() {
        let sb = NoopSandbox;
        assert!(sb.is_available());
        assert_eq!(sb.name(), "none");
        let cmd = sb
            .wrap_command("ls", Path::new("/tmp"), &SandboxConfig::default())
            .unwrap();
        assert_eq!(cmd, vec!["bash", "-c", "ls"]);
    }
}

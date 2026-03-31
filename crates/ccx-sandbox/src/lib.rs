pub mod macos;
pub mod linux;
pub mod none;

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox setup failed: {0}")]
    Setup(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sandbox not supported on this platform")]
    NotSupported,
}

/// Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub allow_read: Vec<String>,
    pub allow_write: Vec<String>,
    pub allow_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_read: vec!["/".into()],
            allow_write: Vec::new(),
            allow_network: false,
        }
    }
}

/// Trait for platform-specific sandbox implementations.
pub trait Sandbox: Send + Sync {
    /// Wrap a command to run inside the sandbox.
    fn wrap_command(
        &self,
        command: &str,
        working_dir: &Path,
        config: &SandboxConfig,
    ) -> Result<Vec<String>, SandboxError>;

    /// Name of this sandbox implementation.
    fn name(&self) -> &str;

    /// Whether this sandbox is available on the current platform.
    fn is_available(&self) -> bool;
}

/// Create the appropriate sandbox for the current platform.
pub fn create_sandbox() -> Box<dyn Sandbox> {
    #[cfg(target_os = "macos")]
    {
        let sb = macos::SeatbeltSandbox;
        if sb.is_available() {
            return Box::new(sb);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let sb = linux::LandlockSandbox;
        if sb.is_available() {
            return Box::new(sb);
        }
    }

    Box::new(none::NoopSandbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SandboxConfig::default();
        assert!(config.enabled);
        assert!(!config.allow_network);
    }

    #[test]
    fn test_create_sandbox() {
        let sb = create_sandbox();
        assert!(!sb.name().is_empty());
    }
}

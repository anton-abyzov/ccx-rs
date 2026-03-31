use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::types::{JsonRpcRequest, JsonRpcResponse};

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("process not running")]
    ProcessNotRunning,
    #[error("no stdout available")]
    NoStdout,
    #[error("no stdin available")]
    NoStdin,
}

/// Stdio transport for MCP: spawns a subprocess and communicates via JSON-RPC over stdin/stdout.
pub struct StdioTransport {
    child: Child,
}

impl StdioTransport {
    /// Spawn a new MCP server process.
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, TransportError> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child })
    }

    /// Send a JSON-RPC request and read the response.
    pub async fn request(
        &mut self,
        req: &JsonRpcRequest,
    ) -> Result<JsonRpcResponse, TransportError> {
        let stdin = self.child.stdin.as_mut().ok_or(TransportError::NoStdin)?;
        let stdout = self.child.stdout.as_mut().ok_or(TransportError::NoStdout)?;

        let mut line = serde_json::to_string(req)?;
        line.push('\n');
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;

        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: JsonRpcResponse = serde_json::from_str(&response_line)?;
        Ok(response)
    }

    /// Kill the subprocess.
    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        self.child.kill().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_nonexistent() {
        let result = StdioTransport::spawn("nonexistent_binary_xyz", &[]);
        assert!(result.is_err());
    }
}

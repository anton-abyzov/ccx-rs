use crate::transport::{StdioTransport, TransportError};
use crate::types::{JsonRpcRequest, McpTool};

/// MCP client that communicates with a server over stdio.
pub struct McpClient {
    transport: StdioTransport,
    next_id: u64,
    server_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("server error: [{code}] {message}")]
    Server { code: i64, message: String },
    #[error("unexpected response format")]
    BadResponse,
}

impl McpClient {
    /// Connect to an MCP server by spawning it.
    pub fn connect(
        server_name: impl Into<String>,
        command: &str,
        args: &[&str],
    ) -> Result<Self, McpError> {
        let transport = StdioTransport::spawn(command, args)?;
        Ok(Self {
            transport,
            next_id: 1,
            server_name: server_name.into(),
        })
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Initialize the MCP connection.
    pub async fn initialize(&mut self) -> Result<serde_json::Value, McpError> {
        let req = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "ccx",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );
        self.call(req).await
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        let req = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let result = self.call(req).await?;

        let tools: Vec<McpTool> = result
            .get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .unwrap_or_default();
        Ok(tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let req = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            })),
        );
        self.call(req).await
    }

    async fn call(&mut self, req: JsonRpcRequest) -> Result<serde_json::Value, McpError> {
        let resp = self.transport.request(&req).await?;
        if let Some(err) = resp.error {
            return Err(McpError::Server {
                code: err.code,
                message: err.message,
            });
        }
        resp.result.ok_or(McpError::BadResponse)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Shut down the MCP server.
    pub async fn shutdown(&mut self) -> Result<(), McpError> {
        self.transport.shutdown().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_nonexistent() {
        let result = McpClient::connect("test", "nonexistent_mcp_server_xyz", &[]);
        assert!(result.is_err());
    }
}

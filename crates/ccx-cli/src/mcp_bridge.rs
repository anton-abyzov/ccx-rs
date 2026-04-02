use std::sync::Arc;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use ccx_mcp::{McpClient, McpTool};
use tokio::sync::Mutex;

/// Bridge that wraps an MCP tool as a ccx Tool.
pub struct McpToolBridge {
    client: Arc<Mutex<McpClient>>,
    tool: McpTool,
}

impl McpToolBridge {
    pub fn new(client: Arc<Mutex<McpClient>>, tool: McpTool) -> Self {
        Self { client, tool }
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn name(&self) -> &str {
        &self.tool.name
    }

    fn description(&self) -> &str {
        &self.tool.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.tool.input_schema.clone()
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let mut client = self.client.lock().await;
        match client.call_tool(&self.tool.name, input).await {
            Ok(result) => {
                // MCP returns content as array of {type, text} blocks.
                let content = result
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    item.get("text").and_then(|t| t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| serde_json::to_string_pretty(&result).unwrap_or_default());
                let is_error = result
                    .get("isError")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);
                Ok(ToolResult { content, is_error })
            }
            Err(e) => Err(ToolError::Execution(e.to_string())),
        }
    }
}

/// MCP server configuration entry.
#[derive(Debug, serde::Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Top-level MCP config file format (.mcp.json).
#[derive(Debug, serde::Deserialize)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: std::collections::HashMap<String, McpServerConfig>,
}

/// Load MCP config from .mcp.json in the given directory.
pub fn load_mcp_config(dir: &std::path::Path) -> Option<McpConfig> {
    let path = dir.join(".mcp.json");
    if path.exists() {
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

/// Spawn MCP servers, discover tools, and register them into the registry.
/// Returns client handles to keep servers alive for the session.
pub async fn register_mcp_tools(
    config: &McpConfig,
    registry: &mut ccx_core::ToolRegistry,
) -> Vec<Arc<Mutex<McpClient>>> {
    let mut clients = Vec::new();

    for (name, server) in &config.mcp_servers {
        let args_refs: Vec<&str> = server.args.iter().map(|s| s.as_str()).collect();
        match McpClient::connect(name.clone(), &server.command, &args_refs) {
            Ok(mut client) => {
                if let Err(e) = client.initialize().await {
                    eprintln!("MCP [{name}]: init failed: {e}");
                    continue;
                }
                match client.list_tools().await {
                    Ok(tools) => {
                        let client = Arc::new(Mutex::new(client));
                        let tool_count = tools.len();
                        for tool in tools {
                            let bridge = McpToolBridge::new(Arc::clone(&client), tool);
                            registry.register(Box::new(bridge));
                        }
                        eprintln!("MCP [{name}]: {tool_count} tools registered");
                        clients.push(client);
                    }
                    Err(e) => {
                        eprintln!("MCP [{name}]: list_tools failed: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("MCP [{name}]: connect failed: {e}");
            }
        }
    }

    clients
}

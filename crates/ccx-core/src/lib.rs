pub mod agent;
pub mod agent_loop;
pub mod context;
pub mod tool;

pub use agent::{AgentDef, AgentError, AgentManager, AgentMessage, AgentResult};
pub use agent_loop::{AgentCallback, AgentLoop, AgentLoopError, NoopCallback};
pub use context::ToolContext;
pub use tool::{Tool, ToolError, ToolRegistry, ToolResult};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.1.0");
    }
}

pub mod agent_tool;
pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob_tool;
pub mod grep;
pub mod notebook_edit;
pub mod team_create;
pub mod todo_write;
pub mod web_fetch;
pub mod web_search;

pub use agent_tool::AgentTool;
pub use bash::BashTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use glob_tool::GlobTool;
pub use grep::GrepTool;
pub use notebook_edit::NotebookEditTool;
pub use team_create::TeamCreateTool;
pub use todo_write::TodoWriteTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Register all built-in tools into a registry.
pub fn register_all(registry: &mut ccx_core::ToolRegistry) {
    registry.register(Box::new(BashTool));
    registry.register(Box::new(FileReadTool));
    registry.register(Box::new(FileWriteTool));
    registry.register(Box::new(FileEditTool));
    registry.register(Box::new(GlobTool));
    registry.register(Box::new(GrepTool));
    registry.register(Box::new(WebFetchTool));
    registry.register(Box::new(WebSearchTool));
    registry.register(Box::new(AgentTool));
    registry.register(Box::new(TodoWriteTool));
    registry.register(Box::new(NotebookEditTool));
    registry.register(Box::new(TeamCreateTool));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_all() {
        let mut registry = ccx_core::ToolRegistry::new();
        register_all(&mut registry);
        assert!(registry.get("Bash").is_some());
        assert!(registry.get("Read").is_some());
        assert!(registry.get("Write").is_some());
        assert!(registry.get("Edit").is_some());
        assert!(registry.get("Glob").is_some());
        assert!(registry.get("Grep").is_some());
        assert!(registry.get("WebFetch").is_some());
        assert!(registry.get("WebSearch").is_some());
        assert!(registry.get("Agent").is_some());
        assert!(registry.get("TodoWrite").is_some());
        assert!(registry.get("NotebookEdit").is_some());
        assert!(registry.get("TeamCreate").is_some());
    }

    #[test]
    fn test_register_all_count() {
        let mut registry = ccx_core::ToolRegistry::new();
        register_all(&mut registry);
        assert_eq!(registry.names().len(), 12);
    }
}

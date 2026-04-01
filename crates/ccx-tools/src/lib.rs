pub mod agent_tool;
pub mod bash;
pub mod enter_plan_mode;
pub mod exit_plan_mode;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob_tool;
pub mod grep;
pub mod meta_helpers;
pub mod notebook_edit;
pub mod send_message;
pub mod task_create;
pub mod task_list;
pub mod task_update;
pub mod team_create;
pub mod team_delete;
pub mod todo_write;
pub mod web_fetch;
pub mod web_search;

pub use agent_tool::AgentTool;
pub use bash::BashTool;
pub use enter_plan_mode::EnterPlanModeTool;
pub use exit_plan_mode::ExitPlanModeTool;
pub use file_edit::FileEditTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use glob_tool::GlobTool;
pub use grep::GrepTool;
pub use notebook_edit::NotebookEditTool;
pub use send_message::SendMessageTool;
pub use task_create::TaskCreateTool;
pub use task_list::TaskListTool;
pub use task_update::TaskUpdateTool;
pub use team_create::TeamCreateTool;
pub use team_delete::TeamDeleteTool;
pub use todo_write::TodoWriteTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Register all built-in tools into a registry.
pub fn register_all(registry: &mut ccx_core::ToolRegistry) {
    // Core file/shell tools.
    registry.register(Box::new(BashTool));
    registry.register(Box::new(FileReadTool));
    registry.register(Box::new(FileWriteTool));
    registry.register(Box::new(FileEditTool));
    registry.register(Box::new(GlobTool));
    registry.register(Box::new(GrepTool));

    // Web tools.
    registry.register(Box::new(WebFetchTool));
    registry.register(Box::new(WebSearchTool));

    // Agent orchestration.
    registry.register(Box::new(AgentTool));

    // Task management.
    registry.register(Box::new(TodoWriteTool));
    registry.register(Box::new(NotebookEditTool));
    registry.register(Box::new(TaskCreateTool));
    registry.register(Box::new(TaskUpdateTool));
    registry.register(Box::new(TaskListTool));

    // Team coordination.
    registry.register(Box::new(TeamCreateTool));
    registry.register(Box::new(TeamDeleteTool));
    registry.register(Box::new(SendMessageTool));

    // Plan mode.
    registry.register(Box::new(EnterPlanModeTool));
    registry.register(Box::new(ExitPlanModeTool));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_all() {
        let mut registry = ccx_core::ToolRegistry::new();
        register_all(&mut registry);

        // Core tools.
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

        // New meta-tools.
        assert!(registry.get("TeamCreate").is_some());
        assert!(registry.get("TeamDelete").is_some());
        assert!(registry.get("SendMessage").is_some());
        assert!(registry.get("TaskCreate").is_some());
        assert!(registry.get("TaskUpdate").is_some());
        assert!(registry.get("TaskList").is_some());
        assert!(registry.get("EnterPlanMode").is_some());
        assert!(registry.get("ExitPlanMode").is_some());
    }

    #[test]
    fn test_register_all_count() {
        let mut registry = ccx_core::ToolRegistry::new();
        register_all(&mut registry);
        assert_eq!(registry.names().len(), 19);
    }
}

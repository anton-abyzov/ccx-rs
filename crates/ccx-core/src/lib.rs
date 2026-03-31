/// Core agent loop — placeholder for Phase 2 implementation.
///
/// Will contain:
/// - Main query loop: message -> API -> tool_use -> execute -> loop
/// - Tool registry with dynamic MCP tools
/// - Streaming output rendering

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

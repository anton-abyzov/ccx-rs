# ccx-rs

AI coding assistant CLI in Rust. 4.7MB binary, 11 tools, Claude Code-style TUI.

![CCX-RS Terminal UI](assets/screenshot.png)

## Quick Start

```bash
git clone https://github.com/anton-abyzov/ccx-rs.git
cd ccx-rs
cargo build --release
export ANTHROPIC_API_KEY="your-key-here"
./target/release/ccx chat
```

## Features

- Inline TUI with welcome panel, styled chat, tool execution display
- 11 built-in tools (Bash, FileRead, FileWrite, FileEdit, Glob, Grep, WebFetch, WebSearch, Agent, TodoWrite, NotebookEdit)
- Tab completion for slash commands + discovered skills
- Persistent command history
- Streaming responses with markdown rendering
- Tool permission prompts (allow/deny/always)
- Context compression
- MCP client
- Cost tracking

## Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/exit` | Exit the session |
| `/clear` | Clear conversation history |
| `/cost` | Show token usage and cost |
| `/model` | Switch model |
| `/tools` | List available tools |
| `/version` | Show version info |

Discovered skills from `CLAUDE.md` and MCP servers are also available as slash commands.

## Architecture

14-crate workspace:

| Crate | Purpose |
|-------|---------|
| `ccx-cli` | CLI entry point |
| `ccx-core` | Core agent loop |
| `ccx-api` | Anthropic API client with streaming |
| `ccx-auth` | API key management |
| `ccx-tools` | Tool interface + 11 implementations |
| `ccx-permission` | Permission DSL and rules |
| `ccx-compact` | Context compression |
| `ccx-memory` | Memory persistence |
| `ccx-skill` | Skill loading and execution |
| `ccx-prompt` | System prompt builder |
| `ccx-config` | Settings and CLAUDE.md |
| `ccx-mcp` | MCP client |
| `ccx-tui` | Ratatui-based terminal UI |
| `ccx-sandbox` | OS-native sandboxing |

## Building

```bash
cargo build --release    # optimized binary
cargo test               # run test suite
cargo install --path crates/ccx-cli
```

## License

MIT + Apache-2.0

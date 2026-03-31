# ccx-rs

A Rust implementation of an AI coding assistant CLI. 20MB static binary, 5ms startup, OS-native sandboxing. Built on top of OpenAI Codex's Apache-2.0 infrastructure crates.

## Backstory

On March 31, 2026, security researcher Chaofan Shou ([@Fried_rice](https://x.com/Fried_rice/status/2038894956459290963)) discovered that Anthropic's npm package `@anthropic-ai/claude-code` shipped with a 57MB source map file (`cli.js.map`), exposing the full TypeScript source -- 512,000 lines across 1,900+ files -- completely unobfuscated.

What the architecture analysis uncovered was impressive engineering: 43 built-in tools, 4-layer context compression, multi-agent orchestration, MCP protocol, a permission DSL, and undocumented features (BUDDY AI pet, KAIROS daemon mode, Auto-Dream memory consolidation). Full analysis: [verified-skill.com/insights/claude-code](https://verified-skill.com/insights/claude-code).

The CCX project emerged from this discovery. Rust was the performance play -- if you're going to rewrite a 114MB Node.js CLI, do it in the language that gave us ripgrep, bat, fd, and delta. ccx-rs builds on OpenAI Codex's Apache-2.0 crate ecosystem (sandbox, TUI, MCP, exec) and adds the Claude-specific layer on top: API streaming, 4-layer compression, memory persistence, and the full tool system.

Unlike [instructkr/claw-code](https://github.com/instructkr/claw-code) (41.7k stars), which wraps Claude Code in a Python harness, ccx-rs is a ground-up Rust implementation with real tool execution, OS-native sandboxing, and a comprehensive test suite.

- Original tweet: https://x.com/Fried_rice/status/2038894956459290963
- Architecture analysis: https://verified-skill.com/insights/claude-code
- CCX umbrella: https://github.com/anton-abyzov/ccx

## Why Rust?

- **20MB static binary** vs 114MB npm package -- zero runtime dependencies
- **5ms cold start** vs 400ms for Node.js
- **OS-native sandboxing** -- Seatbelt (macOS), Landlock (Linux), Windows Sandbox
- **Codex foundation** -- fork 40+ Apache-2.0 crates from openai/codex (sandbox, TUI, MCP, file-search, git-utils, exec)
- **Proven path** -- ripgrep, bat, fd, delta all rewrote Node/Python tools in Rust successfully

## Architecture

Based on Claude Code's 512K-line TypeScript architecture, built on Codex's Rust infrastructure:

### Reused from Codex (Apache-2.0)
- `sandboxing/` -- OS-native sandboxing (Seatbelt, Landlock, Windows)
- `tui/` -- Ratatui-based terminal UI
- `exec/` -- Safe command execution with timeouts
- `shell-command/` -- Shell command parsing
- `mcp-server/` -- MCP protocol implementation
- `file-search/` -- Codebase navigation
- `git-utils/` -- Git operations
- `apply-patch/` -- Code modification
- `network-proxy/` -- Controlled network access
- `plugin/` -- Plugin system

### Built new (Claude API layer)
- `claude-api/` -- Anthropic Messages API client with streaming
- `claude-auth/` -- API key management
- `claude-core/` -- Core agent loop with tool_use protocol
- `claude-prompt/` -- System prompt construction
- `claude-tools/` -- Claude-specific tool definitions
- `claude-compact/` -- 4-layer context compression
- `claude-memory/` -- Memory persistence system
- `claude-skill/` -- Skill loading and execution
- `claude-permission/` -- Permission DSL and rules

## Tech Stack

| Component | Crate |
|-----------|-------|
| TUI | ratatui + crossterm |
| HTTP/Streaming | reqwest + eventsource-stream |
| JSON | serde + serde_json |
| Schema Validation | jsonschema |
| Async Runtime | tokio |
| CLI Parsing | clap |
| Markdown | termimad |
| Syntax Highlighting | syntect |
| Diff | similar |
| Config | toml + serde |
| Testing | assert_cmd + predicates + insta + wiremock |

## Workspace Structure

```
Cargo.toml                    # Workspace root
crates/
  claude-cli/                 # CLI entry point
  claude-core/                # Core agent loop
  claude-api/                 # Anthropic API client
  claude-auth/                # Auth and key management
  claude-tools/               # Tool interface + implementations
  claude-permission/          # Permission system
  claude-compact/             # Context compression
  claude-memory/              # Memory persistence
  claude-skill/               # Skill system
  claude-prompt/              # System prompt builder
  claude-config/              # Settings and CLAUDE.md
  claude-mcp/                 # MCP client (wraps Codex's rmcp)
  claude-tui/                 # TUI layer (extends Codex's tui)
  claude-sandbox/             # Sandbox (wraps Codex's sandboxing)
  claude-exec/                # Command execution (wraps Codex's exec)
  claude-search/              # File search (wraps Codex's file-search)
  claude-git/                 # Git ops (wraps Codex's git-utils)
```

## Getting Started

```sh
cargo install ccx-rs
```

## Development

```sh
git clone https://github.com/anton-abyzov/ccx-rs.git
cd ccx-rs
cargo build
cargo test
```

## License

MIT (new code) + Apache-2.0 (Codex-derived crates)

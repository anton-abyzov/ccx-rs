# ccx-rs

AI coding assistant CLI in Rust. 4.7MB binary, 19 tools, Claude Code-style TUI. Multi-provider: Claude (API key or Max/Pro subscription), OpenRouter (200+ models including free), Ollama (local).

![CCX-RS Terminal UI](assets/screenshot.png)

## Quick Start

### With Claude (API key)
```bash
git clone https://github.com/anton-abyzov/ccx-rs.git
cd ccx-rs
cargo build --release
export ANTHROPIC_API_KEY="your-key-here"
./target/release/ccx chat
```

### With Claude Max/Pro subscription (no API key needed)
```bash
./target/release/ccx chat          # reads token from macOS Keychain
./target/release/ccx chat /login   # authenticate via browser
```

### With OpenRouter (free models, no subscription)
```bash
export OPENROUTER_API_KEY="your-free-key-from-openrouter.ai"

# Standard model — fast, great for coding
./target/release/ccx chat --provider openrouter --model "nvidia/nemotron-3-super-120b-a12b:free"

# Reasoning model — shows thinking process (dim italic text)
./target/release/ccx chat --provider openrouter --model "deepseek/deepseek-r1"
```

Get a free OpenRouter API key at [openrouter.ai/keys](https://openrouter.ai/keys).

## Features

- Inline TUI with welcome panel, ASCII pet, styled `❯` prompt
- 19 tools: Bash, FileRead, FileWrite, FileEdit, Glob, Grep, WebFetch, WebSearch, Agent, TodoWrite, NotebookEdit, TeamCreate, TeamDelete, SendMessage, TaskCreate, TaskUpdate, TaskList, EnterPlanMode, ExitPlanMode
- Multi-provider: Anthropic (direct + OAuth), OpenRouter (200+ models), Ollama (local)
- Tab completion for slash commands + 50+ discovered skills
- Thinking/reasoning display (DeepSeek R1, Claude extended thinking)
- Parallel tool execution (all tools in a turn run concurrently)
- Prompt caching (saves tokens on multi-turn conversations)
- Persistent command history + session resume
- MCP client (connect external tool servers via `.mcp.json`)
- Memory system (loads `~/.claude/memory/` into context)
- Context auto-compaction at token thresholds
- macOS Seatbelt sandboxing (`--sandbox`)
- Image/PDF reading (base64 encoded)
- OAuth login (`/login` — browser-based authentication)
- Cost tracking with per-turn token counts

## Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands + discovered skills |
| `/exit` | Exit the session |
| `/clear` | Clear screen |
| `/cost` | Show token usage and cost |
| `/model` | Show current model |
| `/tools` | List all 19 tools |
| `/version` | Show version info |
| `/login` | Authenticate via browser (OAuth) |
| `/init` | Create CLAUDE.md in current directory |
| `/compact` | Compress conversation context |
| `/config` | Show current configuration |
| `/status` | Show session stats |
| `/sessions` | List previous sessions |
| `/resume` | Resume a previous session |
| `/continue` | Continue most recent session |
| `/doctor` | Check health (API, tools, MCP) |
| `/simplify` | Invoke simplify skill |
| `/batch` | Invoke batch skill |

Plus 50+ discovered skills via Tab completion (type `/sw:` + Tab).

## Model Examples

### Standard coding (fast, no reasoning)
```bash
# Nvidia Nemotron — free, 262K context, excellent tool calling
./target/release/ccx chat --provider openrouter --model "nvidia/nemotron-3-super-120b-a12b:free"
```

### Deep reasoning (shows thinking process)
```bash
# DeepSeek R1 — reasoning model, thinking displayed in dim italic
./target/release/ccx chat --provider openrouter --model "deepseek/deepseek-r1"
# You'll see: 💭 Let me think step by step... (in dim italic before the answer)
```

### Claude Sonnet (default, needs API key or subscription)
```bash
./target/release/ccx chat --model claude-sonnet-4-6
```

### Claude Opus (most capable)
```bash
./target/release/ccx chat --model claude-opus-4-6
```

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

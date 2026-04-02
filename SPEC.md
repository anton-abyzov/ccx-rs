# ccx-rs -- Implementation Spec

## Strategy: Fork Codex Crates + Build Claude Layer

OpenAI Codex (github.com/openai/codex) is Apache-2.0 with 80+ Rust crates.
We reuse ~40 infrastructure crates and build ~15 CCX-specific crates.

## Phase 1: Fork and Adapt Codex Foundation (Week 1-4)

### P1-01: Extract Codex crates
- Fork openai/codex, extract reusable crates into this workspace
- Crates to keep: sandboxing, tui, exec, shell-command, file-search, git-utils, apply-patch, network-proxy, plugin, pty, absolute-path, string, fuzzy-match, cache, image
- Remove: codex-api, codex-client, codex-core, backend-client, chatgpt, login (OpenAI-specific)
- Update Cargo.toml workspace members

### P1-02: Claude API client crate
```rust
// crates/claude-api/src/lib.rs
pub struct ClaudeClient { /* reqwest::Client, api_key, model */ }

impl ClaudeClient {
    pub async fn stream_message(&self, req: MessageRequest) -> impl Stream<Item = StreamEvent> { ... }
}

pub enum StreamEvent {
    MessageStart { message: Message },
    ContentBlockStart { index: usize, content_block: ContentBlock },
    ContentBlockDelta { index: usize, delta: Delta },
    ContentBlockStop { index: usize },
    MessageDelta { delta: MessageDelta, usage: Usage },
    MessageStop,
}
```
- SSE parsing via reqwest streaming
- Tool_use content block handling
- Prompt caching headers
- Extended thinking support
- Error handling (rate limits, prompt-too-long, auth)

### P1-03: Claude auth crate
- API key from env, config file, or keyring
- No OAuth needed (unlike Codex's ChatGPT auth)

### P1-04: Adapt TUI crate
- Replace Codex's OpenAI-specific UI with CCX branding
- Keep: Ratatui rendering, input handling, diff display
- Add: permission dialog, tool output views, markdown rendering

## Phase 2: Core Agent Loop (Week 4-8)

### P2-01: claude-core crate
- Main query loop: message -> API -> tool_use -> execute -> loop
- Tool registry with dynamic MCP tools
- Streaming output rendering

### P2-02: claude-tools crate
- Tool trait matching Claude's tool_use schema
- Built-in tools: Bash, FileRead, FileWrite, FileEdit, Glob, Grep, WebFetch, Agent
- Wraps Codex's exec crate for command execution
- Wraps Codex's file-search for Glob
- Wraps Codex's apply-patch for FileEdit

### P2-03: claude-prompt crate
- System prompt construction from components
- CLAUDE.md parsing and injection
- Dynamic boundary for prompt caching

## Phase 3: Permission and Config (Week 8-12)

### P3-01: claude-permission crate
- Permission modes: default, plan, bypassPermissions, dontAsk, acceptEdits, auto
- Rule DSL: `allow: ["Bash(git *)"]`, `deny: ["Bash(rm *)"]`
- Settings cascade: CLI > session > project > user > defaults
- Interactive TUI prompts via crossterm

### P3-02: claude-config crate
- Settings from `~/.claude/settings.json`
- CLAUDE.md file discovery (walk up to home)
- YAML frontmatter parsing
- Environment variable handling

### P3-03: claude-memory crate
- Memory types: user, feedback, project, reference
- MEMORY.md index with file-based storage
- Truncation (200 lines, 25KB)

## Phase 4: Context Management (Week 12-16)

### P4-01: claude-compact crate
- MicroCompact: strip tool results between turns
- AutoCompact: summarize at ~187K token threshold
- Token counting via tiktoken-rs
- Post-compact file restoration (max 5 files, 50K tokens)
- Image stripping before compaction

### P4-02: claude-mcp crate
- Wraps Codex's MCP implementation
- Adapt for Claude's tool_use format
- stdio and SSE transport support

### P4-03: claude-skill crate
- Markdown skill files with YAML frontmatter
- Bundled vs user vs project skills
- Inline and fork execution modes

## Phase 5: Agent System (Week 16-20)

### P5-01: Agent spawning
- Tokio tasks for subagents
- Named agent addressing via channels
- Background agent support
- Context isolation per agent

### P5-02: Sandbox integration
- Wrap Codex's sandboxing crate
- Seatbelt profiles for macOS
- Landlock rules for Linux
- No sandbox on Windows (matches reference implementation behavior, document as known gap)

## Phase 6: Polish (Week 20-24)

### P6-01: Binary optimization
- Strip symbols, LTO, codegen-units=1
- Target: <25MB binary
- Cross-compilation via cross-rs

### P6-02: Distribution
- cargo install
- Homebrew formula
- GitHub releases with binaries for all platforms
- npm wrapper (optional)

### P6-03: Hook system, vim mode, cost tracking

## Key Decisions

- **Codex crate extraction**: Copy, don't git-subtree. Clean break allows independent evolution
- **No OpenAI deps**: Remove all OpenAI API, auth, and model-specific code
- **Dual license**: MIT for new code, Apache-2.0 attribution for Codex-derived crates
- **Tokio**: Required for async streaming. Single-threaded runtime sufficient
- **Serde everywhere**: All API types derive Serialize/Deserialize

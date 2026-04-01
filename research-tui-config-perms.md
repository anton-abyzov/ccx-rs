# Research Report: ccx-rs TUI, Config, and Permissions Parity
**Date:** 2025-07  
**Scope:** `ccx-tui`, `ccx-config`, `ccx-permission`, `ccx-skill`, `ccx-prompt`, `ccx-compact`, `ccx-mcp`, `ccx-memory`, `ccx-sandbox`, `ccx-cli`  
**Comparison Target:** Claude Code TypeScript (anthropics/claude-code)

---

## 1. CLI / Commands

### Slash Commands Available
| Command | Rust (ccx) | TS Claude Code |
|---------|-----------|---------------|
| `/help` | ✅ | ✅ |
| `/exit` / `/quit` | ✅ | ✅ |
| `/clear` | ✅ | ✅ |
| `/cost` | ✅ | ✅ |
| `/model` | ✅ (read-only, no change) | ✅ (interactive change) |
| `/compact` | ⚠️ Stub only (prints "not yet implemented") | ✅ Full |
| `/init` | ⚠️ Stub only (prints "not yet implemented") | ✅ Full |
| `/version` | ✅ | ✅ |
| `/tools` | ✅ | ✅ |
| `/bug` | ❌ Missing | ✅ |
| `/review` | ❌ Missing | ✅ |
| `/pr_comments` | ❌ Missing | ✅ |
| `/terminal-setup` | ❌ Missing | ✅ |
| `/config` | ❌ Missing | ✅ |
| `/memory` | ❌ Missing | ✅ |
| `/vim` | ❌ Missing | ✅ |

**Notes:**
- Skills discovered via `ccx_skill::discover_all_skills()` are appended to the slash-command list and tab-completed.
- `/model` reads but does NOT change the active model mid-session (TS version supports interactive model switch).
- `/compact` and `/init` exist as stubs — they print a message but do nothing functional.

### CLI Flags
| Flag | Rust (ccx) | TS Claude Code |
|------|-----------|---------------|
| `--model` | ✅ (`chat --model`) | ✅ |
| `--permission-mode` / `-p` | ✅ (`chat --permission-mode`) | ✅ |
| `--prompt` / `-p` (single shot) | ✅ (`chat --prompt`) | `--print` / `-p` |
| `--print` | ❌ Missing flag name | ✅ |
| `--output-format` | ❌ Missing | ✅ (json/stream-json/text) |
| `--resume` | ❌ Missing | ✅ |
| `--continue` / `-c` | ❌ Missing | ✅ |
| `--api-key` | ✅ | ✅ (env preferred) |
| `--max-turns` | ✅ | ✅ |
| `--tui` | ✅ (Rust-specific) | N/A |
| `--dangerously-skip-permissions` | ✅ | ✅ |

**Default permission mode:** `bypass` (TS default is `default`). This is a notable difference — the Rust binary ships with bypass mode as default, which is less safe.

---

## 2. TUI / Inline Rendering

### Inline Mode (default)
- **Welcome panel:** ✅ Two-column layout with box-drawing chars, pet ASCII art, tips, recent activity (static "No recent activity")
- **User message display:** ✅ Gray background bar, `❯` prefix
- **Tool execution display:** ✅ Green dot `●` on start, `└ done` / `✗ error` on end, collapsible output (first 3 lines + `[+N more]` when >5 lines)
- **Markdown rendering:** ✅ Custom parser: headings (h1–h3), bold (`**`), inline code (backtick), links `[text](url)`, unordered/numbered lists, code blocks (triple backtick)
- **Streaming spinner:** ✅ `● Working...` shown while buffering, cleared on display
- **Input handling:** ✅ Rustyline with history persistence at `~/.ccx_history`
- **Tab completion:** ✅ `CcxCompleter` completes slash commands + discovered skills, with inline hints
- **Permission prompt:** ✅ Inline single-keypress (`y`/`n`/`a`) with AlwaysAllow session memory
- **"Recent activity" section:** ⚠️ Static placeholder ("No recent activity") — not real history

**Missing vs TS:**
- Multiline input (Shift+Enter) not implemented — Enter immediately submits
- Paste bracketed mode / large paste handling not implemented
- Image input not supported in inline mode
- Vim mode not implemented
- `/effort` mentioned in footer label but command not implemented
- Tool output collapse/expand is text-only, not interactive toggle

### Full-Screen TUI (ratatui, `--tui` flag)
- **Framework:** ratatui + crossterm
- **Screens:** Welcome + Chat (two-screen state machine)
- **Layout:** Title bar / main area / input area / footer — 4-row layout
- **Keyboard:** Ctrl+C/D quit, Enter send, arrows/Home/End/PageUp/Down navigation, Ctrl+L clear, Ctrl+U clear input
- **Streaming:** Text streamed into last assistant message
- **Tool display:** Tools shown as `[Tool: detail]` messages in chat (no collapsible view)
- **Tab completion:** ❌ Not wired in TUI mode (Tab key is a no-op in the full TUI)
- **Permission prompts:** ❌ Not wired in TUI mode (no `should_allow_tool` callback)
- **Slash commands:** ❌ Not handled in TUI mode (goes straight to agent)
- **Markdown rendering:** ❌ No markdown parsing in ratatui chat view — raw text

---

## 3. Config / Settings

### Settings File
| Feature | Rust (ccx) | TS Claude Code |
|---------|-----------|---------------|
| Location | `~/.claude/settings.json` | `~/.claude/settings.json` |
| Keys: `model` | ✅ | ✅ |
| Keys: `max_tokens` | ✅ | ✅ |
| Keys: `permissions.mode` | ✅ | ✅ |
| Keys: `permissions.allow` | ✅ | ✅ |
| Keys: `permissions.deny` | ✅ | ✅ |
| Keys: `theme` | ❌ | ✅ |
| Keys: `autoUpdaterStatus` | ❌ | ✅ |
| Keys: `hasCompletedProjectOnboarding` | ❌ | ✅ |
| Keys: `projects` | ❌ | ✅ (project-specific settings) |
| Keys: `env` (env var overrides) | ❌ | ✅ |
| Keys: `mcpServers` | ❌ | ✅ |
| Project-level settings file | ❌ (no `.claude/settings.json`) | ✅ |
| User-level vs project-level merge | ❌ (only user-level loaded) | ✅ |
| `CLAUDE_CODE_*` env var overrides | ❌ Not implemented | ✅ |

### CLAUDE.md
| Feature | Rust (ccx) | TS Claude Code |
|---------|-----------|---------------|
| Walk up from cwd to root | ✅ | ✅ |
| Check `~/.claude/CLAUDE.md` (global) | ✅ | ✅ |
| Order: global first, project last | ✅ | ✅ |
| YAML frontmatter parsing | ❌ Content injected raw, no frontmatter parse | ✅ |
| `@import` directives | ❌ | ✅ |
| `.claudeignore` support | ❌ | ✅ |
| Memory injection into system prompt | ❌ | ✅ (via `/memory` command) |

---

## 4. Permissions

### Permission Modes
| Mode | Rust | TS | Notes |
|------|------|----|----|
| `default` | ✅ | ✅ | Prompts for unrecognized tools |
| `plan` | ✅ | ✅ | Read-only auto, writes need approval |
| `bypassPermissions` | ✅ | ✅ | Everything allowed |
| `dontAsk` | ✅ | ✅ | Deny anything not explicitly allowed |
| `acceptEdits` | ✅ | ✅ | Auto-accept file edits |
| `auto` | ✅ | ✅ | Full auto |

All 6 modes are implemented. ✅

### Rule DSL
| Feature | Rust | TS |
|---------|------|----|
| `allow: ["Bash(git *)"]` | ✅ glob via `glob` crate | ✅ |
| `deny: ["Bash(rm *)"]` | ✅ | ✅ |
| First match wins | ✅ | ✅ |
| No-match → Ask | ✅ | ✅ |
| Inline tool-call string format | ✅ `Tool(args)` format | ✅ |

### Settings Cascade
| Layer | Rust | TS |
|-------|------|----|
| CLI flag | ✅ (`--permission-mode`) | ✅ |
| Session | ❌ No session layer | ✅ |
| Project (`.claude/settings.json`) | ❌ Not loaded | ✅ |
| User (`~/.claude/settings.json`) | ✅ | ✅ |
| Defaults | ✅ | ✅ |

**Critical gap:** Only user-level settings are loaded. Project-level `.claude/settings.json` is not loaded. The `merge_cascade()` function exists but is never called with multiple layers in practice (only a single user settings layer is used).

### Permission Integration
| Feature | Rust | TS |
|---------|------|----|
| `should_allow_tool()` in inline mode | ✅ Via `InlineCallback` | ✅ |
| Permission prompt in TUI full-screen | ❌ Not connected | ✅ |
| Permission prompt in single-shot mode | ❌ (bypassed by default) | ✅ |
| `classify_tool()` function | ✅ | ✅ |
| `decide()` function (mode + rules) | ✅ | ✅ |

---

## 5. Skills / CLAUDE.md

### Skill Discovery
| Location | Rust | TS |
|----------|------|----|
| `~/.claude/skills/` (flat .md files) | ✅ | ✅ |
| `~/.claude/skills/name/SKILL.md` | ✅ | ✅ |
| `.claude/skills/` (project-level) | ✅ | ✅ |
| Specweave plugin skills via nvm | ✅ (custom extension) | N/A |
| Bundle/built-in skills | ❌ | ✅ |

### Skill Features
| Feature | Rust | TS |
|---------|------|----|
| YAML frontmatter (`name`, `description`, `trigger`, `mode`) | ✅ | ✅ |
| Name derived from directory path (SpecWeave pattern) | ✅ | N/A |
| Inline execution mode | ✅ | ✅ |
| Agent (fork) execution mode | ⚠️ `SkillMode::Agent` defined, no agent spawn | ✅ |
| Trigger keywords | ✅ | ✅ |
| Args injection | ✅ | ✅ |
| Tab completion for skills | ✅ | ✅ |

---

## 6. System Prompt

### Sections Present
| Section | Rust | TS |
|---------|------|----|
| Role description | ✅ | ✅ |
| Environment (OS, arch, shell, cwd) | ✅ | ✅ |
| Git repo detection | ✅ | ✅ |
| Available tools list | ✅ | ✅ |
| CLAUDE.md content injection | ✅ | ✅ |
| Behavioral guidelines | ✅ | ✅ |
| Dynamic boundary (prompt caching) | ❌ | ✅ (cache_control: ephemeral) |
| Memory content injection | ❌ | ✅ |
| Platform-specific tips | ❌ | ✅ |
| Tool JSON schemas (full) | ❌ Only name+description, no JSON schema | ✅ |

**Key gap:** Tool schemas only provide name and description strings — the full JSON input schema is not included in the system prompt. TS version includes full JSON schema definitions which helps the model understand exact parameter structure.

---

## 7. Context Compression

| Feature | Rust | TS |
|---------|------|----|
| Threshold constant | ✅ `DEFAULT_THRESHOLD = 187_000` | ✅ ~187K tokens |
| `should_compact()` check | ✅ | ✅ |
| Token estimation | ⚠️ Simple heuristic (chars/3.5), no tiktoken | ✅ Real tokenizer |
| MicroCompact (strip tool results) | ✅ `micro_compact()` | ✅ |
| AutoCompact (LLM summarization) | ⚠️ Stub: takes first 2 + last 2 messages | ✅ Real LLM summarization |
| Post-compact file restoration | ❌ | ✅ (max 5 files, 50K tokens) |
| Image stripping before compaction | ❌ | ✅ |
| Integration into agent loop | ❌ Not called automatically | ✅ Automatic |
| `/compact` command wired up | ❌ Stub | ✅ |

---

## 8. MCP Client

| Feature | Rust | TS |
|---------|------|----|
| stdio transport | ✅ | ✅ |
| SSE transport | ❌ | ✅ |
| HTTP transport | ❌ | ✅ |
| `initialize` handshake | ✅ | ✅ |
| `tools/list` | ✅ | ✅ |
| `tools/call` | ✅ | ✅ |
| MCP tools registered in agent | ❌ Not wired into ToolRegistry | ✅ |
| `mcpServers` config loading | ❌ | ✅ |
| Tool discovery on startup | ❌ | ✅ |
| Resources support | ❌ (type defined, no impl) | ✅ |
| Notifications/subscriptions | ❌ | ✅ |

**Critical gap:** The MCP client exists as a library but is never instantiated from the CLI or agent loop. No `mcpServers` configuration is read, so MCP tools are never available at runtime.

---

## 9. Memory

| Feature | Rust | TS |
|---------|------|----|
| File-based storage | ✅ | ✅ |
| `MEMORY.md` index | ✅ | ✅ |
| Memory types (user/feedback/project/reference) | ✅ | ✅ |
| YAML frontmatter format | ✅ | ✅ |
| Default location | ⚠️ Not specified/configured — must be passed manually | `~/.claude/memory/` |
| Loading on startup | ❌ Never loaded | ✅ Auto-loaded |
| Injection into system prompt | ❌ | ✅ |
| `/memory` command | ❌ | ✅ |
| Truncation (200 lines, 25KB) | ❌ | ✅ |

**Critical gap:** The `MemoryStore` is never instantiated from the CLI. Memory is not loaded on startup, not injected into the system prompt, and there's no `/memory` command.

---

## 10. Sandbox

| Feature | Rust | TS |
|---------|------|----|
| macOS sandbox-exec (Seatbelt) | ✅ Profile generated | ✅ |
| Linux Landlock | ⚠️ Stub — runs unsandboxed | ✅ (seccomp/namespaces) |
| Windows | ❌ NoopSandbox | N/A |
| Sandbox actually called on Bash exec | ❌ `create_sandbox()` is called as a no-op stub in main | ✅ |
| Working directory always writable | ✅ In profile | ✅ |
| Network blocked by default | ✅ `allow_network: false` | ✅ |
| Integration into Bash tool | ❌ Not wired into `ccx-tools` Bash implementation | ✅ |
| Sandbox config from settings | ❌ | ✅ |

**Critical gap:** `create_sandbox()` is called in `main.rs` only as `let _ = ccx_sandbox::create_sandbox()` — a no-op to suppress the "unused import" warning. The sandbox is never actually applied to any tool execution.

---

## Gaps and Issues Found

### Critical (blocks functionality)
1. **Default permission mode is `bypass`** — the CLI defaults to `--permission-mode bypass`, meaning no permission checks run by default. This is the opposite of the TS default (`default` mode).
2. **MCP never activated** — `ccx-mcp` client exists but is never instantiated; MCP tools are unavailable.
3. **Memory never loaded** — `ccx-memory` store exists but is never started, loaded, or injected into prompts.
4. **Sandbox never applied** — `ccx-sandbox` profile generation works but `wrap_command()` is never called for tool execution; the sandbox is effectively disabled.
5. **Linux sandbox is a stub** — `LandlockSandbox::wrap_command()` explicitly runs the command without any sandboxing.
6. **Context compression not automated** — `should_compact()` and `create_summary()` exist but are never called from the agent loop; auto-compaction never triggers.

### High Priority
7. **Project-level settings not loaded** — `.claude/settings.json` in the project directory is never read; the cascade only has one layer.
8. **`/compact` is a stub** — prints "not yet implemented".
9. **`/init` is a stub** — prints "not yet implemented".
10. **Full-screen TUI lacks permission prompts** — `should_allow_tool()` is not connected in TUI mode, so all tools run without permission checks.
11. **Full-screen TUI has no slash command handling** — all input goes directly to the agent, bypassing slash command parsing.
12. **No `--resume` / `--continue` flags** — conversation resumption not implemented.
13. **No `--output-format`** — structured JSON output for programmatic use missing.

### Medium Priority
14. **Token counting is a heuristic** — chars/3.5 instead of a real tokenizer (tiktoken); inaccurate for code-heavy conversations.
15. **`auto_compact.create_summary()` is a stub** — just concatenates first 2 + last 2 messages, no LLM call.
16. **CLAUDE.md YAML frontmatter not parsed** — `@import` directives and frontmatter metadata are ignored.
17. **Tool JSON schemas not included in system prompt** — only name + description, not the full input schema.
18. **Settings missing many keys** — `theme`, `env`, `mcpServers`, project-specific blocks, `autoUpdaterStatus`, etc.
19. **Recent activity in welcome panel is static** — "No recent activity" is hardcoded.
20. **`SkillMode::Agent` not executed** — defined but fork/spawn logic is absent.

### Low Priority
21. **`/model` is read-only** — cannot change model mid-session.
22. **No multiline input** — Enter submits immediately; no Shift+Enter for newlines.
23. **Vim mode** missing.
24. **`/effort` referenced in footer** — command not implemented.
25. **Missing TS commands** — `/bug`, `/review`, `/pr_comments`, `/terminal-setup`, `/config`, `/memory`.
26. **Tab key is no-op** in full-screen TUI (completion not wired).

---

## What Works Well

1. **All 6 permission modes correctly defined** — Default, Plan, BypassPermissions, DontAsk, AcceptEdits, Auto with correct semantic methods (`allows_reads`, `allows_writes`, `allows_edits`, `allows_bash`).
2. **Permission rule DSL fully functional** — `allow`/`deny` with glob patterns, first-match-wins semantics, settings cascade `merge_cascade()`.
3. **Permission classifier (`classify_tool` + `decide`)** — correctly integrates mode + rules + tool category.
4. **Inline TUI rendering is polished** — welcome panel, styled tool indicators, collapsible output, markdown renderer with bold/code/lists/headings/links.
5. **Tab completion with hints** — `CcxCompleter` completes built-in commands + discovered skills, with inline type-ahead hints.
6. **`~/.ccx_history` persistence** — session history saved and restored.
7. **CLAUDE.md discovery** — walk-up algorithm with global `~/.claude/CLAUDE.md` correctly implemented and ordered.
8. **Skill loading system** — YAML frontmatter, name derivation from path (including SpecWeave `sw:` prefix convention), trigger keywords, inline execution.
9. **MCP client protocol** — JSON-RPC 2.0 over stdio with correct MCP protocol version (`2024-11-05`), `initialize` handshake, `tools/list`, `tools/call`.
10. **Memory store structure** — correct format (markdown + YAML frontmatter, `MEMORY.md` index regeneration, all 4 memory types).
11. **macOS Seatbelt profile** — correct sandbox-exec profile generation with `(deny default)`, configurable read/write paths, network toggle.
12. **Full-screen TUI is functional** — ratatui/crossterm backend, Welcome→Chat state machine, streaming text, scroll, keyboard input.
13. **`micro_compact`** — correctly strips `tool_result` blocks in JSON message arrays.
14. **Threshold constant matches TS** — `DEFAULT_THRESHOLD = 187_000` matches the TS value.
15. **Authentication integration** — `ccx_auth::resolve_auth()` supports both API key and Claude Code OAuth.
16. **`InlineCallback::should_allow_tool()`** — correctly wires permission prompts with AlwaysAllow session memory for the inline mode.

---

## Summary Table

| Area | Status | Completeness |
|------|--------|-------------|
| CLI flags | Partial | 60% |
| Slash commands | Partial | 55% |
| Inline TUI rendering | Good | 80% |
| Full-screen TUI | Partial | 55% |
| Config/settings | Partial | 50% |
| CLAUDE.md | Good | 75% |
| Permission modes | Complete | 100% |
| Permission rules/DSL | Complete | 100% |
| Settings cascade | Partial | 60% |
| Skills | Good | 75% |
| System prompt | Partial | 65% |
| Context compression | Stub | 30% |
| MCP client | Unconnected | 40% |
| Memory | Unconnected | 35% |
| Sandbox | Unconnected | 25% |

**Overall estimated parity: ~60%** — The core permission and config architecture is solid and well-designed. The main gaps are integration/wiring (MCP, memory, sandbox, compression are implemented as libraries but not connected to the agent loop) and a few stub commands.

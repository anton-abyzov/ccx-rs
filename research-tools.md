# Tools Parity Report: Rust ccx-rs vs TypeScript Claude Code

**Date:** 2025-01-27  
**Source (Rust):** `crates/ccx-tools/src/` (11 tools)  
**Source (TS):** `@anthropic-ai/claude-code@2.1.89` → `sdk-tools.d.ts` (official type definitions)

---

## Summary

The Rust implementation covers the **core schema** of all 11 tools correctly, but **misses several important parameters and behaviors** present in the official TypeScript Claude Code. The most critical gaps are in: Agent (missing `subagent_type`, `model`, `name`, `team_name`, `mode`, `isolation`, `run_in_background`), FileRead (missing **image/PDF support**, `pages` param), WebFetch (missing `prompt` param — TS version does LLM-based content extraction), WebSearch (missing `allowed_domains`/`blocked_domains`), NotebookEdit (uses `cell_index` integer instead of `cell_id` string, missing `edit_mode: insert/delete`), and TodoWrite (missing `activeForm` field).

---

## Tool-by-Tool Analysis

### 1. Bash ✅ GOOD PARITY

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `command` (required) | ✅ | ✅ |
| `timeout` (ms, max 600000) | ✅ | ✅ |
| `description` (optional) | ✅ | ✅ |
| `run_in_background` | ✅ | ✅ |
| `dangerouslyDisableSandbox` | ✅ | ❌ **MISSING** |
| Sandbox integration (seatbelt/Landlock) | ✅ | ❌ Not implemented |
| Output truncation | Implied | No explicit limit |
| Background: redirects stdout to file | TS writes to output file | Rust discards stdout/stderr |

**Gaps:**
- `dangerouslyDisableSandbox` parameter not supported
- Sandbox integration not present (SPEC.md Phase 5 item, expected gap)
- Background process: TS stores output to a file for later retrieval via Read tool; Rust simply discards output

---

### 2. Read (FileRead) ⚠️ SIGNIFICANT GAPS

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `file_path` (required) | ✅ | ✅ |
| `offset` (line number) | ✅ | ✅ |
| `limit` (line count) | ✅ | ✅ |
| `pages` (for PDF, e.g. "1-5") | ✅ | ❌ **MISSING** |
| Image file reading (base64 return) | ✅ JPEG/PNG/GIF/WebP | ❌ Returns "binary file" message |
| PDF reading (base64 return) | ✅ | ❌ Returns "binary file" message |
| Notebook reading (returns cells array) | ✅ `.ipynb` special handling | ❌ Returns "binary file" (JSON) |
| Structured output (filePath, content, numLines, startLine, totalLines) | ✅ | ❌ Flat string output |
| `file_unchanged` return type (caching) | ✅ | ❌ |

**Gaps (HIGH PRIORITY):**
- Image files returned as base64 with dimensions — critical for vision-capable workflows
- PDF files returned as base64 — critical for document analysis
- `pages` parameter for paginated PDF reading
- Notebook files (`.ipynb`) have special rendering returning cells array
- Output is structured JSON object vs flat string — affects how Claude processes results

---

### 3. Write (FileWrite) ✅ GOOD PARITY

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `file_path` (required) | ✅ | ✅ |
| `content` (required) | ✅ | ✅ |
| Auto-create parent directories | ✅ | ✅ |
| Overwrite behavior | ✅ (always overwrites) | ✅ |

**Gaps:** None significant. Rust implementation is fully equivalent.

---

### 4. Edit (FileEdit) ✅ GOOD PARITY

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `file_path` (required) | ✅ | ✅ |
| `old_string` (required) | ✅ | ✅ |
| `new_string` (required) | ✅ | ✅ |
| `replace_all` | ✅ | ✅ |
| Error on no-match | ✅ | ✅ |
| Error on multiple matches (without replace_all) | ✅ | ✅ |
| Atomic write (temp file + rename) | Not explicit in TS | ✅ (Rust extra safety) |

**Gaps:** None. Rust implementation matches TS schema exactly and adds atomic write safety.

---

### 5. Glob ✅ GOOD PARITY

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `pattern` (required) | ✅ | ✅ |
| `path` (optional, defaults to CWD) | ✅ | ✅ |
| Sort by modification time | Not specified in TS schema | ✅ (Rust extra feature) |
| Recursive patterns (`**`) | ✅ | ✅ |

**Gaps:** None significant. Rust actually adds mtime sorting not specified in TS.

---

### 6. Grep ✅ EXCELLENT PARITY

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `pattern` (required) | ✅ | ✅ |
| `path` | ✅ | ✅ |
| `glob` | ✅ | ✅ |
| `output_mode` (content/files_with_matches/count) | ✅ | ✅ |
| `-B` (lines before) | ✅ | ✅ |
| `-A` (lines after) | ✅ | ✅ |
| `-C` / `context` | ✅ both aliases | ✅ both aliases |
| `-n` (line numbers) | ✅ | ✅ |
| `-i` (case insensitive) | ✅ | ✅ |
| `type` (file type filter) | ✅ | ✅ |
| `head_limit` | ✅ (default 250) | ✅ (default 250) |
| `offset` | ✅ | ✅ |
| `multiline` | ✅ | ✅ |
| Requires `rg` (ripgrep) | ✅ (bundled in npm) | ✅ (must be on PATH) |

**Gaps:** 
- TS bundles ripgrep in vendor directory; Rust requires system `rg` on PATH
- TS allows `context` as an alias for `-C` (Rust supports this too via `input["context"]`)

---

### 7. WebFetch ⚠️ SCHEMA MISMATCH

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `url` (required) | ✅ | ✅ |
| `prompt` (required in TS!) | ✅ **REQUIRED** | ❌ **MISSING** |
| `timeout` | ❌ Not in TS schema | ✅ (Rust extension) |
| `max_size` | ❌ Not in TS schema | ✅ (Rust extension) |
| HTML stripping | ✅ (uses LLM with `prompt`) | ✅ (custom strip_html) |
| Image/binary content handling | ✅ | Partial (truncation only) |

**Gaps (HIGH PRIORITY):**
- TS `WebFetchInput` requires a `prompt` field: the fetched content is processed by Claude with this prompt (LLM-based extraction), not just raw text returned. The Rust implementation returns raw stripped HTML which is architecturally different.
- TS does NOT expose `timeout`/`max_size` to the model — these are internal implementation details. Rust exposes them as input parameters.

---

### 8. WebSearch ⚠️ MINOR GAPS

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `query` (required) | ✅ | ✅ |
| `allowed_domains` | ✅ | ❌ **MISSING** |
| `blocked_domains` | ✅ | ❌ **MISSING** |
| Brave Search API support | Implied (native provider) | ✅ (via env key) |
| DuckDuckGo fallback | Not in TS | ✅ (Rust addition) |

**Gaps:**
- `allowed_domains` / `blocked_domains` filtering not supported
- TS likely uses a native search provider (Claude's built-in web search) rather than scraping

---

### 9. Agent ⚠️ SIGNIFICANT GAPS

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `prompt` (required) | ✅ | ✅ |
| `description` (required) | ✅ | ✅ |
| `subagent_type` | ✅ (named agent type) | ❌ **MISSING** |
| `model` | ✅ `"sonnet"\|"opus"\|"haiku"` | ❌ **MISSING** |
| `run_in_background` | ✅ async agent launch | ❌ **MISSING** |
| `name` | ✅ addressable agent name | ❌ **MISSING** |
| `team_name` | ✅ team context | ❌ **MISSING** |
| `mode` | ✅ `acceptEdits\|bypassPermissions\|default\|dontAsk\|plan` | ❌ **MISSING** |
| `isolation` | ✅ `"worktree"` (git worktree isolation) | ❌ **MISSING** |
| Async output file | ✅ agent writes to `outputFile` | ❌ |
| `agentId` in output | ✅ | ❌ |
| Tool limits for sub-agent | Configurable per agent type | Hardcoded: Bash/Read/Write/Glob/Grep |
| Max turns | 20 (hardcoded) | 20 (hardcoded) |

**Gaps (HIGH PRIORITY):**
- `subagent_type` — lets you call named agent definitions (skill files)
- `model` override per agent invocation
- `run_in_background` — launch async agents and retrieve results via `TaskOutput`
- `name` — makes agents addressable for inter-agent messaging
- `team_name` — team routing/context
- `mode` — permission mode per spawned agent (especially `bypassPermissions`)
- `isolation: "worktree"` — isolated git worktree per agent (critical for safety)

---

### 10. TodoWrite ⚠️ MINOR SCHEMA GAP

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `todos` array (required) | ✅ | ✅ |
| `content` per todo | ✅ | ✅ |
| `status` enum (`pending\|in_progress\|completed`) | ✅ | ✅ |
| `activeForm` per todo | ✅ **REQUIRED in TS** | ❌ **MISSING** |
| Persistence to file | `.ccx-todos.json` in working dir | ✅ (same approach) |

**Gaps:**
- `activeForm` field is required in TS `TodoWriteInput` per todo item — this is likely an active form/context identifier for the UI. Rust ignores this field.
- TS todos are persisted in a server-side state store, not necessarily a file — Rust uses `.ccx-todos.json` as a reasonable approximation.

---

### 11. NotebookEdit ⚠️ SCHEMA MISMATCH

| Feature | TypeScript | Rust |
|---------|-----------|------|
| `notebook_path` (required) | ✅ | ✅ |
| `new_source` (required) | ✅ | ✅ |
| `cell_id` (string identifier) | ✅ **string cell_id** | ❌ Uses `cell_index` (integer) |
| `cell_index` (integer) | ❌ Not in TS | ✅ (Rust-only) |
| `cell_type` (`code\|markdown`) | ✅ | ✅ (also has `raw` in Rust) |
| `edit_mode` (`replace\|insert\|delete`) | ✅ | ❌ **MISSING** |
| Insert new cell | ✅ via `edit_mode: "insert"` | ❌ |
| Delete cell | ✅ via `edit_mode: "delete"` | ❌ |
| `raw` cell type | Not in TS schema | ✅ (Rust extension) |
| Clears outputs on code cell edit | ✅ | ✅ |
| Relative path support | Requires absolute path | ✅ (Rust extra) |

**Gaps (HIGH PRIORITY):**
- TS uses `cell_id` (string UUID-style identifier from notebook format), not `cell_index` integer — incompatible with real Jupyter notebooks where cells have stable IDs
- `edit_mode: "insert"` and `edit_mode: "delete"` — inserting after a specific cell and deleting a cell are not supported
- Missing insert-at-beginning support (`cell_id` omitted with `edit_mode: "insert"`)

---

## Tools Present in TS But NOT in Rust

The following tools exist in the official Claude Code but have **no Rust equivalent**:

| TS Tool | Purpose | Rust Status |
|---------|---------|-------------|
| `TaskOutput` | Get output from background agent/task | ❌ Not implemented |
| `TaskStop` | Stop a running background task | ❌ Not implemented |
| `ExitPlanMode` | Exit plan mode with allowed prompts | ❌ Not implemented |
| `ListMcpResources` | List available MCP resources | ❌ Not implemented |
| `ReadMcpResource` | Read a specific MCP resource | ❌ Not implemented |
| `Mcp` (generic) | Invoke any MCP tool | ❌ Not implemented |
| `AskUserQuestion` | Present structured UI question to user | ❌ Not implemented |
| `Config` | Read/write Claude configuration | ❌ Not implemented |
| `EnterWorktree` | Enter a git worktree context | ❌ Not implemented |
| `ExitWorktree` | Exit a git worktree context | ❌ Not implemented |

**Rust-only tools (not in TS Claude Code):**
| Rust Tool | Notes |
|-----------|-------|
| `TeamCreate` | Custom ccx-rs extension for team management |

---

## Priority Gap Summary

### 🔴 HIGH PRIORITY (breaks real-world parity)

1. **FileRead: Image/PDF support** — TS returns base64 encoded images/PDFs with metadata. Rust returns "binary file" message. This breaks vision and document workflows entirely.

2. **WebFetch: `prompt` parameter** — TS uses Claude to process the fetched page with a custom prompt. Rust just strips HTML tags. Architecturally different.

3. **Agent: Missing 7 parameters** — `subagent_type`, `model`, `run_in_background`, `name`, `team_name`, `mode`, `isolation`. These make the Agent tool nearly non-functional for team/async/permission workflows.

4. **NotebookEdit: `cell_id` vs `cell_index`** — Real Jupyter notebooks use cell IDs. The Rust integer index approach will break on notebooks edited by other tools.

5. **NotebookEdit: `edit_mode`** — Cannot insert or delete cells, only replace.

### 🟡 MEDIUM PRIORITY

6. **WebSearch: `allowed_domains`/`blocked_domains`** — Important for safety and targeted searches.

7. **TodoWrite: `activeForm` field** — Required by TS schema but unused by Rust.

8. **Bash: `dangerouslyDisableSandbox`** — Only matters once sandbox is implemented.

9. **Bash: Background process output file** — TS stores output to a recoverable file; Rust discards it.

### 🟢 LOW PRIORITY / EXTENSIONS

10. **WebFetch: `timeout`/`max_size`** — Rust exposes these but TS doesn't. Fine as extensions but shouldn't be required.

11. **Glob: mtime sorting** — Extra feature in Rust, not in TS spec.

12. **Missing tools** (TaskOutput, TaskStop, ExitPlanMode, MCP tools, AskUserQuestion, Config, Worktree) — Planned for later phases per SPEC.md.

---

## Rust Extension Features (Good Additions)

These are features Rust adds beyond the TS spec:

- **FileEdit**: Atomic write via temp-file + rename (data safety improvement)
- **FileEdit**: Line number reporting in success message
- **Glob**: Sort by modification time
- **Grep**: 30-second internal timeout (TS relies on process management)
- **FileRead**: Human-readable truncation notice
- **Bash**: Explicit propagation of key env vars (CARGO_HOME, RUSTUP_HOME, etc.)
- **Bash**: `stdin(null)` to prevent interactive hangs
- **WebFetch**: `max_size` parameter for body size control
- **WebSearch**: DuckDuckGo fallback when no API key set
- **NotebookEdit**: Relative path resolution
- **TeamCreate**: Novel team management tool (ccx-rs specific)

---

## Conclusion

The Rust ccx-rs tool implementations have **excellent parity** for the simple file/search tools (Bash, Write, Edit, Glob, Grep) and are production-ready. The **major gaps** are in the richer tools: FileRead (no image/PDF), WebFetch (no LLM post-processing), Agent (missing 7 input params making async/team/permission flows impossible), and NotebookEdit (incompatible cell addressing). Fixing FileRead image support and the Agent parameter set should be the highest priorities for the next development sprint.

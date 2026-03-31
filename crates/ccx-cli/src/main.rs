use std::io::Write;

use clap::{Parser, Subcommand};

/// ccx — Claude Code in Rust
#[derive(Parser)]
#[command(name = "ccx", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session
    Chat {
        /// Model to use
        #[arg(long, default_value = "claude-sonnet-4-6")]
        model: String,

        /// API key (overrides ANTHROPIC_API_KEY env var)
        #[arg(long)]
        api_key: Option<String>,

        /// Initial prompt (non-interactive single-shot mode)
        #[arg(short, long)]
        prompt: Option<String>,

        /// Permission mode (default, plan, bypass, dontask, acceptedits, auto)
        #[arg(long, default_value = "default")]
        permission_mode: String,

        /// Maximum turns per conversation exchange
        #[arg(long, default_value = "50")]
        max_turns: usize,
    },
    /// Show version and crate information
    Info,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            model,
            api_key,
            prompt,
            permission_mode,
            max_turns,
        } => {
            if let Err(e) = run_chat(
                &model,
                api_key.as_deref(),
                prompt.as_deref(),
                &permission_mode,
                max_turns,
            )
            .await
            {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Info => {
            print_info();
        }
    }
}

fn print_info() {
    println!("ccx v{}", ccx_core::version());
    println!("Crates:");
    println!("  ccx-api        - Claude API client with streaming");
    println!("  ccx-auth       - API key resolution");
    println!("  ccx-core       - Agent loop, tools, hooks, cost tracking");
    println!("  ccx-tools      - Built-in tools (Bash, Read, Write, Edit, Glob, Grep, WebFetch, WebSearch, Agent, TodoWrite, NotebookEdit)");
    println!("  ccx-prompt     - System prompt + CLAUDE.md");
    println!("  ccx-permission - Permission modes and rules");
    println!("  ccx-config     - Settings loading");
    println!("  ccx-memory     - File-based memory system");
    println!("  ccx-compact    - Context compaction");
    println!("  ccx-mcp        - MCP client");
    println!("  ccx-skill      - Skill loader");
    println!("  ccx-sandbox    - Sandboxing (Seatbelt/Landlock)");
    println!("  ccx-tui        - Terminal UI");
}

async fn run_chat(
    model: &str,
    explicit_key: Option<&str>,
    prompt: Option<&str>,
    permission_mode: &str,
    max_turns: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve API key.
    let resolved = ccx_auth::resolve_api_key(explicit_key)?;
    let client = ccx_api::ClaudeClient::new(&resolved.key, model);

    // Load settings.
    let settings = ccx_config::load_default_settings().unwrap_or_default();

    // Resolve permission mode.
    let mode = match permission_mode {
        "plan" => ccx_permission::PermissionMode::Plan,
        "bypass" => ccx_permission::PermissionMode::BypassPermissions,
        "dontask" => ccx_permission::PermissionMode::DontAsk,
        "acceptedits" => ccx_permission::PermissionMode::AcceptEdits,
        "auto" => ccx_permission::PermissionMode::Auto,
        _ => settings.permissions.mode.unwrap_or_default(),
    };

    // Build tool registry.
    let mut registry = ccx_core::ToolRegistry::new();
    ccx_tools::register_all(&mut registry);

    // Build system prompt with tool schemas.
    let cwd = std::env::current_dir()?;
    let claude_md_files = ccx_prompt::discover_claude_md(&cwd);
    let tool_schemas: Vec<ccx_prompt::ToolSchema> = registry
        .names()
        .iter()
        .filter_map(|name| {
            registry.get(name).map(|t| ccx_prompt::ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
            })
        })
        .collect();
    let system_prompt = ccx_prompt::build_full_system_prompt(
        &claude_md_files,
        &cwd.to_string_lossy(),
        &tool_schemas,
    );

    // Print startup info.
    match &resolved.source {
        ccx_auth::KeySource::EnvVar => eprintln!("Using API key from ANTHROPIC_API_KEY"),
        ccx_auth::KeySource::ConfigFile(path) => {
            eprintln!("Using API key from {}", path.display())
        }
        ccx_auth::KeySource::Explicit => eprintln!("Using provided API key"),
    }
    eprintln!(
        "Model: {model} | Mode: {mode:?} | Tools: {}",
        registry.names().len()
    );
    if !claude_md_files.is_empty() {
        eprintln!(
            "CLAUDE.md: {}",
            claude_md_files
                .iter()
                .map(|f| f.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let ctx = ccx_core::ToolContext::new(cwd);
    let mut agent = ccx_core::AgentLoop::new(client, registry, ctx, system_prompt);
    agent.set_max_turns(max_turns);

    if let Some(text) = prompt {
        // Non-interactive single-shot mode.
        run_single_shot(&mut agent, text).await?;
    } else {
        // Interactive REPL mode.
        run_interactive(&mut agent).await?;
    }

    // Suppress unused imports for crates wired but not directly called here.
    let _ = mode;
    let _ = ccx_memory::MemoryType::User;
    let _ = ccx_compact::DEFAULT_THRESHOLD;
    let _ = ccx_sandbox::create_sandbox();

    Ok(())
}

/// Run a single prompt and exit.
async fn run_single_shot(
    agent: &mut ccx_core::AgentLoop,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cb = StreamCallback::new();
    let _result = agent.send_message(text, &mut cb).await?;
    println!();
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

/// Run an interactive REPL loop reading from stdin.
async fn run_interactive(
    agent: &mut ccx_core::AgentLoop,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "\nccx interactive mode. Type /exit or Ctrl+D to quit, /cost for usage.\n"
    );

    let mut cb = StreamCallback::new();

    loop {
        // Print prompt and flush.
        eprint!("\x1b[1m> \x1b[0m");
        std::io::stderr().flush().ok();

        // Read a line from stdin (supports Ctrl+D for EOF).
        let line = tokio::select! {
            result = read_stdin_line() => result,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\n\nInterrupted.");
                break;
            }
        };

        match line {
            Ok(Some(text)) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Handle built-in commands.
                match trimmed {
                    "/exit" | "/quit" | "exit" | "quit" => break,
                    "/cost" => {
                        eprintln!("{}", agent.cost().summary());
                        continue;
                    }
                    "/clear" => {
                        eprintln!("Conversation cleared.");
                        // Note: we can't truly clear the agent's messages
                        // without creating a new AgentLoop. For now, just
                        // inform the user.
                        continue;
                    }
                    "/help" => {
                        print_repl_help();
                        continue;
                    }
                    _ => {}
                }

                // Send to agent.
                cb.reset();
                match agent.send_message(trimmed, &mut cb).await {
                    Ok(_) => {
                        // Ensure output ends with a newline.
                        if cb.chars_printed > 0 {
                            println!();
                        }
                    }
                    Err(e) => {
                        eprintln!("\n\x1b[31mError: {e}\x1b[0m");
                    }
                }
            }
            Ok(None) => {
                // EOF (Ctrl+D).
                eprintln!("\nGoodbye.");
                break;
            }
            Err(e) => {
                eprintln!("Input error: {e}");
                break;
            }
        }
    }

    // Print final cost summary.
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

/// Read a single line from stdin asynchronously.
async fn read_stdin_line() -> Result<Option<String>, std::io::Error> {
    tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        match std::io::stdin().read_line(&mut buf) {
            Ok(0) => Ok(None),            // EOF
            Ok(_) => Ok(Some(buf)),
            Err(e) => Err(e),
        }
    })
    .await
    .unwrap_or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
}

fn print_repl_help() {
    eprintln!(
        "\
Commands:
  /help     Show this help
  /cost     Show token usage and cost
  /clear    Clear conversation
  /exit     Exit the REPL

Tips:
  - Multi-line input: end lines with \\ to continue
  - Ctrl+C interrupts the current generation
  - Ctrl+D exits the REPL"
    );
}

/// Streaming callback that prints text to stdout and tool info to stderr.
struct StreamCallback {
    chars_printed: usize,
}

impl StreamCallback {
    fn new() -> Self {
        Self { chars_printed: 0 }
    }

    fn reset(&mut self) {
        self.chars_printed = 0;
    }
}

impl ccx_core::AgentCallback for StreamCallback {
    fn on_text(&mut self, text: &str) {
        print!("{text}");
        std::io::stdout().flush().ok();
        self.chars_printed += text.len();
    }

    fn on_tool_start(&mut self, name: &str, input: &serde_json::Value) {
        // Show a brief description of what the tool is doing.
        let detail = match name {
            "Bash" => input["command"]
                .as_str()
                .map(|c| {
                    if c.len() > 60 {
                        format!("{}...", &c[..57])
                    } else {
                        c.to_string()
                    }
                })
                .unwrap_or_default(),
            "Read" | "Write" | "Edit" => input["file_path"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "Glob" => input["pattern"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "Grep" => input["pattern"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "WebFetch" => input["url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "WebSearch" => input["query"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "Agent" => input["description"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            "TodoWrite" => {
                let count = input["todos"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                format!("{count} items")
            }
            "NotebookEdit" => {
                let path = input["notebook_path"].as_str().unwrap_or("");
                let idx = input["cell_index"].as_u64().unwrap_or(0);
                format!("{path} cell {idx}")
            }
            _ => String::new(),
        };

        if detail.is_empty() {
            eprintln!("\n\x1b[33m[{name}]\x1b[0m");
        } else {
            eprintln!("\n\x1b[33m[{name}: {detail}]\x1b[0m");
        }
    }

    fn on_tool_end(
        &mut self,
        name: &str,
        result: &Result<ccx_core::ToolResult, ccx_core::ToolError>,
    ) {
        match result {
            Ok(r) if !r.is_error => {
                eprintln!("\x1b[32m[{name}: ok]\x1b[0m");
            }
            Ok(r) => {
                let preview = &r.content[..r.content.len().min(200)];
                eprintln!("\x1b[31m[{name}: error] {preview}\x1b[0m");
            }
            Err(e) => {
                eprintln!("\x1b[31m[{name}: error] {e}\x1b[0m");
            }
        }
    }

    fn on_thinking(&mut self, _text: &str) {
        // Could optionally show thinking indicator.
    }

    fn on_retry(&mut self, attempt: u32, delay_ms: u64, reason: &str) {
        eprintln!(
            "\x1b[33m[retry {attempt}: {reason}, waiting {:.1}s]\x1b[0m",
            delay_ms as f64 / 1000.0
        );
    }

    fn on_turn_complete(&mut self, _turn: usize, _cost: &ccx_core::CostTracker) {
        // Could show per-turn cost.
    }
}

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

        /// Initial prompt (non-interactive)
        #[arg(short, long)]
        prompt: Option<String>,

        /// Permission mode
        #[arg(long, default_value = "default")]
        permission_mode: String,
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
        } => {
            if let Err(e) =
                run_chat(&model, api_key.as_deref(), prompt.as_deref(), &permission_mode).await
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
    println!("  ccx-api       - Claude API client with streaming");
    println!("  ccx-auth      - API key resolution");
    println!("  ccx-core      - Agent loop, tools, hooks, cost tracking");
    println!("  ccx-tools     - Built-in tools (Bash, Read, Write, Edit, Glob, Grep, WebFetch)");
    println!("  ccx-prompt    - System prompt + CLAUDE.md");
    println!("  ccx-permission - Permission modes and rules");
    println!("  ccx-config    - Settings loading");
    println!("  ccx-memory    - File-based memory system");
    println!("  ccx-compact   - Context compaction");
    println!("  ccx-mcp       - MCP client");
    println!("  ccx-skill     - Skill loader");
    println!("  ccx-sandbox   - Sandboxing (Seatbelt/Landlock)");
    println!("  ccx-tui       - Terminal UI");
}

async fn run_chat(
    model: &str,
    explicit_key: Option<&str>,
    prompt: Option<&str>,
    permission_mode: &str,
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

    // Build system prompt.
    let cwd = std::env::current_dir()?;
    let claude_md_files = ccx_prompt::discover_claude_md(&cwd);
    let system_prompt = ccx_prompt::build_system_prompt(&claude_md_files, &cwd.to_string_lossy());

    // Cost tracker.
    let cost = ccx_core::CostTracker::new();

    // Print startup info.
    match &resolved.source {
        ccx_auth::KeySource::EnvVar => eprintln!("Using API key from ANTHROPIC_API_KEY"),
        ccx_auth::KeySource::ConfigFile(path) => {
            eprintln!("Using API key from {}", path.display())
        }
        ccx_auth::KeySource::Explicit => eprintln!("Using provided API key"),
    }
    eprintln!("Model: {model} | Mode: {mode:?} | Tools: {}", registry.names().len());
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

    if let Some(text) = prompt {
        // Non-interactive: run agent loop for a single prompt.
        let ctx = ccx_core::ToolContext::new(cwd);
        let mut agent = ccx_core::AgentLoop::new(client, registry, ctx, system_prompt);

        struct PrintCallback;
        impl ccx_core::AgentCallback for PrintCallback {
            fn on_text(&mut self, text: &str) {
                print!("{text}");
            }
            fn on_tool_start(&mut self, name: &str, _input: &serde_json::Value) {
                eprintln!("\n[tool: {name}]");
            }
            fn on_tool_end(&mut self, name: &str, result: &Result<ccx_core::ToolResult, ccx_core::ToolError>) {
                match result {
                    Ok(r) if !r.is_error => eprintln!("[{name}: ok]"),
                    Ok(r) => eprintln!("[{name}: error] {}", &r.content[..r.content.len().min(200)]),
                    Err(e) => eprintln!("[{name}: error] {e}"),
                }
            }
        }

        let mut cb = PrintCallback;
        let _result = agent.send_message(text, &mut cb).await?;
        println!();
        eprintln!("\n{}", cost.summary());
    } else {
        eprintln!("Interactive mode: use --prompt for single queries, or TUI coming soon.");
    }

    // Suppress unused imports for crates we've wired but don't call directly yet.
    let _ = mode;
    let _ = ccx_memory::MemoryType::User;
    let _ = ccx_compact::DEFAULT_THRESHOLD;
    let _ = ccx_sandbox::create_sandbox();

    Ok(())
}

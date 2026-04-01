use std::io::Write;
use std::sync::mpsc;

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

        /// Use full-screen TUI instead of inline rendering
        #[arg(long)]
        tui: bool,
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
            tui,
        } => {
            if let Err(e) = run_chat(
                &model,
                api_key.as_deref(),
                prompt.as_deref(),
                &permission_mode,
                max_turns,
                tui,
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
    use_tui: bool,
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

    let tool_count = registry.names().len();

    let ctx = ccx_core::ToolContext::new(cwd.clone());
    let mut agent = ccx_core::AgentLoop::new(client, registry, ctx, system_prompt);
    agent.set_max_turns(max_turns);

    if let Some(text) = prompt {
        // Non-interactive single-shot mode.
        match &resolved.source {
            ccx_auth::KeySource::EnvVar => eprintln!("Using API key from ANTHROPIC_API_KEY"),
            ccx_auth::KeySource::ConfigFile(path) => {
                eprintln!("Using API key from {}", path.display())
            }
            ccx_auth::KeySource::Explicit => eprintln!("Using provided API key"),
        }
        eprintln!("Model: {model} | Mode: {mode:?} | Tools: {tool_count}");
        run_single_shot(&mut agent, text).await?;
    } else {
        // Interactive mode (inline default, full-screen TUI with --tui).
        let auth_source = match &resolved.source {
            ccx_auth::KeySource::EnvVar => "Env".to_string(),
            ccx_auth::KeySource::ConfigFile(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Config".into()),
            ccx_auth::KeySource::Explicit => "API Key".into(),
        };
        let cwd_display = shorten_home(&cwd);

        if use_tui {
            run_tui_mode(&mut agent, model, &auth_source, &cwd_display, tool_count).await?;
        } else {
            run_inline_mode(&mut agent, model, &auth_source, &cwd_display, tool_count).await?;
        }
    }

    // Suppress unused imports for crates wired but not directly called here.
    let _ = mode;
    let _ = ccx_memory::MemoryType::User;
    let _ = ccx_compact::DEFAULT_THRESHOLD;
    let _ = ccx_sandbox::create_sandbox();

    Ok(())
}

/// Shorten path by replacing $HOME with ~.
fn shorten_home(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy();
        let path_str = path.to_string_lossy();
        if path_str.starts_with(home_str.as_ref()) {
            return format!("~{}", &path_str[home_str.len()..]);
        }
    }
    path.to_string_lossy().to_string()
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

/// Run the full TUI with welcome screen, chat, and streaming.
async fn run_tui_mode(
    agent: &mut ccx_core::AgentLoop,
    model: &str,
    auth_source: &str,
    cwd_display: &str,
    tool_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tui_tx, tui_rx) = mpsc::channel::<ccx_tui::TuiEvent>();
    let (input_tx, input_rx) = mpsc::channel::<ccx_tui::TuiInput>();

    let welcome = ccx_tui::WelcomeInfo {
        model: model.to_string(),
        auth_source: auth_source.to_string(),
        cwd: cwd_display.to_string(),
        tool_count,
    };

    // Spawn TUI thread (blocking — owns the terminal).
    let tui_handle = std::thread::spawn(move || {
        ccx_tui::run_tui_configured(tui_rx, input_tx, welcome)
    });

    // Agent loop: wait for user input, send to API, push events to TUI.
    loop {
        let user_input = match input_rx.recv() {
            Ok(ccx_tui::TuiInput::Message(text)) => text,
            Ok(ccx_tui::TuiInput::Quit) | Err(_) => break,
        };

        let mut cb = TuiCallback {
            tx: tui_tx.clone(),
        };

        match agent.send_message(&user_input, &mut cb).await {
            Ok(_) => {}
            Err(e) => {
                let _ = tui_tx.send(ccx_tui::TuiEvent::NewMessage(ccx_tui::ChatMessage {
                    role: ccx_tui::ChatRole::Error,
                    content: format!("Error: {e}"),
                }));
            }
        }
    }

    let _ = tui_tx.send(ccx_tui::TuiEvent::Quit);
    let _ = tui_handle.join();

    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

/// Callback that forwards agent events to the TUI channel.
struct TuiCallback {
    tx: mpsc::Sender<ccx_tui::TuiEvent>,
}

impl ccx_core::AgentCallback for TuiCallback {
    fn on_text(&mut self, text: &str) {
        let _ = self
            .tx
            .send(ccx_tui::TuiEvent::StreamText(text.to_string()));
    }

    fn on_tool_start(&mut self, name: &str, input: &serde_json::Value) {
        let detail = extract_tool_detail(name, input);
        let _ = self.tx.send(ccx_tui::TuiEvent::ToolStart {
            name: name.to_string(),
            detail,
        });
    }

    fn on_tool_end(
        &mut self,
        name: &str,
        result: &Result<ccx_core::ToolResult, ccx_core::ToolError>,
    ) {
        let (success, preview) = match result {
            Ok(r) if !r.is_error => (true, String::new()),
            Ok(r) => (false, r.content[..r.content.len().min(200)].to_string()),
            Err(e) => (false, e.to_string()),
        };
        let _ = self.tx.send(ccx_tui::TuiEvent::ToolEnd {
            name: name.to_string(),
            success,
            preview,
        });
    }

    fn on_thinking(&mut self, _text: &str) {}

    fn on_retry(&mut self, attempt: u32, delay_ms: u64, reason: &str) {
        let _ = self
            .tx
            .send(ccx_tui::TuiEvent::NewMessage(ccx_tui::ChatMessage {
                role: ccx_tui::ChatRole::Tool,
                content: format!(
                    "[retry {attempt}: {reason}, waiting {:.1}s]",
                    delay_ms as f64 / 1000.0
                ),
            }));
    }

    fn on_turn_complete(&mut self, _turn: usize, _cost: &ccx_core::CostTracker) {}
}

/// Streaming callback for single-shot mode (prints to stdout/stderr).
struct StreamCallback {
    chars_printed: usize,
}

impl StreamCallback {
    fn new() -> Self {
        Self { chars_printed: 0 }
    }
}

impl ccx_core::AgentCallback for StreamCallback {
    fn on_text(&mut self, text: &str) {
        print!("{text}");
        std::io::stdout().flush().ok();
        self.chars_printed += text.len();
    }

    fn on_tool_start(&mut self, name: &str, input: &serde_json::Value) {
        let detail = extract_tool_detail(name, input);
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

    fn on_thinking(&mut self, _text: &str) {}

    fn on_retry(&mut self, attempt: u32, delay_ms: u64, reason: &str) {
        eprintln!(
            "\x1b[33m[retry {attempt}: {reason}, waiting {:.1}s]\x1b[0m",
            delay_ms as f64 / 1000.0
        );
    }

    fn on_turn_complete(&mut self, _turn: usize, _cost: &ccx_core::CostTracker) {}
}

/// Extract a human-readable detail string for tool start events.
fn extract_tool_detail(name: &str, input: &serde_json::Value) -> String {
    match name {
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
        "Glob" => input["pattern"].as_str().unwrap_or("").to_string(),
        "Grep" => input["pattern"].as_str().unwrap_or("").to_string(),
        "WebFetch" => input["url"].as_str().unwrap_or("").to_string(),
        "WebSearch" => input["query"].as_str().unwrap_or("").to_string(),
        "Agent" => input["description"].as_str().unwrap_or("").to_string(),
        "TodoWrite" => {
            let count = input["todos"].as_array().map(|a| a.len()).unwrap_or(0);
            format!("{count} items")
        }
        "NotebookEdit" => {
            let path = input["notebook_path"].as_str().unwrap_or("");
            let idx = input["cell_index"].as_u64().unwrap_or(0);
            format!("{path} cell {idx}")
        }
        _ => String::new(),
    }
}

/// Callback for inline rendering mode.
struct InlineCallback {
    in_text: bool,
}

impl InlineCallback {
    fn new() -> Self {
        Self { in_text: false }
    }

    fn finish_text(&mut self) {
        if self.in_text {
            println!();
            self.in_text = false;
        }
    }
}

impl ccx_core::AgentCallback for InlineCallback {
    fn on_text(&mut self, text: &str) {
        self.in_text = true;
        ccx_tui::inline::render_text(text);
    }

    fn on_tool_start(&mut self, name: &str, input: &serde_json::Value) {
        self.finish_text();
        let detail = extract_tool_detail(name, input);
        ccx_tui::inline::render_tool_start(name, &detail);
    }

    fn on_tool_end(
        &mut self,
        _name: &str,
        result: &Result<ccx_core::ToolResult, ccx_core::ToolError>,
    ) {
        let (success, preview) = match result {
            Ok(r) if !r.is_error => {
                let p: String = r.content.chars().take(500).collect();
                (true, p)
            }
            Ok(r) => {
                let p: String = r.content.chars().take(200).collect();
                (false, p)
            }
            Err(e) => (false, e.to_string()),
        };
        ccx_tui::inline::render_tool_end(success, &preview);
    }

    fn on_thinking(&mut self, _text: &str) {}

    fn on_retry(&mut self, attempt: u32, delay_ms: u64, reason: &str) {
        self.finish_text();
        ccx_tui::inline::render_error(&format!(
            "retry {attempt}: {reason}, waiting {:.1}s",
            delay_ms as f64 / 1000.0
        ));
    }

    fn on_turn_complete(&mut self, _turn: usize, _cost: &ccx_core::CostTracker) {
        self.finish_text();
    }
}

/// Run inline interactive mode (default — no full-screen).
async fn run_inline_mode(
    agent: &mut ccx_core::AgentLoop,
    model: &str,
    auth_source: &str,
    cwd_display: &str,
    tool_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    ccx_tui::inline::render_welcome(model, auth_source, cwd_display, tool_count);

    loop {
        ccx_tui::inline::render_prompt();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() || input.is_empty() {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        match input {
            "/exit" | "/quit" => break,
            "/help" => {
                println!("Commands: /exit, /quit, /clear, /cost, /help");
                continue;
            }
            "/clear" => {
                print!("\x1b[2J\x1b[H");
                std::io::stdout().flush()?;
                continue;
            }
            "/cost" => {
                println!("{}", agent.cost().summary());
                continue;
            }
            _ => {}
        }

        ccx_tui::inline::render_user_message(input);

        let mut cb = InlineCallback::new();
        match agent.send_message(input, &mut cb).await {
            Ok(_) => cb.finish_text(),
            Err(e) => {
                cb.finish_text();
                ccx_tui::inline::render_error(&format!("Error: {e}"));
            }
        }

        ccx_tui::inline::render_separator();
    }

    ccx_tui::inline::render_footer(model);
    println!("\nGoodbye!");
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

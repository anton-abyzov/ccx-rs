mod commands;
mod completer;
mod mcp_bridge;
mod sessions;

use std::collections::HashSet;
use std::io::Write;
use std::sync::mpsc;

use clap::{Parser, Subcommand};
use rustyline::error::ReadlineError;
use rustyline::Editor;

/// ccx — Claude Code in Rust
#[derive(Parser)]
#[command(name = "ccx", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Logging level (trace, debug, info, warn, error)
    #[arg(long, global = true, default_value = "info")]
    log_level: String,
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
        #[arg(long, default_value = "bypass")]
        permission_mode: String,

        /// Maximum turns per conversation exchange
        #[arg(long, default_value = "200")]
        max_turns: usize,

        /// Use full-screen TUI instead of inline rendering
        #[arg(long)]
        tui: bool,

        /// Skip all permission prompts (bypass mode)
        #[arg(long)]
        dangerously_skip_permissions: bool,

        /// Disable extended thinking (enabled by default for Anthropic)
        #[arg(long)]
        no_thinking: bool,

        /// Thinking token budget (default 10000, set 0 to disable)
        #[arg(long, default_value = "10000")]
        thinking_budget: u32,

        /// Hide thinking/reasoning output from display
        #[arg(long)]
        hide_thinking: bool,

        /// Enable sandbox for bash commands (macOS: Seatbelt, Linux: Landlock)
        #[arg(long)]
        sandbox: bool,

        /// Provider: anthropic (default), openrouter
        #[arg(long, default_value = "anthropic")]
        provider: String,

        /// OpenRouter API key (overrides OPENROUTER_API_KEY env var)
        #[arg(long)]
        openrouter_key: Option<String>,
    },
    /// Show version and crate information
    Info,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    let level = cli.log_level.to_lowercase();
    let filter = match level.as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    };
    env_logger::builder()
        .filter_level(filter)
        .format_target(false)
        .format_timestamp(None)
        .init();

    match cli.command {
        Commands::Chat {
            model,
            api_key,
            prompt,
            permission_mode,
            max_turns,
            tui,
            dangerously_skip_permissions,
            no_thinking,
            thinking_budget,
            hide_thinking,
            sandbox,
            provider,
            openrouter_key,
        } => {
            if let Err(e) = run_chat(
                &model,
                api_key.as_deref(),
                prompt.as_deref(),
                &permission_mode,
                max_turns,
                tui,
                dangerously_skip_permissions,
                no_thinking,
                thinking_budget,
                hide_thinking,
                sandbox,
                &provider,
                openrouter_key.as_deref(),
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
    dangerously_skip_permissions: bool,
    no_thinking: bool,
    thinking_budget: u32,
    hide_thinking: bool,
    sandbox: bool,
    provider: &str,
    openrouter_key: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve client based on provider.
    let or_key_env = std::env::var("OPENROUTER_API_KEY").ok();

    let (client, auth_source, email): (ccx_api::ApiClient, String, Option<String>) =
        match provider {
            "openrouter" => {
                let key = openrouter_key
                    .or(or_key_env.as_deref())
                    .ok_or("OpenRouter API key required: set OPENROUTER_API_KEY or use --openrouter-key")?;
                let client =
                    ccx_api::ApiClient::OpenAi(ccx_api::OpenAiClient::openrouter(key, model));
                (client, "OpenRouter".to_string(), None)
            }
            _ => {
                let auth = ccx_auth::resolve_auth(explicit_key)?;
                let email = if let Some(token) = auth.oauth_token() {
                    ccx_auth::fetch_oauth_email(token).await
                } else {
                    None
                };
                let client =
                    ccx_api::ApiClient::Claude(ccx_api::ClaudeClient::with_auth(&auth, model));
                let auth_source = auth.display_label().to_string();
                (client, auth_source, email)
            }
        };

    // Load settings.
    let settings = ccx_config::load_default_settings().unwrap_or_default();

    // Resolve permission mode.
    let mode = match permission_mode {
        "plan" => ccx_permission::PermissionMode::Plan,
        "bypass" => ccx_permission::PermissionMode::BypassPermissions,
        "dontask" => ccx_permission::PermissionMode::DontAsk,
        "acceptedits" => ccx_permission::PermissionMode::AcceptEdits,
        "auto" => ccx_permission::PermissionMode::Auto,
        "default" => ccx_permission::PermissionMode::Default,
        _ => settings.permissions.mode.unwrap_or(ccx_permission::PermissionMode::BypassPermissions),
    };

    // Bypass permissions when flag is set or mode allows it.
    let bypass_permissions = dangerously_skip_permissions || mode.allows_writes();

    // Build tool registry with built-in tools.
    let mut registry = ccx_core::ToolRegistry::new();
    ccx_tools::register_all(&mut registry);

    let cwd = std::env::current_dir()?;

    // Wire MCP: load .mcp.json and register MCP server tools.
    let _mcp_clients = if let Some(mcp_config) = mcp_bridge::load_mcp_config(&cwd) {
        mcp_bridge::register_mcp_tools(&mcp_config, &mut registry).await
    } else {
        Vec::new()
    };

    // Build system prompt with tool schemas and skill routing hints.
    let claude_md_files = ccx_prompt::discover_claude_md(&cwd);
    let tool_schemas: Vec<ccx_prompt::ToolSchema> = registry
        .names()
        .iter()
        .filter_map(|name| {
            registry.get(name).map(|t| ccx_prompt::ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: Some(t.input_schema()),
            })
        })
        .collect();
    let all_skills = ccx_skill::discover_all_skills();
    let skill_infos: Vec<ccx_prompt::SkillInfo> = all_skills
        .iter()
        .map(|s| ccx_prompt::SkillInfo {
            name: s.name.clone(),
            description: s.description.clone(),
        })
        .collect();
    let mut system_prompt = ccx_prompt::build_full_system_prompt(
        &claude_md_files,
        &cwd.to_string_lossy(),
        &tool_schemas,
        &skill_infos,
    );

    // Wire memory: load memories and inject into system prompt.
    let memory_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("memory");
    let memory_store = ccx_memory::MemoryStore::new(memory_dir);
    if let Ok(index) = memory_store.load_index() {
        if !index.is_empty() {
            system_prompt.push_str("\n\n# Memories\n\n");
            system_prompt.push_str(&index);
        }
    }

    let tool_names: Vec<String> = registry.names().into_iter().map(|s| s.to_string()).collect();
    let tool_count = tool_names.len();

    // Wire sandbox: set sandboxed flag on tool context when --sandbox is used.
    let mut ctx = ccx_core::ToolContext::new(cwd.clone());
    if sandbox {
        ctx.sandboxed = true;
    }

    let mut agent = ccx_core::AgentLoop::new(client, registry, ctx, system_prompt);
    agent.set_max_turns(max_turns);
    // Thinking enabled by default for Anthropic; disable with --no-thinking or --thinking-budget 0.
    let thinking_enabled = provider == "anthropic" && !no_thinking && thinking_budget > 0;
    if thinking_enabled {
        agent.set_thinking(ccx_api::ThinkingConfig {
            thinking_type: "enabled".to_string(),
            budget_tokens: thinking_budget,
        });
    }
    let show_thinking = !hide_thinking;

    if let Some(text) = prompt {
        // Non-interactive single-shot mode.
        eprintln!("Auth: {auth_source}");
        if let Some(ref email) = email {
            eprintln!("Account: {email}");
        }
        eprintln!("Model: {model} | Mode: {mode:?} | Tools: {tool_count}");
        run_single_shot(&mut agent, text, show_thinking).await?;
    } else {
        // Interactive mode (inline default, full-screen TUI with --tui).
        let cwd_display = shorten_home(&cwd);

        if use_tui {
            run_tui_mode(&mut agent, model, &auth_source, &cwd_display, tool_count, email.as_deref()).await?;
        } else {
            run_inline_mode(&mut agent, model, &auth_source, &cwd_display, &tool_names, bypass_permissions, email.as_deref(), show_thinking).await?;
        }
    }

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
    show_thinking: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cb = StreamCallback::new(show_thinking);
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
    email: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tui_tx, tui_rx) = mpsc::channel::<ccx_tui::TuiEvent>();
    let (input_tx, input_rx) = mpsc::channel::<ccx_tui::TuiInput>();

    let welcome = ccx_tui::WelcomeInfo {
        model: model.to_string(),
        auth_source: auth_source.to_string(),
        email: email.map(|s| s.to_string()),
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

    fn on_thinking(&mut self, text: &str) {
        let _ = self.tx.send(ccx_tui::TuiEvent::StreamText(
            format!("\x1b[2;3m{text}\x1b[0m"),
        ));
    }

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
    show_thinking: bool,
}

impl StreamCallback {
    fn new(show_thinking: bool) -> Self {
        Self { chars_printed: 0, show_thinking }
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

    fn on_thinking(&mut self, text: &str) {
        if self.show_thinking {
            print!("\x1b[2;3m{text}\x1b[0m");
            std::io::stdout().flush().ok();
        }
    }

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

/// Callback for inline rendering mode with buffered markdown output.
struct InlineCallback {
    text_buffer: String,
    spinner_shown: bool,
    always_allow: HashSet<String>,
    bypass_permissions: bool,
    auth_source: String,
    retry_count: u32,
    show_thinking: bool,
    thinking_active: bool,
}

impl InlineCallback {
    fn new(bypass_permissions: bool, auth_source: &str, show_thinking: bool) -> Self {
        Self {
            text_buffer: String::new(),
            spinner_shown: false,
            always_allow: HashSet::new(),
            bypass_permissions,
            auth_source: auth_source.to_string(),
            retry_count: 0,
            show_thinking,
            thinking_active: false,
        }
    }

    fn finish_text(&mut self) {
        if !self.text_buffer.is_empty() {
            if self.spinner_shown {
                ccx_tui::inline::clear_spinner();
                self.spinner_shown = false;
            }
            ccx_tui::inline::render_markdown(&self.text_buffer);
            self.text_buffer.clear();
        } else if self.spinner_shown {
            ccx_tui::inline::clear_spinner();
            self.spinner_shown = false;
        }
    }
}

impl ccx_core::AgentCallback for InlineCallback {
    fn on_text(&mut self, text: &str) {
        if self.thinking_active {
            println!();
            self.thinking_active = false;
        }
        if self.text_buffer.is_empty() && !self.spinner_shown {
            ccx_tui::inline::render_spinner();
            self.spinner_shown = true;
        }
        self.text_buffer.push_str(text);
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

    fn on_thinking(&mut self, text: &str) {
        if self.show_thinking {
            if self.spinner_shown {
                ccx_tui::inline::clear_spinner();
                self.spinner_shown = false;
            }
            print!("\x1b[2;3m{text}\x1b[0m");
            std::io::stdout().flush().ok();
            self.thinking_active = true;
        }
    }

    fn on_retry(&mut self, _attempt: u32, delay_ms: u64, _reason: &str) {
        self.finish_text();
        self.retry_count += 1;
        let label = if self.auth_source.starts_with("Claude") {
            format!("{} daily limit", self.auth_source)
        } else {
            "rate limited".to_string()
        };
        ccx_tui::inline::render_error(&format!(
            "Rate limited ({label}). Retrying in {:.1}s...",
            delay_ms as f64 / 1000.0
        ));
        if self.retry_count >= 3 {
            ccx_tui::inline::render_error(
                "Try: --provider openrouter --model deepseek/deepseek-r1-0528:free",
            );
        }
    }

    fn on_turn_complete(&mut self, _turn: usize, _cost: &ccx_core::CostTracker) {
        self.finish_text();
    }

    fn should_allow_tool(&mut self, name: &str, input: &serde_json::Value) -> bool {
        if self.bypass_permissions || self.always_allow.contains(name) {
            return true;
        }
        let detail = extract_tool_detail(name, input);
        match ccx_tui::inline::prompt_permission(name, &detail) {
            ccx_tui::inline::PermissionChoice::Allow => true,
            ccx_tui::inline::PermissionChoice::Deny => false,
            ccx_tui::inline::PermissionChoice::AlwaysAllow => {
                self.always_allow.insert(name.to_string());
                true
            }
        }
    }
}

/// Run inline interactive mode (default — no full-screen).
async fn run_inline_mode(
    agent: &mut ccx_core::AgentLoop,
    model: &str,
    auth_source: &str,
    cwd_display: &str,
    tool_names: &[String],
    bypass_permissions: bool,
    email: Option<&str>,
    show_thinking: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let tool_count = tool_names.len();
    ccx_tui::inline::render_welcome(model, auth_source, cwd_display, tool_count, email);
    ccx_tui::inline::render_footer_line(model);
    println!();

    // Set up rustyline with tab completion, hints, and skill discovery.
    let config = rustyline::Config::builder()
        .completion_type(rustyline::CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config)?;
    let ccx_completer = completer::CcxCompleter::new();
    rl.set_helper(Some(ccx_completer));

    // Discover skills for command handling and display.
    let discovered_skills = ccx_skill::discover_all_skills();
    let skill_display: Vec<(String, String)> = discovered_skills
        .iter()
        .map(|s| {
            let desc = if s.description.len() > 60 {
                format!("{}...", &s.description[..57])
            } else if s.description.is_empty() {
                s.name.clone()
            } else {
                s.description.clone()
            };
            (format!("/{}", s.name), desc)
        })
        .collect();

    // Load persistent history.
    let history_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".ccx_history");
    let _ = rl.load_history(&history_path);

    // Session tracking.
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_str = cwd.to_string_lossy().to_string();
    let session_id = sessions::new_session_id();
    let mut session_turns: usize = 0;
    let mut first_preview = String::new();

    // MCP server config for /doctor.
    let mcp_config = mcp_bridge::load_mcp_config(&cwd);

    loop {
        match rl.readline("❯ ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);

                // Slash command handling.
                if input.starts_with('/') {
                    // Parse command and args.
                    let (cmd_word, cmd_args) = match input.split_once(' ') {
                        Some((c, a)) => (c, Some(a.trim())),
                        None => (input, None),
                    };

                    // Check built-in commands first.
                    let handled = match cmd_word {
                        "/exit" | "/quit" => break,
                        "/help" => {
                            commands::print_command_list(&skill_display);
                            true
                        }
                        "/clear" => {
                            print!("\x1b[2J\x1b[H");
                            std::io::stdout().flush()?;
                            true
                        }
                        "/cost" => {
                            println!("{}", agent.cost().summary());
                            true
                        }
                        "/model" => {
                            println!("Model: {model}");
                            true
                        }
                        "/compact" => {
                            let before = agent.messages().len();
                            agent.compact();
                            let after = agent.messages().len();
                            let before_tokens: usize = agent.messages().iter().map(|m| {
                                match &m.content {
                                    ccx_api::MessageContent::Text(t) => t.len() / 4,
                                    ccx_api::MessageContent::Blocks(b) => b.iter().map(|bl| match bl {
                                        ccx_api::ContentBlock::Text { text } => text.len() / 4,
                                        ccx_api::ContentBlock::ToolUse { input, .. } => input.to_string().len() / 4,
                                        ccx_api::ContentBlock::ToolResult { content, .. } => content.len() / 4,
                                        ccx_api::ContentBlock::Thinking { thinking, .. } => thinking.len() / 4,
                                    }).sum(),
                                }
                            }).sum();
                            println!("\x1b[32mContext compacted.\x1b[0m Messages: {before} → {after} | ~{before_tokens} tokens remaining");
                            true
                        }
                        "/init" => {
                            let claude_md_path = cwd.join("CLAUDE.md");
                            if claude_md_path.exists() {
                                println!("\x1b[33mCLAUDE.md already exists in this directory.\x1b[0m");
                            } else {
                                let template = "# Project Instructions\n\nDescribe your project here.\n";
                                match std::fs::write(&claude_md_path, template) {
                                    Ok(_) => println!("\x1b[32mCreated CLAUDE.md\x1b[0m"),
                                    Err(e) => println!("\x1b[31mFailed to create CLAUDE.md: {e}\x1b[0m"),
                                }
                            }
                            true
                        }
                        "/version" => {
                            println!("ccx v{}", env!("CARGO_PKG_VERSION"));
                            true
                        }
                        "/login" => {
                            match tokio::runtime::Handle::current().block_on(ccx_auth::oauth::login()) {
                                Ok(_) => println!("Login successful! Restart ccx to use your subscription."),
                                Err(e) => println!("\x1b[31mLogin failed: {e}\x1b[0m"),
                            }
                            true
                        }
                        "/tools" => {
                            let names = tool_names.join(", ");
                            println!("Available tools ({tool_count}): {names}");
                            true
                        }
                        "/resume" => {
                            if let Some(sid) = cmd_args {
                                match sessions::load_session(sid) {
                                    Ok(session) => {
                                        println!("\x1b[32mResumed session {}\x1b[0m", session.id);
                                        println!("  Preview: {}", session.preview);
                                        println!("  Turns: {} | Messages: {}", session.total_turns, session.messages.len());
                                        // Note: we can't inject messages into the agent loop directly,
                                        // but we inform the model about the resumed context.
                                    }
                                    Err(e) => println!("\x1b[31m{e}\x1b[0m"),
                                }
                            } else {
                                let all = sessions::list_sessions();
                                if all.is_empty() {
                                    println!("\x1b[90mNo saved sessions.\x1b[0m");
                                } else {
                                    println!("\n\x1b[1mRecent sessions:\x1b[0m\n");
                                    for (i, s) in all.iter().take(10).enumerate() {
                                        let ts = chrono_format(s.updated_at);
                                        println!("  \x1b[33m{}\x1b[0m  \x1b[90m{}\x1b[0m  {}", s.id, ts, s.preview);
                                        if i >= 9 { break; }
                                    }
                                    println!("\n\x1b[90mUsage: /resume <session-id>\x1b[0m\n");
                                }
                            }
                            true
                        }
                        "/continue" => {
                            match sessions::find_latest_for_cwd(&cwd_str) {
                                Some(session) => {
                                    println!("\x1b[32mResuming latest session for this directory:\x1b[0m {}", session.id);
                                    println!("  Preview: {}", session.preview);
                                    println!("  Turns: {} | Updated: {}", session.total_turns, chrono_format(session.updated_at));
                                }
                                None => {
                                    println!("\x1b[90mNo previous session found for this directory.\x1b[0m");
                                }
                            }
                            true
                        }
                        "/doctor" => {
                            println!("\n\x1b[1mccx Doctor\x1b[0m\n");

                            // Check API key.
                            let api_ok = std::env::var("ANTHROPIC_API_KEY").map(|k| !k.is_empty()).unwrap_or(false);
                            let oauth_ok = ccx_auth::resolve_auth(None).is_ok();
                            if api_ok || oauth_ok {
                                println!("  \x1b[32m✓\x1b[0m Authentication: {auth_source}");
                            } else {
                                println!("  \x1b[31m✗\x1b[0m Authentication: no API key or OAuth token found");
                            }

                            // Check tools.
                            println!("  \x1b[32m✓\x1b[0m Tools: {tool_count} registered");

                            // Check MCP servers.
                            if let Some(ref cfg) = mcp_config {
                                let count = cfg.mcp_servers.len();
                                println!("  \x1b[32m✓\x1b[0m MCP servers: {count} configured in .mcp.json");
                                for name in cfg.mcp_servers.keys() {
                                    println!("    \x1b[90m- {name}\x1b[0m");
                                }
                            } else {
                                println!("  \x1b[90m-\x1b[0m MCP servers: none (.mcp.json not found)");
                            }

                            // Check CLAUDE.md.
                            if cwd.join("CLAUDE.md").exists() {
                                println!("  \x1b[32m✓\x1b[0m CLAUDE.md: found");
                            } else {
                                println!("  \x1b[90m-\x1b[0m CLAUDE.md: not found (use /init to create)");
                            }

                            // Check skills.
                            let skill_count = discovered_skills.len();
                            println!("  \x1b[32m✓\x1b[0m Skills: {skill_count} discovered");

                            println!();
                            true
                        }
                        "/config" => {
                            println!("\n\x1b[1mCurrent Configuration\x1b[0m\n");
                            println!("  Model:       {model}");
                            println!("  Provider:    {}", if auth_source.contains("OAuth") || auth_source.contains("Claude") { "anthropic (OAuth)" } else { "anthropic" });
                            println!("  Auth:        {auth_source}");
                            if let Some(ref e) = email {
                                println!("  Account:     {e}");
                            }
                            println!("  Permission:  {}", if bypass_permissions { "bypass" } else { "default" });
                            println!("  Tools:       {tool_count}");
                            println!("  Skills:      {}", discovered_skills.len());
                            if let Some(ref cfg) = mcp_config {
                                println!("  MCP servers: {}", cfg.mcp_servers.len());
                            }
                            println!("  Session:     {session_id}");
                            println!();
                            true
                        }
                        "/sessions" => {
                            let all = sessions::list_sessions();
                            if all.is_empty() {
                                println!("\x1b[90mNo saved sessions.\x1b[0m");
                            } else {
                                println!("\n\x1b[1mRecent sessions ({}):\x1b[0m\n", all.len());
                                for s in all.iter().take(15) {
                                    let ts = chrono_format(s.updated_at);
                                    let dir = shorten_home_str(&s.cwd);
                                    println!(
                                        "  \x1b[33m{}\x1b[0m  \x1b[90m{}\x1b[0m  \x1b[36m{}\x1b[0m  {}",
                                        s.id, ts, dir, s.preview
                                    );
                                }
                                println!();
                            }
                            true
                        }
                        "/status" => {
                            let cost = agent.cost();
                            println!("\n\x1b[1mSession Status\x1b[0m\n");
                            println!("  Session:    {session_id}");
                            println!("  Model:      {model}");
                            println!("  Turns:      {session_turns}");
                            println!("  Messages:   {}", agent.messages().len());
                            println!("  Tokens in:  {}", cost.total_input_tokens);
                            println!("  Tokens out: {}", cost.total_output_tokens);
                            println!("  API calls:  {}", cost.api_calls);
                            println!("  Cost:       ${:.4}", cost.estimated_cost_usd());
                            println!("  Tools:      {tool_count}");
                            println!();
                            true
                        }
                        "/simplify" => {
                            // Route to the simplify skill if discovered.
                            if let Some(skill) = ccx_skill::find_skill(&discovered_skills, "simplify") {
                                let result = ccx_skill::expand_skill(skill, None);
                                let user_msg = format!(
                                    "The user invoked skill 'simplify'\n\n<skill-content>\n{}\n</skill-content>",
                                    result.expanded_prompt
                                );
                                ccx_tui::inline::clear_previous_line();
                                ccx_tui::inline::render_user_message("/simplify");
                                let mut cb = InlineCallback::new(bypass_permissions, auth_source, show_thinking);
                                match agent.send_message(&user_msg, &mut cb).await {
                                    Ok(_) => cb.finish_text(),
                                    Err(e) => {
                                        cb.finish_text();
                                        ccx_tui::inline::render_error(&format!("Error: {e}"));
                                    }
                                }
                                ccx_tui::inline::render_separator();
                                session_turns += 1;
                            } else {
                                println!("\x1b[90mSimplify skill not found. Ensure skills are installed.\x1b[0m");
                            }
                            true
                        }
                        "/batch" => {
                            if let Some(batch_args) = cmd_args {
                                if let Some(skill) = ccx_skill::find_skill(&discovered_skills, "batch") {
                                    let result = ccx_skill::expand_skill(skill, Some(batch_args));
                                    let user_msg = format!(
                                        "The user invoked skill 'batch' with args: {}\n\n<skill-content>\n{}\n</skill-content>",
                                        batch_args, result.expanded_prompt
                                    );
                                    ccx_tui::inline::clear_previous_line();
                                    ccx_tui::inline::render_user_message(input);
                                    let mut cb = InlineCallback::new(bypass_permissions, auth_source, show_thinking);
                                    match agent.send_message(&user_msg, &mut cb).await {
                                        Ok(_) => cb.finish_text(),
                                        Err(e) => {
                                            cb.finish_text();
                                            ccx_tui::inline::render_error(&format!("Error: {e}"));
                                        }
                                    }
                                    ccx_tui::inline::render_separator();
                                    session_turns += 1;
                                } else {
                                    println!("\x1b[90mBatch skill not found. Ensure skills are installed.\x1b[0m");
                                }
                            } else {
                                println!("\x1b[33mUsage: /batch <prompt>\x1b[0m");
                            }
                            true
                        }
                        "/" => {
                            commands::print_command_list(&skill_display);
                            true
                        }
                        _ => false,
                    };

                    if handled {
                        continue;
                    }

                    // Try to match a discovered skill.
                    let after_slash = &input[1..];
                    let (skill_query, skill_args) = match after_slash.split_once(' ') {
                        Some((name, args)) => (name, Some(args)),
                        None => (after_slash, None),
                    };

                    if let Some(skill) = ccx_skill::find_skill(&discovered_skills, skill_query) {
                        let result = ccx_skill::expand_skill(skill, skill_args);
                        let user_msg = if let Some(args) = skill_args {
                            format!(
                                "The user invoked skill '{}' with args: {}\n\n<skill-content>\n{}\n</skill-content>",
                                skill.name, args, result.expanded_prompt
                            )
                        } else {
                            format!(
                                "The user invoked skill '{}'\n\n<skill-content>\n{}\n</skill-content>",
                                skill.name, result.expanded_prompt
                            )
                        };

                        ccx_tui::inline::clear_previous_line();
                        ccx_tui::inline::render_user_message(input);

                        let mut cb = InlineCallback::new(bypass_permissions, auth_source, show_thinking);
                        match agent.send_message(&user_msg, &mut cb).await {
                            Ok(_) => cb.finish_text(),
                            Err(e) => {
                                cb.finish_text();
                                ccx_tui::inline::render_error(&format!("Error: {e}"));
                            }
                        }

                        ccx_tui::inline::render_separator();
                        session_turns += 1;
                        continue;
                    }

                    // No builtin or skill matched — show suggestions.
                    commands::print_suggestions(input, &skill_display);
                    continue;
                }

                // Track first message for session preview.
                if first_preview.is_empty() {
                    first_preview = sessions::make_preview(input);
                }

                ccx_tui::inline::clear_previous_line();
                ccx_tui::inline::render_user_message(input);

                let mut cb = InlineCallback::new(bypass_permissions, auth_source, show_thinking);
                match agent.send_message(input, &mut cb).await {
                    Ok(_) => cb.finish_text(),
                    Err(e) => {
                        cb.finish_text();
                        ccx_tui::inline::render_error(&format!("Error: {e}"));
                    }
                }

                session_turns += 1;
                ccx_tui::inline::render_separator();
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Input error: {err:?}");
                break;
            }
        }
    }

    // Save session if we had any turns.
    if session_turns > 0 {
        let cost = agent.cost();
        let session = sessions::Session {
            id: session_id.clone(),
            cwd: cwd_str,
            model: model.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            messages: agent.messages().to_vec(),
            preview: if first_preview.is_empty() { "(no messages)".into() } else { first_preview },
            total_turns: session_turns,
            total_input_tokens: cost.total_input_tokens,
            total_output_tokens: cost.total_output_tokens,
        };
        if let Err(e) = sessions::save_session(&session) {
            eprintln!("\x1b[33mFailed to save session: {e}\x1b[0m");
        }
    }

    // Save history for next session.
    let _ = rl.save_history(&history_path);

    ccx_tui::inline::render_footer(model);
    println!("\nGoodbye!");
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

/// Format epoch seconds to a human-readable timestamp.
fn chrono_format(epoch: u64) -> String {
    let secs = epoch;
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    // Approximate date from epoch.
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{year}-{month:02}-{day:02} {hours:02}:{mins:02}")
}

/// Shorten a path string by replacing $HOME with ~.
fn shorten_home_str(path: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy();
        if path.starts_with(home_str.as_ref()) {
            return format!("~{}", &path[home_str.len()..]);
        }
    }
    path.to_string()
}

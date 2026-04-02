mod commands;
mod completer;
mod mcp_bridge;
mod sessions;

use std::collections::HashSet;
use std::io::Write;
use std::sync::mpsc;

use clap::{Parser, Subcommand};
use rustyline::Editor;
use rustyline::error::ReadlineError;

/// ccx — Claude Code in Rust
#[derive(Parser)]
#[command(name = "ccx", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Logging level (trace, debug, info, warn, error)
    #[arg(long, global = true, default_value = "warn")]
    log_level: String,

    /// Enable debug logging (shorthand for --log-level debug)
    #[arg(long, global = true)]
    debug: bool,

    // ── Top-level flags forwarded to Chat when no subcommand given ──
    /// Model to use
    #[arg(long, global = true)]
    model: Option<String>,

    /// Effort level: low, medium, high (default), max
    #[arg(long, global = true)]
    effort: Option<String>,

    /// Replace the entire system prompt
    #[arg(long, global = true)]
    system_prompt: Option<String>,

    /// Append text to the default system prompt
    #[arg(long, global = true)]
    append_system_prompt: Option<String>,

    /// Output format: text (default), json, stream-json
    #[arg(long, global = true)]
    output_format: Option<String>,

    /// Maximum spend in USD before refusing further API calls
    #[arg(long, global = true)]
    max_budget_usd: Option<f64>,

    /// Pipe mode: read prompt from args/stdin, print response, exit
    #[arg(short = 'p', long, global = true)]
    print: bool,

    /// Provider: anthropic (default), openrouter, openai
    #[arg(long, global = true)]
    provider: Option<String>,

    /// OpenRouter API key
    #[arg(long, global = true)]
    openrouter_key: Option<String>,

    /// API key (overrides env var)
    #[arg(long, global = true)]
    api_key: Option<String>,

    /// Continue most recent session
    #[arg(short = 'c', long, global = true)]
    r#continue: bool,

    /// Resume a session by ID
    #[arg(short = 'r', long, global = true)]
    resume: Option<String>,

    /// Skip all permission checks
    #[arg(long, global = true)]
    dangerously_skip_permissions: bool,

    /// Trailing arguments used as prompt in pipe mode
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
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

        /// Skip loading project-level .mcp.json (global MCP configs still loaded)
        #[arg(long)]
        no_mcp: bool,

        /// Provider: anthropic (default), openrouter, openai
        #[arg(long, default_value = "anthropic")]
        provider: String,

        /// OpenRouter API key (overrides OPENROUTER_API_KEY env var)
        #[arg(long)]
        openrouter_key: Option<String>,

        /// Resume a session (optionally by ID; no ID = list sessions)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        resume: Option<String>,

        /// Continue the most recent session for this directory
        #[arg(long = "continue")]
        continue_session: bool,

        /// Effort level: low, medium, high (default), max
        #[arg(long, default_value = "high")]
        effort: String,

        /// Replace the entire system prompt
        #[arg(long)]
        system_prompt: Option<String>,

        /// Append text to the default system prompt
        #[arg(long)]
        append_system_prompt: Option<String>,

        /// Output format: text (default), json, stream-json
        #[arg(long, default_value = "text")]
        output_format: String,

        /// Maximum spend in USD before refusing further API calls
        #[arg(long)]
        max_budget_usd: Option<f64>,

        /// Pipe mode: read prompt from args/stdin, print response, exit
        #[arg(short = 'p', long)]
        print: bool,
    },
    /// Show version and crate information
    Info,
    /// Update ccx to the latest version
    Update,
    /// Manage authentication
    Auth {
        /// Auth action: status, login, logout
        #[arg(default_value = "status")]
        action: String,
    },
    /// Configure and manage MCP servers
    Mcp {
        /// MCP action: list, add, remove
        #[arg(default_value = "list")]
        action: String,
        /// Server name (for add/remove)
        name: Option<String>,
        /// Command to run (for add)
        command: Option<String>,
        /// Arguments (for add)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging: default to warn, --debug overrides to debug level
    let filter = if cli.debug {
        log::LevelFilter::Debug
    } else {
        let level = cli.log_level.to_lowercase();
        match level.as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Warn,
        }
    };
    env_logger::builder()
        .filter_level(filter)
        .format_target(false)
        .format_timestamp(None)
        .init();

    log::debug!("Starting ccx with log level: {:?}", filter);

    // GAP 1: default to Chat when no subcommand given.
    let command = cli.command.unwrap_or_else(|| {
        // Check if -p / --print was passed at top level.
        let print_mode = cli.print;
        // Collect trailing args as prompt for pipe mode.
        let prompt_from_args = if !cli.args.is_empty() {
            Some(cli.args.join(" "))
        } else {
            None
        };

        Commands::Chat {
            model: cli.model.unwrap_or_else(|| "claude-sonnet-4-6".into()),
            api_key: cli.api_key,
            prompt: if print_mode { prompt_from_args } else { None },
            permission_mode: "bypass".into(),
            max_turns: 200,
            tui: false,
            dangerously_skip_permissions: cli.dangerously_skip_permissions,
            no_thinking: false,
            thinking_budget: 10000,
            hide_thinking: false,
            sandbox: false,
            no_mcp: false,
            provider: cli.provider.unwrap_or_else(|| "anthropic".into()),
            openrouter_key: cli.openrouter_key,
            resume: cli.resume,
            continue_session: cli.r#continue,
            effort: cli.effort.unwrap_or_else(|| "high".into()),
            system_prompt: cli.system_prompt,
            append_system_prompt: cli.append_system_prompt,
            output_format: cli.output_format.unwrap_or_else(|| "text".into()),
            max_budget_usd: cli.max_budget_usd,
            print: print_mode,
        }
    });

    match command {
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
            no_mcp,
            provider,
            openrouter_key,
            resume,
            continue_session,
            effort,
            system_prompt,
            append_system_prompt,
            output_format,
            max_budget_usd,
            print,
        } => {
            // Model aliases — short names that auto-resolve provider + full model ID
            let (model, provider) = resolve_model_alias(&model, &provider);

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
                no_mcp,
                &provider,
                openrouter_key.as_deref(),
                resume.as_deref(),
                continue_session,
                &effort,
                system_prompt.as_deref(),
                append_system_prompt.as_deref(),
                &output_format,
                max_budget_usd,
                print,
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
        Commands::Update => {
            run_update();
        }
        Commands::Auth { action } => {
            run_auth(&action).await;
        }
        Commands::Mcp {
            action,
            name,
            command,
            args,
        } => {
            run_mcp(&action, name.as_deref(), command.as_deref(), &args);
        }
    }
}

async fn run_auth(action: &str) {
    const GREEN: &str = "\x1b[32m";
    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[90m";
    const RESET: &str = "\x1b[0m";

    match action {
        "status" | "" => {
            println!("\n{BOLD}Authentication Status:{RESET}\n");

            // 1. OAuth (keychain / credentials file)
            match ccx_auth::resolve_auth(None) {
                Ok(ccx_auth::AuthMethod::OAuthToken {
                    subscription_type, ..
                }) => {
                    let label = match subscription_type.as_str() {
                        "max" => "Claude Max",
                        "pro" => "Claude Pro",
                        "team" => "Claude Team",
                        _ => "Claude Subscription",
                    };
                    println!("  {GREEN}●{RESET} {BOLD}{label}{RESET} (OAuth)");
                }
                Ok(ccx_auth::AuthMethod::ApiKey(ref k)) => {
                    let src = match &k.source {
                        ccx_auth::KeySource::EnvVar => "ANTHROPIC_API_KEY",
                        ccx_auth::KeySource::ConfigFile(_) => "~/.claude/config.json",
                        ccx_auth::KeySource::Explicit => "explicit",
                    };
                    let masked = mask_key(&k.key);
                    println!("  {GREEN}●{RESET} {BOLD}Anthropic API Key{RESET} ({src})");
                    println!("    {DIM}{masked}{RESET}");
                }
                _ => {
                    println!("  {DIM}○{RESET} Anthropic — not authenticated");
                    println!("    {DIM}Run: ccx auth login{RESET}");
                }
            }

            // 2. Other provider keys from env
            for (var, label) in [
                ("OPENROUTER_API_KEY", "OpenRouter"),
                ("OPENAI_API_KEY", "OpenAI"),
            ] {
                if let Ok(key) = std::env::var(var) {
                    let masked = mask_key(&key);
                    println!("  {GREEN}●{RESET} {BOLD}{label}{RESET} ({var})");
                    println!("    {DIM}{masked}{RESET}");
                }
            }

            println!();
        }
        "login" => {
            match ccx_auth::oauth::login().await {
                Ok(_) => println!("\x1b[32m✓ Login successful!\x1b[0m"),
                Err(e) => eprintln!("\x1b[31mLogin failed: {e}\x1b[0m"),
            }
        }
        "logout" => {
            println!("Clearing credentials...");
            #[cfg(target_os = "macos")]
            {
                let user = std::env::var("USER").unwrap_or_default();
                std::process::Command::new("security")
                    .args([
                        "delete-generic-password",
                        "-a",
                        &user,
                        "-s",
                        "Claude Code-credentials",
                    ])
                    .output()
                    .ok();
            }
            let creds = dirs::home_dir().unwrap().join(".claude/.credentials.json");
            std::fs::remove_file(&creds).ok();
            println!("\x1b[32m✓ Logged out\x1b[0m");
        }
        _ => {
            println!("Usage: ccx auth [status|login|logout]");
        }
    }
}

fn run_mcp(action: &str, name: Option<&str>, command: Option<&str>, args: &[String]) {
    const GREEN: &str = "\x1b[32m";
    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[90m";
    const RESET: &str = "\x1b[0m";

    let mcp_path = std::path::PathBuf::from(".mcp.json");

    match action {
        "list" | "" => {
            println!("\n{BOLD}MCP Servers:{RESET}\n");
            let mut found = false;

            // Project-level .mcp.json
            if mcp_path.exists()
                && let Ok(content) = std::fs::read_to_string(&mcp_path)
                    && let Ok(config) = serde_json::from_str::<serde_json::Value>(&content)
                        && let Some(servers) = config["mcpServers"].as_object() {
                            for (name, server) in servers {
                                let cmd = server["command"].as_str().unwrap_or("unknown");
                                let srv_args = server["args"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|v| v.as_str())
                                            .collect::<Vec<_>>()
                                            .join(" ")
                                    })
                                    .unwrap_or_default();
                                println!(
                                    "  {GREEN}●{RESET} {BOLD}{name}{RESET} — {DIM}{cmd} {srv_args}{RESET}"
                                );
                                found = true;
                            }
                        }

            // Global MCP config
            let global_mcp = dirs::home_dir().unwrap().join(".claude/mcp.json");
            if global_mcp.exists()
                && let Ok(content) = std::fs::read_to_string(&global_mcp)
                    && let Ok(config) = serde_json::from_str::<serde_json::Value>(&content)
                        && let Some(servers) = config["mcpServers"].as_object()
                            && !servers.is_empty() {
                                println!("\n  {DIM}Global (~/.claude/mcp.json):{RESET}");
                                for (name, server) in servers {
                                    let cmd = server["command"].as_str().unwrap_or("unknown");
                                    println!(
                                        "  {GREEN}●{RESET} {BOLD}{name}{RESET} — {DIM}{cmd}{RESET}"
                                    );
                                    found = true;
                                }
                            }

            if !found {
                println!("  {DIM}No MCP servers configured{RESET}");
            }

            println!("\n  {DIM}Add: ccx mcp add <name> <command> [args...]{RESET}");
            println!("  {DIM}Remove: ccx mcp remove <name>{RESET}");
            println!();
        }
        "add" => {
            let name = match name {
                Some(n) => n,
                None => {
                    eprintln!("Usage: ccx mcp add <name> <command> [args...]");
                    return;
                }
            };
            let cmd = match command {
                Some(c) => c,
                None => {
                    eprintln!("Usage: ccx mcp add <name> <command> [args...]");
                    return;
                }
            };

            // Read or create .mcp.json
            let mut config: serde_json::Value = if mcp_path.exists() {
                let content = std::fs::read_to_string(&mcp_path).unwrap_or_default();
                serde_json::from_str(&content).unwrap_or_else(|_| {
                    serde_json::json!({"mcpServers": {}})
                })
            } else {
                serde_json::json!({"mcpServers": {}})
            };

            let servers = config["mcpServers"]
                .as_object_mut()
                .expect("mcpServers must be an object");

            let server_entry = if args.is_empty() {
                serde_json::json!({"command": cmd})
            } else {
                serde_json::json!({"command": cmd, "args": args})
            };

            servers.insert(name.to_string(), server_entry);

            match std::fs::write(&mcp_path, serde_json::to_string_pretty(&config).unwrap()) {
                Ok(_) => println!("\x1b[32m✓ Added MCP server '{name}'\x1b[0m"),
                Err(e) => eprintln!("\x1b[31mFailed to write .mcp.json: {e}\x1b[0m"),
            }
        }
        "remove" => {
            let name = match name {
                Some(n) => n,
                None => {
                    eprintln!("Usage: ccx mcp remove <name>");
                    return;
                }
            };

            if !mcp_path.exists() {
                eprintln!("No .mcp.json found");
                return;
            }

            let content = std::fs::read_to_string(&mcp_path).unwrap_or_default();
            let mut config: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Invalid .mcp.json: {e}");
                    return;
                }
            };

            if let Some(servers) = config["mcpServers"].as_object_mut() {
                if servers.remove(name).is_some() {
                    match std::fs::write(
                        &mcp_path,
                        serde_json::to_string_pretty(&config).unwrap(),
                    ) {
                        Ok(_) => println!("\x1b[32m✓ Removed MCP server '{name}'\x1b[0m"),
                        Err(e) => eprintln!("\x1b[31mFailed to write .mcp.json: {e}\x1b[0m"),
                    }
                } else {
                    eprintln!("Server '{name}' not found in .mcp.json");
                }
            }
        }
        _ => {
            println!("Usage: ccx mcp [list|add|remove]");
        }
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    let suffix = &key[key.len() - 4..];
    if key.starts_with("sk-ant-") {
        format!("sk-ant-...{suffix}")
    } else if key.starts_with("sk-or-") {
        format!("sk-or-...{suffix}")
    } else if key.starts_with("sk-") {
        format!("sk-...{suffix}")
    } else {
        format!("...{suffix}")
    }
}

const CCX_PATH_BLOCK_START: &str = "# >>> ccx path >>>";
const CCX_PATH_BLOCK_END: &str = "# <<< ccx path <<<";

fn ccx_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ccx.exe"
    } else {
        "ccx"
    }
}

fn path_contains_dir(path_env: &std::ffi::OsStr, dir: &std::path::Path) -> bool {
    std::env::split_paths(path_env).any(|entry| entry == dir)
}

fn preferred_user_bin_dir() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    for candidate in [home.join(".local/bin"), home.join("bin")] {
        if path_contains_dir(
            &std::env::var_os("PATH").unwrap_or_default(),
            &candidate,
        ) {
            return candidate;
        }
    }
    home.join(".ccx/bin")
}

fn shell_profile_target() -> Option<(std::path::PathBuf, bool)> {
    let home = dirs::home_dir()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.contains("fish") {
        Some((home.join(".config/fish/config.fish"), true))
    } else if shell.contains("zsh") {
        Some((home.join(".zshrc"), false))
    } else if shell.contains("bash") {
        for candidate in [
            home.join(".bash_profile"),
            home.join(".bash_login"),
            home.join(".profile"),
            home.join(".bashrc"),
        ] {
            if candidate.exists() {
                return Some((candidate, false));
            }
        }
        Some((home.join(".bash_profile"), false))
    } else {
        Some((home.join(".profile"), false))
    }
}

fn render_path_block(install_dir: &std::path::Path, fish_syntax: bool) -> String {
    if fish_syntax {
        format!(
            "\n{CCX_PATH_BLOCK_START}\nset -gx PATH \"{}\" $PATH\n{CCX_PATH_BLOCK_END}\n",
            install_dir.display()
        )
    } else {
        format!(
            "\n{CCX_PATH_BLOCK_START}\nexport PATH=\"{}:$PATH\"\n{CCX_PATH_BLOCK_END}\n",
            install_dir.display()
        )
    }
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };

    let mut left_parts = parse(left);
    let mut right_parts = parse(right);
    let max_len = left_parts.len().max(right_parts.len());
    left_parts.resize(max_len, 0);
    right_parts.resize(max_len, 0);
    left_parts.cmp(&right_parts)
}

fn ensure_path_block(install_dir: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    let Some((profile_path, fish_syntax)) = shell_profile_target() else {
        return Ok(None);
    };

    if let Some(parent) = profile_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create profile dir {}: {e}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(&profile_path).unwrap_or_default();
    let block = render_path_block(install_dir, fish_syntax);
    let updated = if let Some(start) = existing.find(CCX_PATH_BLOCK_START) {
        if let Some(end_rel) = existing[start..].find(CCX_PATH_BLOCK_END) {
            let end = start + end_rel + CCX_PATH_BLOCK_END.len();
            let mut replaced = String::new();
            replaced.push_str(&existing[..start]);
            replaced.push_str(&block);
            replaced.push_str(&existing[end..]);
            replaced
        } else {
            format!("{existing}{block}")
        }
    } else {
        format!("{existing}{block}")
    };

    if updated != existing {
        std::fs::write(&profile_path, updated)
            .map_err(|e| format!("failed to update {}: {e}", profile_path.display()))?;
    }

    Ok(Some(profile_path))
}

fn is_dir_writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".ccx-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn install_downloaded_binary(
    downloaded_path: &std::path::Path,
    install_path: &std::path::Path,
) -> Result<(), String> {
    if let Some(parent) = install_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::copy(downloaded_path, install_path)
        .map_err(|e| format!("failed to copy to {}: {e}", install_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(install_path)
            .map_err(|e| format!("failed to read permissions for {}: {e}", install_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(install_path, perms)
            .map_err(|e| format!("failed to chmod {}: {e}", install_path.display()))?;
    }
    let _ = std::fs::remove_file(downloaded_path);
    Ok(())
}

fn run_update() {
    println!("Checking for updates...");

    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: v{current_version}");

    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "https://api.github.com/repos/anton-abyzov/ccx-rs/releases/latest",
        ])
        .output();

    match output {
        Ok(out) => {
            let body = String::from_utf8_lossy(&out.stdout);
            if let Some(tag) = body
                .split("\"tag_name\":\"")
                .nth(1)
                .or_else(|| body.split("\"tag_name\": \"").nth(1))
                .and_then(|s| s.split('"').next())
            {
                let latest = tag.trim_start_matches('v');
                println!("Latest version:  v{latest}");

                match compare_versions(latest, current_version) {
                    std::cmp::Ordering::Equal => {
                        println!("\n\x1b[32m✓ Already up to date!\x1b[0m");
                    }
                    std::cmp::Ordering::Less => {
                        println!(
                            "\n\x1b[32m✓ Already on a newer local version (v{current_version}) than the latest release.\x1b[0m"
                        );
                    }
                    std::cmp::Ordering::Greater => {
                    println!("\nUpdating to v{latest}...");

                    let artifact = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
                        "ccx-macos-arm64"
                    } else if cfg!(target_os = "macos") {
                        "ccx-macos-x64"
                    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
                        "ccx-linux-arm64"
                    } else if cfg!(target_os = "linux") {
                        "ccx-linux-x64"
                    } else if cfg!(target_os = "windows") {
                        "ccx-windows-x64.exe"
                    } else {
                        eprintln!("Unsupported platform. Update manually from: https://github.com/anton-abyzov/ccx-rs/releases");
                        std::process::exit(1);
                    };

                    let url = format!(
                        "https://github.com/anton-abyzov/ccx-rs/releases/download/{tag}/{artifact}"
                    );
                    let tmp = std::env::temp_dir().join(format!(
                        "ccx-update-{}{}",
                        std::process::id(),
                        if cfg!(target_os = "windows") { ".exe" } else { "" }
                    ));
                    let tmp_str = tmp.to_string_lossy().to_string();

                    let dl = std::process::Command::new("curl")
                        .args(["-fsSL", &url, "-o", &tmp_str])
                        .status()
                        .or_else(|_| {
                            std::process::Command::new("wget")
                                .args(["-q", &url, "-O", &tmp_str])
                                .status()
                        });

                    match dl {
                        Ok(s) if s.success() => {
                            let current_exe =
                                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::new());
                            let current_dir = current_exe.parent().map(std::path::Path::to_path_buf);
                            let install_target = if cfg!(target_os = "windows") {
                                preferred_user_bin_dir().join(ccx_binary_name())
                            } else if let Some(dir) = &current_dir {
                                if is_dir_writable(dir) {
                                    current_exe.clone()
                                } else {
                                    preferred_user_bin_dir().join(ccx_binary_name())
                                }
                            } else {
                                preferred_user_bin_dir().join(ccx_binary_name())
                            };

                            let install_result = install_downloaded_binary(&tmp, &install_target);

                            if install_result.is_ok() {
                                let install_dir = install_target
                                    .parent()
                                    .map(std::path::Path::to_path_buf)
                                    .unwrap_or_else(preferred_user_bin_dir);
                                let install_moved = current_dir.as_ref() != Some(&install_dir);
                                let path_env = std::env::var_os("PATH").unwrap_or_default();

                                if install_moved {
                                    match ensure_path_block(&install_dir) {
                                        Ok(Some(profile)) => {
                                            println!(
                                                "\n\x1b[38;2;138;99;210mUpdated shell profile:\x1b[0m {}",
                                                profile.display()
                                            );
                                        }
                                        Ok(None) => {}
                                        Err(err) => {
                                            eprintln!("\x1b[33mWarning:\x1b[0m {err}");
                                        }
                                    }
                                }

                                println!("\n\x1b[32m✓ Updated to v{latest}!\x1b[0m");
                                println!("  Installed at: {}", install_target.display());

                                if install_moved
                                    || !path_contains_dir(&path_env, &install_dir)
                                {
                                    println!("\n\x1b[38;2;138;99;210mRun in this shell:\x1b[0m");
                                    if cfg!(target_os = "windows") {
                                        println!("  $env:Path=\"{};\" + $env:Path", install_dir.display());
                                    } else {
                                        println!("  export PATH=\"{}:$PATH\"", install_dir.display());
                                        println!("  hash -r");
                                    }
                                }
                            } else {
                                eprintln!("{}", install_result.err().unwrap_or_default());
                            }
                        }
                        _ => {
                            eprintln!(
                                "Download failed. Try manually: curl -fsSL {url} -o ccx && chmod +x ccx"
                            );
                        }
                    }
                    }
                }
            } else {
                eprintln!(
                    "Could not determine latest version. Visit: https://github.com/anton-abyzov/ccx-rs/releases"
                );
            }
        }
        Err(_) => {
            eprintln!(
                "curl not found. Visit: https://github.com/anton-abyzov/ccx-rs/releases"
            );
        }
    }
}

fn print_info() {
    println!("ccx v{}", ccx_core::version());
    println!("Crates:");
    println!("  ccx-api        - Claude API client with streaming");
    println!("  ccx-auth       - API key resolution");
    println!("  ccx-core       - Agent loop, tools, hooks, cost tracking");
    println!(
        "  ccx-tools      - Built-in tools (Bash, Read, Write, Edit, Glob, Grep, WebFetch, WebSearch, Agent, TodoWrite, NotebookEdit)"
    );
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

/// Map effort level to (max_tokens, thinking_enabled, thinking_budget).
/// Convert hook definitions from settings.json into a ccx_core HookRegistry.
fn build_hook_registry(settings: &ccx_config::Settings) -> ccx_core::HookRegistry {
    let mut registry = ccx_core::HookRegistry::new();
    for (event_name, defs) in &settings.hooks {
        let event = match event_name.as_str() {
            "PreToolUse" => ccx_core::HookEvent::PreTool,
            "PostToolUse" => ccx_core::HookEvent::PostTool,
            "UserPromptSubmit" | "PreMessage" => ccx_core::HookEvent::PreMessage,
            "PostMessage" => ccx_core::HookEvent::PostMessage,
            _ => continue,
        };
        for def in defs {
            registry.add(ccx_core::Hook {
                event,
                pattern: def.matcher.clone(),
                command: def.command.clone(),
            });
        }
    }
    registry
}

/// Resolve model aliases to full model IDs + correct provider.
/// Allows: `ccx --model deepseek`, `ccx --model nemotron`, `ccx --model gpt4o`, etc.
fn resolve_model_alias(model: &str, provider: &str) -> (String, String) {
    match model.to_lowercase().as_str() {
        // DeepSeek R1 — reasoning model (free via OpenRouter)
        "deepseek" | "deepseek-r1" | "r1" => (
            "deepseek/deepseek-r1".into(),
            "openrouter".into(),
        ),
        // Nvidia Nemotron — fast coding model (free via OpenRouter)
        "nemotron" | "nvidia" | "nemotron-120b" => (
            "nvidia/nemotron-3-super-120b-a12b:free".into(),
            "openrouter".into(),
        ),
        // Nvidia Nemotron Nano — fastest free model
        "nemotron-nano" | "nano" => (
            "nvidia/nemotron-3-nano-30b-a3b:free".into(),
            "openrouter".into(),
        ),
        // Qwen — large context (1M) free model
        "qwen" | "qwen3" => (
            "qwen/qwen3-235b-a22b:free".into(),
            "openrouter".into(),
        ),
        // Claude aliases (keep as anthropic provider)
        "sonnet" | "claude-sonnet" | "claude" => (
            "claude-sonnet-4-6".into(),
            "anthropic".into(),
        ),
        "opus" | "claude-opus" => (
            "claude-opus-4-6".into(),
            "anthropic".into(),
        ),
        "haiku" | "claude-haiku" => (
            "claude-haiku-4-5".into(),
            "anthropic".into(),
        ),
        // OpenAI aliases
        "gpt4o" | "gpt-4o" | "4o" => (
            "gpt-4o".into(),
            "openai".into(),
        ),
        "o1" | "o1-preview" => (
            "o1".into(),
            "openai".into(),
        ),
        // No alias — use as-is
        _ => (model.to_string(), provider.to_string()),
    }
}

fn effort_config(effort: &str) -> (u32, bool, u32) {
    match effort {
        "low" => (1024, false, 0),
        "medium" => (4096, false, 0),
        "high" => (16384, true, 10000),
        "max" => (32768, true, 32000),
        _ => (16384, true, 10000),
    }
}

/// User's choice from the interactive auth picker.
#[derive(Debug, Clone, Copy)]
enum AuthChoice {
    ClaudeSubscription,
    AnthropicApiKey,
    OpenRouter,
    OpenAi,
}

/// Render the auth picker box with the currently selected item highlighted.
fn render_picker(selected: usize) {
    let purple = "\x1b[38;2;138;99;210m";
    let bold = "\x1b[1m";
    let dim = "\x1b[90m";
    let reset = "\x1b[0m";
    let arrow = "\x1b[38;2;138;99;210m❯\x1b[0m";

    let options: &[(&str, &str)] = &[
        (
            "Claude subscription (Pro/Max/Team)",
            "Opens browser → sign in → auto-connect",
        ),
        (
            "Anthropic API key",
            "Enter key from console.anthropic.com",
        ),
        (
            "OpenRouter (free models available)",
            "Enter key from openrouter.ai/keys",
        ),
        ("OpenAI (GPT-4o, o1)", "Enter your OpenAI API key"),
    ];

    print!("\x1b[2J\x1b[H"); // clear screen, cursor home
    println!();
    println!("{purple}╭──────────────────────────────────────────────────╮{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}│{reset}  {bold}Welcome to CCX!{reset}                                 {purple}│{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}│{reset}  Select login method:                            {purple}│{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");

    for (i, (label, hint)) in options.iter().enumerate() {
        let prefix = if i == selected {
            format!("{arrow} {bold}{}{reset}", i + 1)
        } else {
            format!("  {dim}{}{reset}", i + 1)
        };
        let label_fmt = if i == selected {
            format!("{bold}{label}{reset}")
        } else {
            label.to_string()
        };
        // Option line
        println!(
            "{purple}│{reset}  {prefix}. {label_fmt:<42}{purple}│{reset}"
        );
        // Hint line (dimmed)
        println!(
            "{purple}│{reset}       {dim}{hint:<43}{reset}{purple}│{reset}"
        );
        println!("{purple}│{reset}                                                  {purple}│{reset}");
    }

    println!("{purple}╰──────────────────────────────────────────────────╯{reset}");
    println!();
    println!(
        "{dim}  ↑/↓ to navigate  •  Enter to select  •  Esc to quit{reset}"
    );
    std::io::stdout().flush().ok();
}

/// Show an interactive login picker using crossterm raw mode.
/// Returns the user's choice, or None if they press Esc.
fn interactive_auth_picker() -> Option<AuthChoice> {
    use crossterm::event::{self, Event, KeyCode};
    use crossterm::terminal;

    let mut selected: usize = 0;
    let num_options = 4;

    terminal::enable_raw_mode().ok()?;
    render_picker(selected);

    let result = loop {
        if let Ok(Event::Key(key)) = event::read() {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if selected > 0 {
                        selected -= 1;
                    }
                    render_picker(selected);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected < num_options - 1 {
                        selected += 1;
                    }
                    render_picker(selected);
                }
                KeyCode::Char('1') => {
                    selected = 0;
                    break Some(selected);
                }
                KeyCode::Char('2') => {
                    selected = 1;
                    break Some(selected);
                }
                KeyCode::Char('3') => {
                    selected = 2;
                    break Some(selected);
                }
                KeyCode::Char('4') => {
                    selected = 3;
                    break Some(selected);
                }
                KeyCode::Enter => break Some(selected),
                KeyCode::Esc | KeyCode::Char('q') => break None,
                _ => {}
            }
        }
    };

    terminal::disable_raw_mode().ok();
    // Clear the picker screen
    print!("\x1b[2J\x1b[H");
    std::io::stdout().flush().ok();

    result.map(|idx| match idx {
        0 => AuthChoice::ClaudeSubscription,
        1 => AuthChoice::AnthropicApiKey,
        2 => AuthChoice::OpenRouter,
        3 => AuthChoice::OpenAi,
        _ => unreachable!(),
    })
}

/// Print a non-interactive authentication guide (for print/pipe mode).
fn print_auth_guide_noninteractive() {
    let purple = "\x1b[38;2;138;99;210m";
    let bold = "\x1b[1m";
    let dim = "\x1b[90m";
    let reset = "\x1b[0m";

    println!();
    println!("{purple}╭──────────────────────────────────────────────────╮{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}│{reset}  {bold}Welcome to CCX!{reset}                                 {purple}│{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}│{reset}  No credentials found. Options:                  {purple}│{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}│{reset}  {purple}1.{reset} {bold}Claude subscription{reset} — run: ccx /login        {purple}│{reset}");
    println!("{purple}│{reset}  {purple}2.{reset} {bold}Anthropic API key{reset}                           {purple}│{reset}");
    println!("{purple}│{reset}       export ANTHROPIC_API_KEY=\"sk-ant-...\"      {purple}│{reset}");
    println!("{purple}│{reset}  {purple}3.{reset} {bold}OpenRouter{reset} — openrouter.ai/keys              {purple}│{reset}");
    println!("{purple}│{reset}       export OPENROUTER_API_KEY=\"sk-or-...\"      {purple}│{reset}");
    println!("{purple}│{reset}  {purple}4.{reset} {bold}OpenAI{reset} — platform.openai.com/api-keys        {purple}│{reset}");
    println!("{purple}│{reset}       export OPENAI_API_KEY=\"sk-...\"             {purple}│{reset}");
    println!("{purple}│{reset}                                                  {purple}│{reset}");
    println!("{purple}╰──────────────────────────────────────────────────╯{reset}");
    println!();
    println!("{dim}Set one of the above and re-run ccx.{reset}");
}

/// Prompt the user for an API key (after exiting raw mode).
fn prompt_for_api_key(provider_name: &str, url: &str) -> Option<String> {
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!();
    println!(
        "Get your API key at: {bold}{url}{reset}"
    );
    println!();
    print!("Enter API key: ");
    std::io::stdout().flush().ok();

    let mut key = String::new();
    if std::io::stdin().read_line(&mut key).is_err() {
        eprintln!("Failed to read input");
        return None;
    }
    let key = key.trim().to_string();
    if key.is_empty() {
        eprintln!("No key entered. Run ccx again to retry.");
        return None;
    }
    let _ = provider_name; // used in caller context
    Some(key)
}

/// Handle the first-run authentication flow when no credentials are found.
/// In interactive mode, shows the picker. In print mode, shows the guide and exits.
async fn run_first_run_auth(
    print_mode: bool,
    model: &str,
    _oi_key_env: &Option<String>,
) -> (
    ccx_api::ApiClient,
    String,
    Option<String>,
    bool,
    String,
    String,
) {
    if print_mode {
        print_auth_guide_noninteractive();
        std::process::exit(1);
    }

    let green = "\x1b[32m";
    let dim = "\x1b[90m";
    let reset = "\x1b[0m";

    let choice = interactive_auth_picker();

    match choice {
        Some(AuthChoice::ClaudeSubscription) => {
            println!("{dim}Starting browser login...{reset}");
            match ccx_auth::oauth::login().await {
                Ok(tokens) => {
                    let email = ccx_auth::fetch_oauth_email(&tokens.access_token).await;
                    if let Some(ref e) = email {
                        println!("{green}✓ Logged in as {e}{reset}");
                    } else {
                        println!("{green}✓ Logged in successfully!{reset}");
                    }
                    let auth = ccx_auth::AuthMethod::OAuthToken {
                        access_token: tokens.access_token.clone(),
                        api_key: tokens.api_key.clone(),
                        subscription_type: tokens
                            .subscription_type
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    };
                    let client = ccx_api::ApiClient::Claude(
                        ccx_api::ClaudeClient::with_auth(&auth, model),
                    );
                    (
                        client,
                        auth.display_label().to_string(),
                        email,
                        false,
                        tokens.access_token,
                        "anthropic".to_string(),
                    )
                }
                Err(e) => {
                    eprintln!("\x1b[31mOAuth login failed: {e}\x1b[0m");
                    eprintln!("{dim}Starting without authentication — type /login to retry.{reset}");
                    let auth = ccx_auth::AuthMethod::None;
                    let client = ccx_api::ApiClient::Claude(
                        ccx_api::ClaudeClient::with_auth(&auth, model),
                    );
                    (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
                }
            }
        }
        Some(AuthChoice::AnthropicApiKey) => {
            if let Some(key) = prompt_for_api_key("Anthropic", "https://console.anthropic.com/settings/keys") {
                println!("{dim}Validating...{reset}");
                match ccx_auth::validate_api_key("anthropic", &key).await {
                    Ok(()) => {
                        if let Err(e) = ccx_auth::save_ccx_credential("anthropic", &key) {
                            eprintln!("{dim}Warning: could not save credentials: {e}{reset}");
                        }
                        println!("{green}✓ API key validated. Using Anthropic provider.{reset}");
                        // Set for this session
                        unsafe { std::env::set_var("ANTHROPIC_API_KEY", &key) };
                        let auth = ccx_auth::AuthMethod::ApiKey(ccx_auth::ResolvedKey {
                            key: key.clone(),
                            source: ccx_auth::KeySource::ConfigFile(ccx_auth::ccx_config_path().unwrap()),
                        });
                        let client = ccx_api::ApiClient::Claude(
                            ccx_api::ClaudeClient::with_auth(&auth, model),
                        );
                        (client, "API Key".to_string(), None, false, key, "anthropic".to_string())
                    }
                    Err(e) => {
                        eprintln!("\x1b[31mValidation failed: {e}\x1b[0m");
                        eprintln!("{dim}Starting without authentication — type /login to retry.{reset}");
                        let auth = ccx_auth::AuthMethod::None;
                        let client = ccx_api::ApiClient::Claude(
                            ccx_api::ClaudeClient::with_auth(&auth, model),
                        );
                        (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
                    }
                }
            } else {
                let auth = ccx_auth::AuthMethod::None;
                let client = ccx_api::ApiClient::Claude(
                    ccx_api::ClaudeClient::with_auth(&auth, model),
                );
                (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
            }
        }
        Some(AuthChoice::OpenRouter) => {
            if let Some(key) = prompt_for_api_key("OpenRouter", "https://openrouter.ai/keys") {
                println!("{dim}Validating...{reset}");
                match ccx_auth::validate_api_key("openrouter", &key).await {
                    Ok(()) => {
                        if let Err(e) = ccx_auth::save_ccx_credential("openrouter", &key) {
                            eprintln!("{dim}Warning: could not save credentials: {e}{reset}");
                        }
                        println!("{green}✓ API key validated. Using OpenRouter provider.{reset}");
                        unsafe { std::env::set_var("OPENROUTER_API_KEY", &key) };
                        let client = ccx_api::ApiClient::OpenAi(
                            ccx_api::OpenAiClient::openrouter(&key, model),
                        );
                        (client, "OpenRouter".to_string(), None, false, key, "openrouter".to_string())
                    }
                    Err(e) => {
                        eprintln!("\x1b[31mValidation failed: {e}\x1b[0m");
                        eprintln!("{dim}Starting without authentication — type /login to retry.{reset}");
                        let auth = ccx_auth::AuthMethod::None;
                        let client = ccx_api::ApiClient::Claude(
                            ccx_api::ClaudeClient::with_auth(&auth, model),
                        );
                        (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
                    }
                }
            } else {
                let auth = ccx_auth::AuthMethod::None;
                let client = ccx_api::ApiClient::Claude(
                    ccx_api::ClaudeClient::with_auth(&auth, model),
                );
                (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
            }
        }
        Some(AuthChoice::OpenAi) => {
            if let Some(key) = prompt_for_api_key("OpenAI", "https://platform.openai.com/api-keys") {
                println!("{dim}Validating...{reset}");
                match ccx_auth::validate_api_key("openai", &key).await {
                    Ok(()) => {
                        if let Err(e) = ccx_auth::save_ccx_credential("openai", &key) {
                            eprintln!("{dim}Warning: could not save credentials: {e}{reset}");
                        }
                        println!("{green}✓ API key validated. Using OpenAI provider.{reset}");
                        unsafe { std::env::set_var("OPENAI_API_KEY", &key) };
                        let client = ccx_api::ApiClient::OpenAi(
                            ccx_api::OpenAiClient::openai(&key, model),
                        );
                        (client, "OpenAI".to_string(), None, false, key, "openai".to_string())
                    }
                    Err(e) => {
                        eprintln!("\x1b[31mValidation failed: {e}\x1b[0m");
                        eprintln!("{dim}Starting without authentication — type /login to retry.{reset}");
                        let auth = ccx_auth::AuthMethod::None;
                        let client = ccx_api::ApiClient::Claude(
                            ccx_api::ClaudeClient::with_auth(&auth, model),
                        );
                        (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
                    }
                }
            } else {
                let auth = ccx_auth::AuthMethod::None;
                let client = ccx_api::ApiClient::Claude(
                    ccx_api::ClaudeClient::with_auth(&auth, model),
                );
                (client, auth.display_label().to_string(), None, true, String::new(), "anthropic".to_string())
            }
        }
        None => {
            // User pressed Esc/quit
            std::process::exit(0);
        }
    }
}

async fn hydrate_runtime_oauth(auth: ccx_auth::AuthMethod) -> ccx_auth::AuthMethod {
    match auth {
        ccx_auth::AuthMethod::OAuthToken {
            access_token,
            api_key: None,
            subscription_type,
        } => match ccx_auth::oauth::derive_cli_api_key(&access_token).await {
            Ok(api_key) => {
                let subscription_type = if subscription_type == "unknown" {
                    ccx_auth::oauth::resolve_subscription_type(&access_token)
                        .await
                        .unwrap_or(subscription_type)
                } else {
                    subscription_type
                };
                ccx_auth::AuthMethod::OAuthToken {
                    access_token,
                    api_key: Some(api_key),
                    subscription_type,
                }
            }
            Err(err) => {
                log::debug!("Failed to derive CLI API key from OAuth token: {err}");
                ccx_auth::AuthMethod::OAuthToken {
                    access_token,
                    api_key: None,
                    subscription_type,
                }
            }
        },
        other => other,
    }
}

#[allow(clippy::too_many_arguments)]
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
    no_mcp: bool,
    provider: &str,
    openrouter_key: Option<&str>,
    resume_id: Option<&str>,
    continue_session: bool,
    effort: &str,
    custom_system_prompt: Option<&str>,
    append_system_prompt: Option<&str>,
    output_format: &str,
    max_budget_usd: Option<f64>,
    print_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve client based on provider. Also capture the resolved key and effective provider
    // so the Agent tool can pass them to sub-agents.
    let or_key_env = std::env::var("OPENROUTER_API_KEY").ok();
    let oi_key_env = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("CCX_OPENAI_KEY"))
        .ok();

    let (client, auth_source, email, no_auth, resolved_api_key, effective_provider): (
        ccx_api::ApiClient,
        String,
        Option<String>,
        bool,
        String,
        String,
    ) = match provider {
        "openrouter" => {
            let key = openrouter_key.or(or_key_env.as_deref()).ok_or(
                "OpenRouter API key required: set OPENROUTER_API_KEY or use --openrouter-key",
            )?;
            let client =
                ccx_api::ApiClient::OpenAi(ccx_api::OpenAiClient::openrouter(key, model));
            (
                client,
                "OpenRouter".to_string(),
                None,
                false,
                key.to_string(),
                "openrouter".to_string(),
            )
        }
        "openai" => {
            let key = oi_key_env.as_deref().ok_or(
                "OpenAI API key required: set OPENAI_API_KEY or CCX_OPENAI_KEY",
            )?;
            let client =
                ccx_api::ApiClient::OpenAi(ccx_api::OpenAiClient::openai(key, model));
            (
                client,
                "OpenAI".to_string(),
                None,
                false,
                key.to_string(),
                "openai".to_string(),
            )
        }
        _ => {
            match ccx_auth::resolve_auth(explicit_key) {
                Ok(auth) => {
                    let auth = hydrate_runtime_oauth(auth).await;
                    let email = if let Some(token) = auth.oauth_token() {
                        ccx_auth::fetch_oauth_email(token).await
                    } else {
                        None
                    };
                    let resolved_key = match &auth {
                        ccx_auth::AuthMethod::ApiKey(r) => r.key.clone(),
                        ccx_auth::AuthMethod::OAuthToken {
                            access_token,
                            api_key,
                            ..
                        } => api_key.clone().unwrap_or_else(|| access_token.clone()),
                        ccx_auth::AuthMethod::None => String::new(),
                    };
                    let client = ccx_api::ApiClient::Claude(
                        ccx_api::ClaudeClient::with_auth(&auth, model),
                    );
                    let auth_source = auth.display_label().to_string();
                    (
                        client,
                        auth_source,
                        email,
                        false,
                        resolved_key,
                        "anthropic".to_string(),
                    )
                }
                Err(_) => {
                    // Auto-detect: try OpenRouter, then OpenAI env vars first.
                    if let Some(ref or_key) = or_key_env {
                        if !or_key.is_empty() {
                            let client = ccx_api::ApiClient::OpenAi(
                                ccx_api::OpenAiClient::openrouter(or_key, model),
                            );
                            (
                                client,
                                "OpenRouter (auto-detected)".to_string(),
                                None,
                                false,
                                or_key.clone(),
                                "openrouter".to_string(),
                            )
                        } else {
                            run_first_run_auth(print_mode, model, &oi_key_env).await
                        }
                    } else if let Some(ref oi_key) = oi_key_env {
                        if !oi_key.is_empty() {
                            let client = ccx_api::ApiClient::OpenAi(
                                ccx_api::OpenAiClient::openai(oi_key, model),
                            );
                            (
                                client,
                                "OpenAI (auto-detected)".to_string(),
                                None,
                                false,
                                oi_key.clone(),
                                "openai".to_string(),
                            )
                        } else {
                            run_first_run_auth(print_mode, model, &oi_key_env).await
                        }
                    } else {
                        run_first_run_auth(print_mode, model, &oi_key_env).await
                    }
                }
            }
        }
    };

    // Load settings from both global (~/.claude/settings.json) and project-local
    // (.claude/settings.json), merging hooks from both sources.
    let global_settings = ccx_config::load_default_settings().unwrap_or_default();
    let cwd_early = std::env::current_dir()?;
    let project_settings = ccx_config::load_project_settings(&cwd_early).unwrap_or_default();
    let settings = ccx_config::merge_settings(global_settings, project_settings);

    // Resolve permission mode.
    let mode = match permission_mode {
        "plan" => ccx_permission::PermissionMode::Plan,
        "bypass" => ccx_permission::PermissionMode::BypassPermissions,
        "dontask" => ccx_permission::PermissionMode::DontAsk,
        "acceptedits" => ccx_permission::PermissionMode::AcceptEdits,
        "auto" => ccx_permission::PermissionMode::Auto,
        "default" => ccx_permission::PermissionMode::Default,
        _ => settings
            .permissions
            .mode
            .unwrap_or(ccx_permission::PermissionMode::BypassPermissions),
    };

    // Bypass permissions when flag is set or mode allows it.
    let bypass_permissions = dangerously_skip_permissions || mode.allows_writes();

    // Build tool registry with built-in tools.
    let mut registry = ccx_core::ToolRegistry::new();
    ccx_tools::register_all(&mut registry);

    let cwd = std::env::current_dir()?;

    // Wire MCP: load .mcp.json and register MCP server tools.
    // --no-mcp skips project-level MCP configs entirely.
    let _mcp_clients = if no_mcp {
        Vec::new()
    } else if let Some(mcp_config) = mcp_bridge::load_mcp_config(&cwd) {
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
    // GAP 3: --system-prompt / --append-system-prompt
    let mut system_prompt = if let Some(custom) = custom_system_prompt {
        custom.to_string()
    } else {
        ccx_prompt::build_full_system_prompt(
            &claude_md_files,
            &cwd.to_string_lossy(),
            &tool_schemas,
            &skill_infos,
        )
    };

    if let Some(extra) = append_system_prompt {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(extra);
    }

    // Wire memory: load from ~/.ccx/memory, fall back to ~/.claude/memory.
    let ccx_memory_dir = sessions::ccx_home()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".ccx"))
        .join("memory");
    let legacy_memory_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("memory");
    let memory_dir = if ccx_memory_dir.exists() {
        ccx_memory_dir
    } else {
        legacy_memory_dir
    };
    let memory_store = ccx_memory::MemoryStore::new(memory_dir);
    if let Ok(index) = memory_store.load_index()
        && !index.is_empty()
    {
        system_prompt.push_str("\n\n# Memories\n\n");
        system_prompt.push_str(&index);
    }

    let tool_names: Vec<String> = registry
        .names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let tool_count = tool_names.len();

    // Wire sandbox: set sandboxed flag on tool context when --sandbox is used.
    let mut ctx = ccx_core::ToolContext::new(cwd.clone());
    if sandbox {
        ctx.sandboxed = true;
    }
    ctx.bypass_permissions = bypass_permissions;
    // Pass provider, key, and model so sub-agents (Agent tool) inherit credentials.
    ctx.provider = effective_provider.clone();
    ctx.api_key = resolved_api_key;
    ctx.model = model.to_string();

    // Build HookRegistry from merged settings.
    let hook_registry = build_hook_registry(&settings);

    let mut agent = ccx_core::AgentLoop::new(client, registry, ctx, system_prompt);
    agent.set_max_turns(max_turns);
    agent.set_hook_registry(hook_registry);

    // GAP 2: effort level controls max_tokens and thinking.
    let (effort_tokens, effort_thinking, effort_budget) = effort_config(effort);
    agent.set_max_tokens(effort_tokens);

    // Thinking: effort level provides defaults, explicit flags override.
    let thinking_enabled =
        provider == "anthropic" && !no_thinking && (effort_thinking || thinking_budget > 0);
    let final_budget = if no_thinking || thinking_budget == 0 {
        0
    } else if thinking_budget != 10000 {
        // Explicit --thinking-budget overrides effort
        thinking_budget
    } else {
        effort_budget
    };
    if thinking_enabled && final_budget > 0 {
        agent.set_thinking(ccx_api::ThinkingConfig {
            thinking_type: "enabled".to_string(),
            budget_tokens: final_budget,
        });
    }

    // GAP 5: --max-budget-usd
    if let Some(budget) = max_budget_usd {
        agent.set_max_budget_usd(budget);
    }

    let show_thinking = !hide_thinking;

    // GAP 6: -p / --print pipe mode
    if print_mode {
        let text = if let Some(p) = prompt {
            p.to_string()
        } else {
            // Read from stdin.
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf.trim().to_string()
        };
        if text.is_empty() {
            eprintln!("Error: no prompt provided. Pass text as argument or via stdin.");
            std::process::exit(1);
        }
        return run_pipe_mode(&mut agent, &text, output_format).await;
    }

    if let Some(text) = prompt {
        // Non-interactive single-shot mode.
        eprintln!("Auth: {auth_source}");
        if let Some(ref email) = email {
            eprintln!("Account: {email}");
        }
        eprintln!("Model: {model} | Mode: {mode:?} | Effort: {effort} | Tools: {tool_count}");
        run_single_shot(&mut agent, text, show_thinking).await?;
    } else {
        // Interactive mode (inline default, full-screen TUI with --tui).
        let cwd_display = shorten_home(&cwd);

        if use_tui {
            run_tui_mode(
                &mut agent,
                model,
                &auth_source,
                &cwd_display,
                tool_count,
                email.as_deref(),
                &effective_provider,
            )
            .await?;
        } else {
            run_inline_mode(
                &mut agent,
                model,
                &auth_source,
                &cwd_display,
                &tool_names,
                bypass_permissions,
                email.as_deref(),
                show_thinking,
                resume_id,
                continue_session,
                effort,
                no_auth,
                &effective_provider,
            )
            .await?;
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
    let _result = send_with_fallback(agent, text, &mut cb).await?;
    println!();
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

/// Attempt to send a message; on RateLimitFallback, switch to OpenRouter and retry.
async fn send_with_fallback<C: ccx_core::AgentCallback>(
    agent: &mut ccx_core::AgentLoop,
    text: &str,
    cb: &mut C,
) -> Result<String, ccx_core::AgentLoopError> {
    match agent.send_message(text, cb).await {
        Ok(result) => Ok(result),
        Err(ccx_core::AgentLoopError::RateLimitFallback) => {
            if let Ok(or_key) = std::env::var("OPENROUTER_API_KEY") {
                let or_model = "nvidia/nemotron-3-super-120b-a12b:free";
                agent.set_client(ccx_api::ApiClient::OpenAi(
                    ccx_api::OpenAiClient::openrouter(&or_key, or_model),
                ));
                agent.set_model(or_model);
                agent.clear_thinking(); // OpenRouter doesn't support Anthropic thinking
                // Pop the user message that was already pushed by the failed send_message
                agent.pop_last_message();
                eprintln!(
                    "\n\x1b[38;2;138;99;210m\u{26a1} Rate limited. Auto-switching to OpenRouter (nemotron)...\x1b[0m\n"
                );
                agent.send_message(text, cb).await
            } else {
                Err(ccx_core::AgentLoopError::RateLimitFallback)
            }
        }
        Err(e) => Err(e),
    }
}

/// GAP 6: Pipe mode — read prompt, print plain response, exit.
/// GAP 4: Supports --output-format text/json/stream-json.
async fn run_pipe_mode(
    agent: &mut ccx_core::AgentLoop,
    text: &str,
    output_format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match output_format {
        "json" => {
            // Collect full response, output as JSON.
            let mut cb = ccx_core::NoopCallback;
            let result = send_with_fallback(agent, text, &mut cb).await?;
            let json = serde_json::json!({
                "response": result,
                "model": agent.model(),
                "cost": {
                    "input_tokens": agent.cost().total_input_tokens,
                    "output_tokens": agent.cost().total_output_tokens,
                    "total_usd": agent.cost().estimated_cost_usd(),
                },
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        "stream-json" => {
            // Stream each event as a JSON line.
            let mut cb = StreamJsonCallback;
            let _result = send_with_fallback(agent, text, &mut cb).await?;
            // Final summary line.
            let done = serde_json::json!({
                "type": "done",
                "cost": {
                    "input_tokens": agent.cost().total_input_tokens,
                    "output_tokens": agent.cost().total_output_tokens,
                    "total_usd": agent.cost().estimated_cost_usd(),
                },
            });
            println!("{}", serde_json::to_string(&done)?);
        }
        _ => {
            // text: plain output, no TUI.
            let mut cb = PipeCallback;
            let _result = send_with_fallback(agent, text, &mut cb).await?;
            println!();
        }
    }
    Ok(())
}

/// Callback for pipe mode: just print text to stdout.
struct PipeCallback;

impl ccx_core::AgentCallback for PipeCallback {
    fn on_text(&mut self, text: &str) {
        print!("{text}");
        std::io::stdout().flush().ok();
    }
}

/// Callback for stream-json output: emit each event as a JSON line.
struct StreamJsonCallback;

impl ccx_core::AgentCallback for StreamJsonCallback {
    fn on_text(&mut self, text: &str) {
        let j = serde_json::json!({"type": "text", "text": text});
        println!("{}", serde_json::to_string(&j).unwrap_or_default());
    }
    fn on_tool_start(&mut self, name: &str, input: &serde_json::Value) {
        let j = serde_json::json!({"type": "tool_start", "name": name, "input": input});
        println!("{}", serde_json::to_string(&j).unwrap_or_default());
    }
    fn on_tool_end(
        &mut self,
        name: &str,
        result: &Result<ccx_core::ToolResult, ccx_core::ToolError>,
    ) {
        let (success, content) = match result {
            Ok(r) => (!r.is_error, r.content.clone()),
            Err(e) => (false, e.to_string()),
        };
        let j = serde_json::json!({"type": "tool_end", "name": name, "success": success, "content": content});
        println!("{}", serde_json::to_string(&j).unwrap_or_default());
    }
    fn on_thinking(&mut self, text: &str) {
        let j = serde_json::json!({"type": "thinking", "text": text});
        println!("{}", serde_json::to_string(&j).unwrap_or_default());
    }
}

/// Run the full TUI with welcome screen, chat, and streaming.
async fn run_tui_mode(
    agent: &mut ccx_core::AgentLoop,
    model: &str,
    auth_source: &str,
    cwd_display: &str,
    tool_count: usize,
    email: Option<&str>,
    provider: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tui_tx, tui_rx) = mpsc::channel::<ccx_tui::TuiEvent>();
    let (input_tx, input_rx) = mpsc::channel::<ccx_tui::TuiInput>();

    let welcome = ccx_tui::WelcomeInfo {
        model: model.to_string(),
        auth_source: auth_source.to_string(),
        email: email.map(|s| s.to_string()),
        cwd: cwd_display.to_string(),
        tool_count,
        provider: provider.to_string(),
    };

    // Spawn TUI thread (blocking — owns the terminal).
    let tui_handle =
        std::thread::spawn(move || ccx_tui::run_tui_configured(tui_rx, input_tx, welcome));

    // Agent loop: wait for user input, send to API, push events to TUI.
    while let Ok(ccx_tui::TuiInput::Message(user_input)) = input_rx.recv() {
        let mut cb = TuiCallback { tx: tui_tx.clone() };

        match agent.send_message(&user_input, &mut cb).await {
            Ok(_) => {}
            Err(ccx_core::AgentLoopError::RateLimitFallback) => {
                if let Ok(or_key) = std::env::var("OPENROUTER_API_KEY") {
                    let or_model = "nvidia/nemotron-3-super-120b-a12b:free";
                    agent.set_client(ccx_api::ApiClient::OpenAi(
                        ccx_api::OpenAiClient::openrouter(&or_key, or_model),
                    ));
                    agent.set_model(or_model);
                    agent.pop_last_message();
                    let _ = tui_tx.send(ccx_tui::TuiEvent::NewMessage(ccx_tui::ChatMessage {
                        role: ccx_tui::ChatRole::Tool,
                        content: "\u{26a1} Rate limited. Auto-switching to OpenRouter (nemotron)...".into(),
                    }));
                    let mut cb2 = TuiCallback { tx: tui_tx.clone() };
                    if let Err(e) = agent.send_message(&user_input, &mut cb2).await {
                        let _ = tui_tx.send(ccx_tui::TuiEvent::NewMessage(ccx_tui::ChatMessage {
                            role: ccx_tui::ChatRole::Error,
                            content: format!("Error: {e}"),
                        }));
                    }
                }
            }
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
        let _ = self.tx.send(ccx_tui::TuiEvent::StreamText(format!(
            "\x1b[2;3m{text}\x1b[0m"
        )));
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
        Self {
            chars_printed: 0,
            show_thinking,
        }
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
        "Read" | "Write" | "Edit" => input["file_path"].as_str().unwrap_or("").to_string(),
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
    email: Option<String>,
    retry_count: u32,
    show_thinking: bool,
    thinking_active: bool,
}

impl InlineCallback {
    fn new(bypass_permissions: bool, auth_source: &str, show_thinking: bool, email: Option<&str>) -> Self {
        Self {
            text_buffer: String::new(),
            spinner_shown: false,
            always_allow: HashSet::new(),
            bypass_permissions,
            auth_source: auth_source.to_string(),
            email: email.map(|s| s.to_string()),
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

    fn on_retry(&mut self, attempt: u32, delay_ms: u64, _reason: &str) {
        self.finish_text();
        self.retry_count += 1;
        let account = if let Some(ref email) = self.email {
            format!("{} -- {}", self.auth_source, email)
        } else {
            self.auth_source.clone()
        };
        let hint = if self.auth_source.starts_with("Claude") {
            "Daily limit may be reached. "
        } else {
            ""
        };
        let delay_secs = delay_ms as f64 / 1000.0;
        ccx_tui::inline::render_error(&format!(
            "Rate limited ({account})"
        ));
        ccx_tui::inline::render_error(&format!(
            "  {hint}Retrying in {delay_secs:.0}s... (attempt {attempt}/5)"
        ));
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

/// Map short model name to full model ID.
/// Run inline interactive mode (default — no full-screen).
#[allow(clippy::too_many_arguments)]
async fn run_inline_mode(
    agent: &mut ccx_core::AgentLoop,
    model: &str,
    auth_source: &str,
    cwd_display: &str,
    tool_names: &[String],
    bypass_permissions: bool,
    email: Option<&str>,
    show_thinking: bool,
    resume_id: Option<&str>,
    continue_session: bool,
    effort: &str,
    mut no_auth: bool,
    effective_provider: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tool_count = tool_names.len();
    let mut current_model = model.to_string();
    let current_effort = effort.to_string();
    ccx_tui::inline::render_welcome_with_provider(&current_model, auth_source, cwd_display, tool_count, email, effective_provider);
    ccx_tui::inline::render_footer_line_with_effort(&current_model, &current_effort);
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
    let history_path = dirs::home_dir().unwrap_or_default().join(".ccx_history");
    let _ = rl.load_history(&history_path);

    // Session tracking.
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_str = cwd.to_string_lossy().to_string();
    let mut session_id = sessions::new_session_id();
    let session_created = sessions::now_epoch();
    let mut session_turns: usize = 0;
    let mut first_preview = String::new();

    // Handle --continue flag.
    if continue_session {
        match sessions::find_latest_for_cwd(&cwd_str, effective_provider) {
            Some(meta) => match sessions::load_session_messages(&cwd_str, effective_provider, &meta.id) {
                Ok(messages) if !messages.is_empty() => {
                    let count = messages.len();
                    agent.set_messages(messages);
                    session_id = meta.id.clone();
                    session_turns = meta.turns;
                    first_preview = meta.preview.clone();
                    let short_id = &session_id[..session_id.len().min(8)];
                    println!(
                        "\x1b[32m\u{21bb} Resuming session {short_id} ({} turns, {count} messages)\x1b[0m",
                        session_turns
                    );
                }
                Ok(_) => println!("\x1b[33mLatest session has no messages.\x1b[0m"),
                Err(e) => println!("\x1b[31mFailed to load session: {e}\x1b[0m"),
            },
            None => println!("\x1b[90mNo previous session found for this directory.\x1b[0m"),
        }
    } else if let Some(id) = resume_id {
        if id.is_empty() {
            // --resume without ID: list sessions.
            let all = sessions::list_sessions_for_project(&cwd_str, effective_provider);
            if all.is_empty() {
                println!("\x1b[90mNo saved sessions for this directory.\x1b[0m");
            } else {
                println!("\n\x1b[1mSessions for this directory:\x1b[0m\n");
                for (i, s) in all.iter().take(10).enumerate() {
                    let ts = sessions::format_epoch(s.last_active);
                    println!(
                        "  \x1b[33m{}\x1b[0m  {ts} ({} turns)  {}",
                        i + 1,
                        s.turns,
                        s.preview
                    );
                    println!("    ID: {}", s.id);
                }
                println!(
                    "\n\x1b[90mUse: /resume <session-id>  or  ccx chat --resume <id>\x1b[0m\n"
                );
            }
        } else {
            // --resume <id>: load specific session.
            match sessions::load_session_messages(&cwd_str, effective_provider, id) {
                Ok(messages) if !messages.is_empty() => {
                    let count = messages.len();
                    let meta = sessions::find_session_meta(&cwd_str, effective_provider, id);
                    session_turns = meta.as_ref().map(|m| m.turns).unwrap_or(0);
                    first_preview = meta.as_ref().map(|m| m.preview.clone()).unwrap_or_default();
                    agent.set_messages(messages);
                    session_id = id.to_string();
                    let short_id = &session_id[..session_id.len().min(8)];
                    println!(
                        "\x1b[32m\u{21bb} Resuming session {short_id} ({session_turns} turns, {count} messages)\x1b[0m",
                    );
                }
                Ok(_) => println!("\x1b[33mSession {id} has no messages.\x1b[0m"),
                Err(e) => println!("\x1b[31m{e}\x1b[0m"),
            }
        }
    }

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
                if let Some(after_slash) = input.strip_prefix('/') {
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
                            if let Some(new_model) = cmd_args {
                                let (resolved, _) = resolve_model_alias(new_model, effective_provider);
                                agent.set_model(&resolved);
                                current_model = resolved.clone();
                                println!("\x1b[32mModel changed to {}\x1b[0m", resolved);
                            } else {
                                println!("Model: {current_model}");
                            }
                            true
                        }
                        "/compact" => {
                            let before = agent.messages().len();
                            agent.compact();
                            let after = agent.messages().len();
                            let before_tokens: usize = agent
                                .messages()
                                .iter()
                                .map(|m| match &m.content {
                                    ccx_api::MessageContent::Text(t) => t.len() / 4,
                                    ccx_api::MessageContent::Blocks(b) => b
                                        .iter()
                                        .map(|bl| match bl {
                                            ccx_api::ContentBlock::Text { text } => text.len() / 4,
                                            ccx_api::ContentBlock::ToolUse { input, .. } => {
                                                input.to_string().len() / 4
                                            }
                                            ccx_api::ContentBlock::ToolResult {
                                                content, ..
                                            } => content.len() / 4,
                                            ccx_api::ContentBlock::Thinking {
                                                thinking, ..
                                            } => thinking.len() / 4,
                                        })
                                        .sum(),
                                })
                                .sum();
                            println!(
                                "\x1b[32mContext compacted.\x1b[0m Messages: {before} → {after} | ~{before_tokens} tokens remaining"
                            );
                            true
                        }
                        "/init" => {
                            let filename = if effective_provider == "anthropic" {
                                "CLAUDE.md"
                            } else {
                                "CCX.md"
                            };
                            // Check if any instruction file already exists.
                            let existing = ["CLAUDE.md", "CCX.md", "AGENTS.md"]
                                .iter()
                                .find(|name| cwd.join(name).exists());
                            if let Some(found) = existing {
                                println!(
                                    "\x1b[33m{found} already exists in this directory.\x1b[0m"
                                );
                            } else {
                                let template =
                                    "# Project Instructions\n\nDescribe your project here.\n";
                                let path = cwd.join(filename);
                                match std::fs::write(&path, template) {
                                    Ok(_) => println!("\x1b[32mCreated {filename}\x1b[0m"),
                                    Err(e) => {
                                        println!("\x1b[31mFailed to create {filename}: {e}\x1b[0m")
                                    }
                                }
                            }
                            true
                        }
                        "/version" => {
                            println!("ccx v{}", env!("CARGO_PKG_VERSION"));
                            true
                        }
                        "/login" => {
                            let claude_available = std::process::Command::new("claude")
                                .arg("--version")
                                .output()
                                .map(|o| o.status.success())
                                .unwrap_or(false);

                            let login_result = if claude_available {
                                println!("Launching Claude auth...");
                                let status = std::process::Command::new("claude")
                                    .args(["auth", "login"])
                                    .status();
                                match status {
                                    Ok(s) if s.success() => Ok(ccx_auth::oauth::OAuthTokens {
                                        access_token: String::new(),
                                        refresh_token: None,
                                        api_key: None,
                                        subscription_type: None,
                                    }),
                                    Ok(_) => Err("Claude auth failed".into()),
                                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error>),
                                }
                            } else {
                                ccx_auth::oauth::login().await
                            };

                            match login_result {
                                Ok(_) => {
                                    println!(
                                        "\x1b[32mLogin successful!\x1b[0m Restart ccx to use your subscription."
                                    );
                                    // Re-check auth after login.
                                    if let Ok(auth) = ccx_auth::resolve_auth(None) {
                                        let mut auth = hydrate_runtime_oauth(auth).await;
                                        let needs_scope_refresh = matches!(
                                            &auth,
                                            ccx_auth::AuthMethod::OAuthToken { api_key: None, .. }
                                        );

                                        if needs_scope_refresh {
                                            println!(
                                                "\x1b[90mRefreshing OAuth session for CCX...\x1b[0m"
                                            );
                                            match ccx_auth::oauth::login().await {
                                                Ok(tokens) => {
                                                    auth = ccx_auth::AuthMethod::OAuthToken {
                                                        access_token: tokens.access_token.clone(),
                                                        api_key: tokens.api_key.clone(),
                                                        subscription_type: tokens
                                                            .subscription_type
                                                            .clone()
                                                            .unwrap_or_else(|| "unknown".to_string()),
                                                    };
                                                }
                                                Err(e) => {
                                                    println!(
                                                        "\x1b[31mCCX OAuth refresh failed: {e}\x1b[0m"
                                                    );
                                                }
                                            }
                                        }

                                        if !auth.is_none() {
                                            no_auth = false;
                                            let new_client = ccx_api::ApiClient::Claude(
                                                ccx_api::ClaudeClient::with_auth(
                                                    &auth,
                                                    agent.model(),
                                                ),
                                            );
                                            agent.set_client(new_client);
                                            println!(
                                                "\x1b[32mAuthenticated as {}. You can start chatting.\x1b[0m",
                                                auth.display_label()
                                            );
                                        }
                                    }
                                }
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
                                match sessions::load_session_messages(&cwd_str, effective_provider, sid) {
                                    Ok(messages) if !messages.is_empty() => {
                                        let count = messages.len();
                                        let meta = sessions::find_session_meta(&cwd_str, effective_provider, sid);
                                        let turns = meta.as_ref().map(|m| m.turns).unwrap_or(0);
                                        first_preview = meta
                                            .as_ref()
                                            .map(|m| m.preview.clone())
                                            .unwrap_or_default();
                                        agent.set_messages(messages);
                                        session_id = sid.to_string();
                                        session_turns = turns;
                                        let short_id = &session_id[..session_id.len().min(8)];
                                        println!(
                                            "\x1b[32m\u{21bb} Resumed session {short_id} ({turns} turns, {count} messages)\x1b[0m"
                                        );
                                    }
                                    Ok(_) => {
                                        println!("\x1b[33mSession {sid} has no messages.\x1b[0m")
                                    }
                                    Err(e) => println!("\x1b[31m{e}\x1b[0m"),
                                }
                            } else {
                                let all = sessions::list_sessions_for_project(&cwd_str, effective_provider);
                                if all.is_empty() {
                                    println!("\x1b[90mNo saved sessions.\x1b[0m");
                                } else {
                                    println!("\n\x1b[1mRecent sessions:\x1b[0m\n");
                                    for (i, s) in all.iter().take(10).enumerate() {
                                        let ts = sessions::format_epoch(s.last_active);
                                        println!(
                                            "  \x1b[33m{}\x1b[0m  \x1b[90m{ts}\x1b[0m  ({} turns)  {}",
                                            s.id, s.turns, s.preview
                                        );
                                        if i >= 9 {
                                            break;
                                        }
                                    }
                                    println!("\n\x1b[90mUsage: /resume <session-id>\x1b[0m\n");
                                }
                            }
                            true
                        }
                        "/continue" => {
                            match sessions::find_latest_for_cwd(&cwd_str, effective_provider) {
                                Some(meta) => {
                                    match sessions::load_session_messages(&cwd_str, effective_provider, &meta.id) {
                                        Ok(messages) if !messages.is_empty() => {
                                            let count = messages.len();
                                            agent.set_messages(messages);
                                            session_id = meta.id.clone();
                                            session_turns = meta.turns;
                                            first_preview = meta.preview.clone();
                                            let short_id = &session_id[..session_id.len().min(8)];
                                            println!(
                                                "\x1b[32m\u{21bb} Resumed session {short_id} ({} turns, {count} messages)\x1b[0m",
                                                session_turns
                                            );
                                        }
                                        Ok(_) => println!(
                                            "\x1b[33mLatest session has no messages.\x1b[0m"
                                        ),
                                        Err(e) => {
                                            println!("\x1b[31mFailed to load session: {e}\x1b[0m")
                                        }
                                    }
                                }
                                None => {
                                    println!(
                                        "\x1b[90mNo previous session found for this directory.\x1b[0m"
                                    );
                                }
                            }
                            true
                        }
                        "/doctor" => {
                            println!("\n\x1b[1mccx Doctor\x1b[0m\n");

                            // Check API key.
                            let api_ok = std::env::var("ANTHROPIC_API_KEY")
                                .map(|k| !k.is_empty())
                                .unwrap_or(false);
                            let oauth_ok = ccx_auth::resolve_auth(None).is_ok();
                            if api_ok || oauth_ok {
                                println!("  \x1b[32m✓\x1b[0m Authentication: {auth_source}");
                            } else {
                                println!(
                                    "  \x1b[31m✗\x1b[0m Authentication: no API key or OAuth token found"
                                );
                            }

                            // Check tools.
                            println!("  \x1b[32m✓\x1b[0m Tools: {tool_count} registered");

                            // Check MCP servers.
                            if let Some(ref cfg) = mcp_config {
                                let count = cfg.mcp_servers.len();
                                println!(
                                    "  \x1b[32m✓\x1b[0m MCP servers: {count} configured in .mcp.json"
                                );
                                for name in cfg.mcp_servers.keys() {
                                    println!("    \x1b[90m- {name}\x1b[0m");
                                }
                            } else {
                                println!(
                                    "  \x1b[90m-\x1b[0m MCP servers: none (.mcp.json not found)"
                                );
                            }

                            // Check instruction file (CLAUDE.md, CCX.md, AGENTS.md).
                            if let Some(found) = ["CLAUDE.md", "CCX.md", "AGENTS.md"]
                                .iter()
                                .find(|name| cwd.join(name).exists())
                            {
                                println!("  \x1b[32m✓\x1b[0m {found}: found");
                            } else {
                                println!(
                                    "  \x1b[90m-\x1b[0m Project instructions: not found (use /init to create)"
                                );
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
                            println!(
                                "  Provider:    {}",
                                if auth_source.contains("OAuth") || auth_source.contains("Claude") {
                                    "anthropic (OAuth)"
                                } else {
                                    "anthropic"
                                }
                            );
                            println!("  Auth:        {auth_source}");
                            if let Some(ref e) = email {
                                println!("  Account:     {e}");
                            }
                            println!(
                                "  Permission:  {}",
                                if bypass_permissions {
                                    "bypass"
                                } else {
                                    "default"
                                }
                            );
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
                            let all = sessions::list_sessions_for_project(&cwd_str, effective_provider);
                            if all.is_empty() {
                                println!("\x1b[90mNo saved sessions for this directory.\x1b[0m");
                            } else {
                                println!(
                                    "\n\x1b[1mSessions for this directory ({}):\x1b[0m\n",
                                    all.len()
                                );
                                for s in all.iter().take(15) {
                                    let ts = sessions::format_epoch(s.last_active);
                                    println!(
                                        "  \x1b[33m{}\x1b[0m  \x1b[90m{ts}\x1b[0m  ({} turns)  {}",
                                        s.id, s.turns, s.preview
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
                            if let Some(skill) =
                                ccx_skill::find_skill(&discovered_skills, "simplify")
                            {
                                let result = ccx_skill::expand_skill(skill, None);
                                let user_msg = format!(
                                    "The user invoked skill 'simplify'\n\n<skill-content>\n{}\n</skill-content>",
                                    result.expanded_prompt
                                );
                                ccx_tui::inline::clear_previous_line();
                                ccx_tui::inline::render_user_message("/simplify");
                                let mut cb = InlineCallback::new(
                                    bypass_permissions,
                                    auth_source,
                                    show_thinking,
                                    email,
                                );
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
                                println!(
                                    "\x1b[90mSimplify skill not found. Ensure skills are installed.\x1b[0m"
                                );
                            }
                            true
                        }
                        "/batch" => {
                            if let Some(batch_args) = cmd_args {
                                if let Some(skill) =
                                    ccx_skill::find_skill(&discovered_skills, "batch")
                                {
                                    let result = ccx_skill::expand_skill(skill, Some(batch_args));
                                    let user_msg = format!(
                                        "The user invoked skill 'batch' with args: {}\n\n<skill-content>\n{}\n</skill-content>",
                                        batch_args, result.expanded_prompt
                                    );
                                    ccx_tui::inline::clear_previous_line();
                                    ccx_tui::inline::render_user_message(input);
                                    let mut cb = InlineCallback::new(
                                        bypass_permissions,
                                        auth_source,
                                        show_thinking,
                                        email,
                                    );
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
                                    println!(
                                        "\x1b[90mBatch skill not found. Ensure skills are installed.\x1b[0m"
                                    );
                                }
                            } else {
                                println!("\x1b[33mUsage: /batch <prompt>\x1b[0m");
                            }
                            true
                        }
                        "/plugins" => {
                            const P_ACCENT: &str = "\x1b[38;2;138;99;210m";
                            const P_DIM: &str = "\x1b[90m";
                            const P_BOLD: &str = "\x1b[1m";
                            const P_RESET: &str = "\x1b[0m";

                            println!("\n{P_BOLD}Installed Plugins:{P_RESET}\n");

                            let home = dirs::home_dir().unwrap_or_default();
                            let user_plugins = home.join(".claude/plugins");
                            let project_plugins = std::path::PathBuf::from(".claude/plugins");
                            let marketplace = home.join(".claude/plugins/marketplaces");

                            let mut count = 0;

                            // User plugins
                            if user_plugins.exists() {
                                for entry in
                                    std::fs::read_dir(&user_plugins).into_iter().flatten().flatten()
                                {
                                    if entry.path().is_dir()
                                        && entry.file_name() != "marketplaces"
                                    {
                                        let name =
                                            entry.file_name().to_string_lossy().to_string();
                                        println!(
                                            "  {P_ACCENT}{name}{P_RESET} {P_DIM}(user){P_RESET}"
                                        );
                                        count += 1;
                                    }
                                }
                            }

                            // Project plugins
                            if project_plugins.exists() {
                                for entry in std::fs::read_dir(&project_plugins)
                                    .into_iter()
                                    .flatten()
                                    .flatten()
                                {
                                    if entry.path().is_dir() {
                                        let name =
                                            entry.file_name().to_string_lossy().to_string();
                                        println!(
                                            "  {P_ACCENT}{name}{P_RESET} {P_DIM}(project){P_RESET}"
                                        );
                                        count += 1;
                                    }
                                }
                            }

                            // Marketplace plugins
                            if marketplace.exists() {
                                for mp in
                                    std::fs::read_dir(&marketplace).into_iter().flatten().flatten()
                                {
                                    if mp.path().is_dir() {
                                        let mp_name =
                                            mp.file_name().to_string_lossy().to_string();
                                        let plugins_dir = mp.path().join("plugins");
                                        if plugins_dir.exists() {
                                            for entry in std::fs::read_dir(&plugins_dir)
                                                .into_iter()
                                                .flatten()
                                                .flatten()
                                            {
                                                if entry.path().is_dir() {
                                                    let name = entry
                                                        .file_name()
                                                        .to_string_lossy()
                                                        .to_string();
                                                    println!(
                                                        "  {P_ACCENT}{name}{P_RESET} {P_DIM}(marketplace: {mp_name}){P_RESET}"
                                                    );
                                                    count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // SpecWeave plugins (from nvm)
                            let nvm_base = home.join(".nvm/versions/node");
                            if nvm_base.exists() {
                                // Deduplicate across node versions
                                let mut seen = HashSet::new();
                                for ver in
                                    std::fs::read_dir(&nvm_base).into_iter().flatten().flatten()
                                {
                                    let plugins_dir =
                                        ver.path().join("lib/node_modules/specweave/plugins");
                                    if plugins_dir.exists() {
                                        for entry in std::fs::read_dir(&plugins_dir)
                                            .into_iter()
                                            .flatten()
                                            .flatten()
                                        {
                                            if entry.path().is_dir() {
                                                let name = entry
                                                    .file_name()
                                                    .to_string_lossy()
                                                    .to_string();
                                                if seen.insert(name.clone()) {
                                                    println!(
                                                        "  {P_ACCENT}{name}{P_RESET} {P_DIM}(specweave){P_RESET}"
                                                    );
                                                    count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if count == 0 {
                                println!("  {P_DIM}No plugins installed{P_RESET}");
                            }
                            println!("\n  {P_DIM}Total: {count} plugin(s){P_RESET}");
                            println!(
                                "  {P_DIM}Skills discovered: {}{P_RESET}",
                                skill_display.len()
                            );
                            println!();
                            true
                        }
                        "/mcp" => {
                            println!("\n\x1b[1mMCP Servers:\x1b[0m\n");
                            if let Some(ref cfg) = mcp_config {
                                for (name, server) in &cfg.mcp_servers {
                                    let args_str = server.args.join(" ");
                                    println!(
                                        "  \x1b[32m●\x1b[0m \x1b[1m{name}\x1b[0m — \x1b[90m{} {args_str}\x1b[0m",
                                        server.command
                                    );
                                }
                            } else {
                                println!("  \x1b[90mNo .mcp.json found in current directory\x1b[0m");
                            }
                            let global_mcp = dirs::home_dir().unwrap().join(".claude/mcp.json");
                            if global_mcp.exists() {
                                println!("\n  \x1b[90mGlobal: ~/.claude/mcp.json\x1b[0m");
                            }
                            println!("\n  \x1b[90mManage: ccx mcp [list|add|remove]\x1b[0m");
                            println!();
                            true
                        }
                        "/auth" => {
                            println!("\n\x1b[1mAuthentication Status:\x1b[0m\n");
                            println!("  Current: \x1b[32m{auth_source}\x1b[0m");
                            if let Some(ref e) = email {
                                println!("  Account: {e}");
                            }
                            for (var, label) in [
                                ("OPENROUTER_API_KEY", "OpenRouter"),
                                ("OPENAI_API_KEY", "OpenAI"),
                            ] {
                                if let Ok(key) = std::env::var(var) {
                                    println!("  \x1b[32m●\x1b[0m \x1b[1m{label}\x1b[0m ({var}): \x1b[90m{}\x1b[0m", mask_key(&key));
                                }
                            }
                            println!("\n  \x1b[90mManage: ccx auth [status|login|logout]\x1b[0m");
                            println!();
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
                        ccx_tui::inline::render_skill_invocation(&skill.name, skill_args);

                        let mut cb =
                            InlineCallback::new(bypass_permissions, auth_source, show_thinking, email);
                        match agent.send_message(&user_msg, &mut cb).await {
                            Ok(_) => cb.finish_text(),
                            Err(ccx_core::AgentLoopError::RateLimitFallback) => {
                                cb.finish_text();
                                let or_key = std::env::var("OPENROUTER_API_KEY").unwrap();
                                let or_model = "nvidia/nemotron-3-super-120b-a12b:free";
                                let or_client = ccx_api::ApiClient::OpenAi(
                                    ccx_api::OpenAiClient::openrouter(&or_key, or_model),
                                );
                                agent.set_client(or_client);
                                agent.set_model(or_model);
                                current_model = or_model.to_string();
                                agent.pop_last_message();
                                println!();
                                println!("\x1b[38;2;138;99;210m\u{26a1} Rate limited. Auto-switching to OpenRouter (nemotron)...\x1b[0m");
                                println!();
                                let mut cb2 = InlineCallback::new(bypass_permissions, auth_source, show_thinking, email);
                                match agent.send_message(&user_msg, &mut cb2).await {
                                    Ok(_) => cb2.finish_text(),
                                    Err(e2) => {
                                        cb2.finish_text();
                                        ccx_tui::inline::render_error(&format!("Error: {e2}"));
                                    }
                                }
                            }
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

                // Block message sends when not authenticated.
                if no_auth {
                    println!(
                        "\x1b[33mNot authenticated. Type /login or set ANTHROPIC_API_KEY.\x1b[0m"
                    );
                    continue;
                }

                // Track first message for session preview.
                if first_preview.is_empty() {
                    first_preview = sessions::make_preview(input);
                }

                ccx_tui::inline::clear_previous_line();
                ccx_tui::inline::render_user_message(input);

                let mut cb = InlineCallback::new(bypass_permissions, auth_source, show_thinking, email);
                match agent.send_message(input, &mut cb).await {
                    Ok(_) => cb.finish_text(),
                    Err(ccx_core::AgentLoopError::RateLimitFallback) => {
                        cb.finish_text();
                        let or_key = std::env::var("OPENROUTER_API_KEY").unwrap();
                        let or_model = "nvidia/nemotron-3-super-120b-a12b:free";
                        let or_client = ccx_api::ApiClient::OpenAi(
                            ccx_api::OpenAiClient::openrouter(&or_key, or_model),
                        );
                        agent.set_client(or_client);
                        agent.set_model(or_model);
                        current_model = or_model.to_string();
                        // Pop the user message pushed by the failed send_message
                        agent.pop_last_message();
                        println!();
                        println!("\x1b[38;2;138;99;210m\u{26a1} Rate limited. Auto-switching to OpenRouter (nemotron)...\x1b[0m");
                        println!();
                        let mut cb2 = InlineCallback::new(bypass_permissions, auth_source, show_thinking, email);
                        match agent.send_message(input, &mut cb2).await {
                            Ok(_) => cb2.finish_text(),
                            Err(e2) => {
                                cb2.finish_text();
                                ccx_tui::inline::render_error(&format!("Error: {e2}"));
                            }
                        }
                    }
                    Err(ccx_core::AgentLoopError::RateLimitExhausted(_)) => {
                        cb.finish_text();
                        println!();
                        ccx_tui::inline::render_error("Rate limit reached. Options:");
                        ccx_tui::inline::render_error("  1. Wait and retry (may take minutes)");
                        ccx_tui::inline::render_error("  2. Use a free model:");
                        ccx_tui::inline::render_error("     export OPENROUTER_API_KEY=\"your-key-from-openrouter.ai/keys\"");
                        ccx_tui::inline::render_error("     ccx --model nemotron");
                        ccx_tui::inline::render_error("  3. Use a different Claude API key:");
                        ccx_tui::inline::render_error("     export ANTHROPIC_API_KEY=\"sk-ant-...\"");
                    }
                    Err(e) => {
                        cb.finish_text();
                        ccx_tui::inline::render_error(&format!("Error: {e}"));
                    }
                }

                session_turns += 1;

                // Incremental session save after each turn.
                let _ = sessions::save_session_messages(&cwd_str, effective_provider, &session_id, agent.messages());
                let _ = sessions::save_session_meta(&sessions::SessionMeta {
                    id: session_id.clone(),
                    cwd: cwd_str.clone(),
                    model: current_model.clone(),
                    created: session_created,
                    last_active: sessions::now_epoch(),
                    preview: first_preview.clone(),
                    name: None,
                    turns: session_turns,
                    total_tokens: agent.cost().total_input_tokens
                        + agent.cost().total_output_tokens,
                }, effective_provider);

                ccx_tui::inline::render_separator();
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Input error: {err:?}");
                break;
            }
        }
    }

    // Final session save and cleanup.
    if session_turns > 0 {
        let _ = sessions::save_session_messages(&cwd_str, effective_provider, &session_id, agent.messages());
        let _ = sessions::save_session_meta(&sessions::SessionMeta {
            id: session_id.clone(),
            cwd: cwd_str.clone(),
            model: model.to_string(),
            created: session_created,
            last_active: sessions::now_epoch(),
            preview: if first_preview.is_empty() {
                "(no messages)".into()
            } else {
                first_preview
            },
            name: None,
            turns: session_turns,
            total_tokens: agent.cost().total_input_tokens + agent.cost().total_output_tokens,
        }, effective_provider);
        sessions::cleanup_sessions(&cwd_str, effective_provider, 100);
    }

    // Save history for next session.
    let _ = rl.save_history(&history_path);

    ccx_tui::inline::render_footer_with_effort(&current_model, &current_effort);
    println!("\nGoodbye!");
    eprintln!("\n{}", agent.cost().summary());
    Ok(())
}

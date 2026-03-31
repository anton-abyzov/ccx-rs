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
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            model,
            api_key,
            prompt,
        } => {
            if let Err(e) = run_chat(&model, api_key.as_deref(), prompt.as_deref()).await {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }
}

async fn run_chat(
    model: &str,
    explicit_key: Option<&str>,
    prompt: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolved = ccx_auth::resolve_api_key(explicit_key)?;
    let client = ccx_api::ClaudeClient::new(&resolved.key, model);

    match &resolved.source {
        ccx_auth::KeySource::EnvVar => eprintln!("Using API key from ANTHROPIC_API_KEY"),
        ccx_auth::KeySource::ConfigFile(path) => {
            eprintln!("Using API key from {}", path.display())
        }
        ccx_auth::KeySource::Explicit => eprintln!("Using provided API key"),
    }
    eprintln!("Model: {}", client.model());

    if let Some(text) = prompt {
        // Non-interactive single prompt mode.
        use ccx_api::{InputMessage, MessageContent, MessageRequest, Role, StreamEvent};
        use futures::StreamExt;

        let req = MessageRequest {
            model: model.to_string(),
            max_tokens: 4096,
            messages: vec![InputMessage {
                role: Role::User,
                content: MessageContent::Text(text.to_string()),
            }],
            system: None,
            temperature: None,
            tools: None,
            stream: Some(true),
        };

        let mut stream = client.stream_message(req).await?;
        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::ContentBlockDelta {
                    delta: ccx_api::Delta::TextDelta { text },
                    ..
                } => print!("{text}"),
                StreamEvent::MessageStop => println!(),
                _ => {}
            }
        }
    } else {
        // Interactive mode placeholder.
        eprintln!("Interactive mode not yet implemented. Use --prompt for single queries.");
    }

    Ok(())
}

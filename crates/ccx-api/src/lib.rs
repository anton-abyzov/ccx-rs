pub mod client;
pub mod error;
pub mod openai_client;
pub mod types;

pub use client::ClaudeClient;
pub use error::Error;
pub use openai_client::OpenAiClient;
pub use types::*;

/// Unified API client that works with both Anthropic and OpenAI-compatible endpoints.
pub enum ApiClient {
    Claude(ClaudeClient),
    OpenAi(OpenAiClient),
}

impl ApiClient {
    pub fn model(&self) -> &str {
        match self {
            Self::Claude(c) => c.model(),
            Self::OpenAi(c) => c.model(),
        }
    }

    pub async fn stream_message(
        &self,
        req: MessageRequest,
    ) -> Result<
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<StreamEvent, Error>> + Send>>,
        Error,
    > {
        match self {
            Self::Claude(c) => c.stream_message(req).await,
            Self::OpenAi(c) => c.stream_message(req).await,
        }
    }
}

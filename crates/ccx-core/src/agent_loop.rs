use ccx_api::{
    ApiClient, ContentBlock, Delta, InputMessage, MessageContent, MessageRequest, Role,
    StopReason, StreamEvent,
};
use futures::StreamExt;

use crate::context::ToolContext;
use crate::cost::CostTracker;
use crate::tool::{ToolError, ToolRegistry, ToolResult};

/// Tracks a content block being streamed.
enum PendingBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        json_buf: String,
    },
    Thinking(String),
    Other,
}

/// Configuration for rate limit retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries for rate-limited requests.
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds).
    pub base_delay_ms: u64,
    /// Maximum delay cap (milliseconds).
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
        }
    }
}

/// The main agent loop: message -> API -> tool_use -> execute -> loop.
pub struct AgentLoop {
    client: ApiClient,
    registry: ToolRegistry,
    context: ToolContext,
    system_prompt: String,
    messages: Vec<InputMessage>,
    max_turns: usize,
    cost: CostTracker,
    retry_config: RetryConfig,
}

/// Callback for streaming events.
pub trait AgentCallback: Send {
    fn on_text(&mut self, _text: &str) {}
    fn on_tool_start(&mut self, _name: &str, _input: &serde_json::Value) {}
    fn on_tool_end(&mut self, _name: &str, _result: &Result<ToolResult, ToolError>) {}
    fn on_thinking(&mut self, _text: &str) {}
    fn on_turn_complete(&mut self, _turn: usize, _cost: &CostTracker) {}
    fn on_retry(&mut self, _attempt: u32, _delay_ms: u64, _reason: &str) {}
    /// Called before tool execution to check permission. Return false to deny.
    fn should_allow_tool(&mut self, _name: &str, _input: &serde_json::Value) -> bool {
        true
    }
}

/// No-op callback.
pub struct NoopCallback;
impl AgentCallback for NoopCallback {}

#[derive(Debug, thiserror::Error)]
pub enum AgentLoopError {
    #[error("API error: {0}")]
    Api(String),
    #[error("exceeded maximum turns ({0})")]
    MaxTurnsExceeded(usize),
    #[error("rate limited after {0} retries")]
    RateLimitExhausted(u32),
}

impl AgentLoop {
    pub fn new(
        client: ApiClient,
        registry: ToolRegistry,
        context: ToolContext,
        system_prompt: String,
    ) -> Self {
        Self {
            client,
            registry,
            context,
            system_prompt,
            messages: Vec::new(),
            max_turns: 50,
            cost: CostTracker::new(),
            retry_config: RetryConfig::default(),
        }
    }

    pub fn set_max_turns(&mut self, max: usize) {
        self.max_turns = max;
    }

    pub fn set_retry_config(&mut self, config: RetryConfig) {
        self.retry_config = config;
    }

    pub fn messages(&self) -> &[InputMessage] {
        &self.messages
    }

    pub fn cost(&self) -> &CostTracker {
        &self.cost
    }

    /// Send a user message and run the agent loop until completion.
    pub async fn send_message(
        &mut self,
        user_text: &str,
        callback: &mut dyn AgentCallback,
    ) -> Result<String, AgentLoopError> {
        self.messages.push(InputMessage {
            role: Role::User,
            content: MessageContent::Text(user_text.to_string()),
        });

        let mut turn = 0;
        loop {
            if turn >= self.max_turns {
                return Err(AgentLoopError::MaxTurnsExceeded(self.max_turns));
            }
            turn += 1;

            let req = MessageRequest {
                model: String::new(),
                max_tokens: 16384,
                messages: self.messages.clone(),
                system: Some(self.system_prompt.clone()),
                temperature: None,
                tools: Some(self.registry.tool_definitions()),
                stream: Some(true),
            };

            // Execute with rate limit retry.
            let stream_result =
                self.stream_with_retry(req, callback).await?;

            let (blocks, stop_reason, usage) = stream_result;

            // Record usage for this turn.
            if let Some(usage) = &usage {
                self.cost.record(usage);
            }

            // Build assistant content blocks from accumulated stream data.
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();

            for block in blocks {
                match block {
                    PendingBlock::Text(text) => {
                        if !text.is_empty() {
                            content.push(ContentBlock::Text { text });
                        }
                    }
                    PendingBlock::ToolUse { id, name, json_buf } => {
                        let input: serde_json::Value = if json_buf.is_empty() {
                            serde_json::Value::Object(Default::default())
                        } else {
                            serde_json::from_str(&json_buf).unwrap_or_default()
                        };
                        content.push(ContentBlock::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                        });
                        tool_calls.push((id, name, input));
                    }
                    PendingBlock::Thinking(thinking) => {
                        if !thinking.is_empty() {
                            content.push(ContentBlock::Thinking { thinking });
                        }
                    }
                    PendingBlock::Other => {}
                }
            }

            self.messages.push(InputMessage {
                role: Role::Assistant,
                content: MessageContent::Blocks(content),
            });

            // Execute tool calls if the model requested them.
            if stop_reason == Some(StopReason::ToolUse) && !tool_calls.is_empty() {
                let mut results = Vec::new();
                for (id, name, input) in tool_calls {
                    if !callback.should_allow_tool(&name, &input) {
                        results.push(ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: "Tool execution denied by user".to_string(),
                            is_error: Some(true),
                        });
                        continue;
                    }
                    callback.on_tool_start(&name, &input);
                    let result =
                        self.registry.execute(&name, input, &self.context).await;
                    callback.on_tool_end(&name, &result);

                    let (tool_content, is_error) = match result {
                        Ok(r) => (r.content, r.is_error),
                        Err(e) => (e.to_string(), true),
                    };
                    results.push(ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: tool_content,
                        is_error: if is_error { Some(true) } else { None },
                    });
                }

                self.messages.push(InputMessage {
                    role: Role::User,
                    content: MessageContent::Blocks(results),
                });

                callback.on_turn_complete(turn, &self.cost);
                continue;
            }

            callback.on_turn_complete(turn, &self.cost);

            // Extract final text from the last assistant message.
            let final_text = self
                .messages
                .last()
                .and_then(|m| match &m.content {
                    MessageContent::Blocks(blocks) => {
                        let texts: Vec<&str> = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect();
                        if texts.is_empty() {
                            None
                        } else {
                            Some(texts.join(""))
                        }
                    }
                    MessageContent::Text(t) => Some(t.clone()),
                })
                .unwrap_or_default();

            return Ok(final_text);
        }
    }

    /// Stream a request with exponential backoff on rate limits.
    async fn stream_with_retry(
        &self,
        req: MessageRequest,
        callback: &mut dyn AgentCallback,
    ) -> Result<
        (
            Vec<PendingBlock>,
            Option<StopReason>,
            Option<ccx_api::Usage>,
        ),
        AgentLoopError,
    > {
        let mut attempt = 0;

        loop {
            match self.client.stream_message(req.clone()).await {
                Ok(stream) => {
                    return self.consume_stream(stream, callback).await;
                }
                Err(ccx_api::Error::RateLimit { retry_after_secs }) => {
                    attempt += 1;
                    if attempt > self.retry_config.max_retries {
                        return Err(AgentLoopError::RateLimitExhausted(attempt));
                    }

                    let delay_ms = if let Some(secs) = retry_after_secs {
                        secs * 1000
                    } else {
                        // Exponential backoff with jitter.
                        let base = self.retry_config.base_delay_ms
                            * 2u64.pow(attempt - 1);
                        base.min(self.retry_config.max_delay_ms)
                    };

                    callback.on_retry(
                        attempt,
                        delay_ms,
                        "rate limited",
                    );
                    tokio::time::sleep(
                        std::time::Duration::from_millis(delay_ms),
                    )
                    .await;
                }
                Err(ccx_api::Error::Overloaded) => {
                    attempt += 1;
                    if attempt > self.retry_config.max_retries {
                        return Err(AgentLoopError::Api(
                            "API overloaded after max retries".into(),
                        ));
                    }

                    let delay_ms = self.retry_config.base_delay_ms
                        * 2u64.pow(attempt - 1);
                    let delay_ms =
                        delay_ms.min(self.retry_config.max_delay_ms);

                    callback.on_retry(attempt, delay_ms, "overloaded");
                    tokio::time::sleep(
                        std::time::Duration::from_millis(delay_ms),
                    )
                    .await;
                }
                Err(e) => {
                    return Err(AgentLoopError::Api(e.to_string()));
                }
            }
        }
    }

    /// Consume a stream of SSE events into pending blocks.
    async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<
                dyn futures::Stream<
                        Item = Result<StreamEvent, ccx_api::Error>,
                    > + Send,
            >,
        >,
        callback: &mut dyn AgentCallback,
    ) -> Result<
        (
            Vec<PendingBlock>,
            Option<StopReason>,
            Option<ccx_api::Usage>,
        ),
        AgentLoopError,
    > {
        let mut blocks: Vec<PendingBlock> = Vec::new();
        let mut stop_reason = None;
        let mut usage = None;

        while let Some(event) = stream.next().await {
            let event =
                event.map_err(|e| AgentLoopError::Api(e.to_string()))?;
            match event {
                StreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    while blocks.len() <= index {
                        blocks.push(PendingBlock::Other);
                    }
                    blocks[index] = match content_block {
                        ContentBlock::Text { text } => PendingBlock::Text(text),
                        ContentBlock::ToolUse { id, name, .. } => {
                            PendingBlock::ToolUse {
                                id,
                                name,
                                json_buf: String::new(),
                            }
                        }
                        ContentBlock::Thinking { thinking } => {
                            PendingBlock::Thinking(thinking)
                        }
                        _ => PendingBlock::Other,
                    };
                }
                StreamEvent::ContentBlockDelta { index, delta } => {
                    if index < blocks.len() {
                        match (&mut blocks[index], delta) {
                            (
                                PendingBlock::Text(buf),
                                Delta::TextDelta { text },
                            ) => {
                                buf.push_str(&text);
                                callback.on_text(&text);
                            }
                            (
                                PendingBlock::ToolUse { json_buf, .. },
                                Delta::InputJsonDelta { partial_json },
                            ) => {
                                json_buf.push_str(&partial_json);
                            }
                            (
                                PendingBlock::Thinking(buf),
                                Delta::ThinkingDelta { thinking },
                            ) => {
                                buf.push_str(&thinking);
                                callback.on_thinking(&thinking);
                            }
                            _ => {}
                        }
                    }
                }
                StreamEvent::MessageDelta {
                    delta,
                    usage: msg_usage,
                } => {
                    stop_reason = delta.stop_reason;
                    if let Some(u) = msg_usage {
                        usage = Some(u);
                    }
                }
                StreamEvent::Error { error } => {
                    return Err(AgentLoopError::Api(format!(
                        "[{}] {}",
                        error.error_type, error.message
                    )));
                }
                _ => {}
            }
        }

        Ok((blocks, stop_reason, usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccx_api::ClaudeClient;

    #[test]
    fn test_agent_loop_creation() {
        let client = ApiClient::Claude(ClaudeClient::new("test-key", "test-model"));
        let registry = ToolRegistry::new();
        let ctx = ToolContext::new(std::path::PathBuf::from("/tmp"));
        let agent = AgentLoop::new(client, registry, ctx, "system".into());
        assert!(agent.messages().is_empty());
        assert_eq!(agent.cost().api_calls, 0);
    }

    #[test]
    fn test_noop_callback() {
        let mut cb = NoopCallback;
        cb.on_text("test");
        cb.on_tool_start("tool", &serde_json::json!({}));
        cb.on_thinking("thinking...");
        cb.on_turn_complete(1, &CostTracker::new());
        cb.on_retry(1, 1000, "rate limit");
        // Should not panic.
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60_000);
    }

    #[test]
    fn test_agent_loop_set_max_turns() {
        let client = ApiClient::Claude(ClaudeClient::new("key", "model"));
        let registry = ToolRegistry::new();
        let ctx = ToolContext::new(std::path::PathBuf::from("/tmp"));
        let mut agent = AgentLoop::new(client, registry, ctx, "sys".into());
        agent.set_max_turns(10);
        // Verify it doesn't panic and internal state is set.
    }

    #[test]
    fn test_retry_config_custom() {
        let client = ApiClient::Claude(ClaudeClient::new("key", "model"));
        let registry = ToolRegistry::new();
        let ctx = ToolContext::new(std::path::PathBuf::from("/tmp"));
        let mut agent = AgentLoop::new(client, registry, ctx, "sys".into());
        agent.set_retry_config(RetryConfig {
            max_retries: 10,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
        });
        // Verify it doesn't panic.
    }
}

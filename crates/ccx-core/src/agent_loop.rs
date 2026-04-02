use ccx_api::{
    ApiClient, ContentBlock, Delta, InputMessage, MessageContent, MessageRequest, Role, StopReason,
    StreamEvent, ThinkingConfig,
};
use futures::StreamExt;
use log::{debug, error, trace, warn};

use crate::context::ToolContext;
use crate::cost::CostTracker;
use crate::hooks::{HookEvent, HookRegistry, run_hook};
use crate::tool::{ToolError, ToolRegistry, ToolResult};

/// Tracks a content block being streamed.
enum PendingBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        json_buf: String,
    },
    Thinking {
        text: String,
        signature: Option<String>,
    },
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
    thinking: Option<ThinkingConfig>,
    max_tokens: u32,
    max_budget_usd: Option<f64>,
    hook_registry: HookRegistry,
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
    #[error("budget limit reached (${0:.2} / ${1:.2}). Use --max-budget-usd to increase.")]
    BudgetExceeded(f64, f64),
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
            thinking: None,
            max_tokens: 16384,
            max_budget_usd: None,
            hook_registry: HookRegistry::new(),
        }
    }

    pub fn set_max_turns(&mut self, max: usize) {
        self.max_turns = max;
    }

    pub fn set_retry_config(&mut self, config: RetryConfig) {
        self.retry_config = config;
    }

    pub fn set_thinking(&mut self, config: ThinkingConfig) {
        self.thinking = Some(config);
    }

    pub fn set_max_tokens(&mut self, tokens: u32) {
        self.max_tokens = tokens;
    }

    pub fn set_max_budget_usd(&mut self, budget: f64) {
        self.max_budget_usd = Some(budget);
    }

    pub fn set_hook_registry(&mut self, registry: HookRegistry) {
        self.hook_registry = registry;
    }

    pub fn model(&self) -> &str {
        self.client.model()
    }

    pub fn set_model(&mut self, model: &str) {
        self.client.set_model(model);
    }

    pub fn set_client(&mut self, client: ApiClient) {
        self.client = client;
    }

    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }

    pub fn append_system_prompt(&mut self, text: &str) {
        self.system_prompt.push_str("\n\n");
        self.system_prompt.push_str(text);
    }

    pub fn messages(&self) -> &[InputMessage] {
        &self.messages
    }

    /// Replace conversation messages (for session resume).
    pub fn set_messages(&mut self, messages: Vec<InputMessage>) {
        self.messages = messages;
    }

    pub fn cost(&self) -> &CostTracker {
        &self.cost
    }

    /// Force compaction of conversation context.
    pub fn compact(&mut self) {
        self.compact_tool_results(2000);
        self.compact_messages();
    }

    /// Auto-compact if conversation exceeds token thresholds.
    fn maybe_compact(&mut self) {
        let total_chars: usize = self.messages.iter().map(Self::message_chars).sum();
        let estimated_tokens = (total_chars as f64 / 3.5).ceil() as usize;

        if estimated_tokens > ccx_compact::DEFAULT_THRESHOLD {
            self.compact_tool_results(2000);
            self.compact_messages();
        } else if estimated_tokens > 100_000 {
            self.compact_tool_results(5000);
        }
    }

    /// Truncate tool result content blocks exceeding max_chars.
    fn compact_tool_results(&mut self, max_chars: usize) {
        for msg in &mut self.messages {
            if let MessageContent::Blocks(blocks) = &mut msg.content {
                for block in blocks {
                    if let ContentBlock::ToolResult { content, .. } = block
                        && content.len() > max_chars
                    {
                        let total = content.len();
                        content.truncate(200);
                        content.push_str(&format!("... [truncated, {total} chars total]"));
                    }
                }
            }
        }
    }

    /// Structural compaction: keep last few messages, drop the rest.
    fn compact_messages(&mut self) {
        if self.messages.len() <= 6 {
            return;
        }
        let keep = 4;
        let drop_count = self.messages.len() - keep;
        let kept = self.messages.split_off(drop_count);
        self.messages.clear();

        self.messages.push(InputMessage {
            role: Role::User,
            content: MessageContent::Text(format!(
                "[Context compacted: {drop_count} earlier messages removed to save context]"
            )),
        });

        // If first kept message is user role, insert assistant bridge for proper alternation.
        let first_is_user = kept.first().is_some_and(|m| matches!(m.role, Role::User));
        if first_is_user {
            self.messages.push(InputMessage {
                role: Role::Assistant,
                content: MessageContent::Text(
                    "Understood, continuing from the recent context.".into(),
                ),
            });
        }

        self.messages.extend(kept);
    }

    fn message_chars(msg: &InputMessage) -> usize {
        match &msg.content {
            MessageContent::Text(t) => t.len(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                    ContentBlock::ToolResult { content, .. } => content.len(),
                    ContentBlock::Thinking { thinking, .. } => thinking.len(),
                })
                .sum(),
        }
    }

    /// Send a user message and run the agent loop until completion.
    pub async fn send_message(
        &mut self,
        user_text: &str,
        callback: &mut dyn AgentCallback,
    ) -> Result<String, AgentLoopError> {
        debug!(
            "Starting agent loop for user message (len: {})",
            user_text.len()
        );
        self.messages.push(InputMessage {
            role: Role::User,
            content: MessageContent::Text(user_text.to_string()),
        });

        let mut turn = 0;
        loop {
            if turn >= self.max_turns {
                error!("Agent loop exceeded maximum turns ({})", self.max_turns);
                return Err(AgentLoopError::MaxTurnsExceeded(self.max_turns));
            }
            turn += 1;

            debug!("Agent loop turn {}/{}", turn, self.max_turns);

            // Auto-compact if conversation is approaching context limits.
            self.maybe_compact();

            // Check budget before each API call.
            if let Some(budget) = self.max_budget_usd {
                let current = self.cost.estimated_cost_usd();
                if current >= budget {
                    warn!("Budget limit reached: ${:.2} / ${:.2}", current, budget);
                    return Err(AgentLoopError::BudgetExceeded(current, budget));
                }
            }

            let req = MessageRequest {
                model: String::new(),
                max_tokens: self.max_tokens,
                messages: self.messages.clone(),
                system: Some(self.system_prompt.clone()),
                temperature: None,
                tools: Some(self.registry.tool_definitions()),
                stream: Some(true),
                thinking: self.thinking.clone(),
            };

            // Execute with rate limit retry.
            let stream_result = self.stream_with_retry(req, callback).await?;

            let (blocks, stop_reason, usage) = stream_result;

            // Record usage for this turn.
            if let Some(usage) = &usage {
                debug!(
                    "Token usage: {} in, {} out",
                    usage.input_tokens, usage.output_tokens
                );
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
                    PendingBlock::Thinking {
                        text: thinking,
                        signature,
                    } => {
                        if !thinking.is_empty() {
                            content.push(ContentBlock::Thinking {
                                thinking,
                                signature,
                            });
                        }
                    }
                    PendingBlock::Other => {}
                }
            }

            self.messages.push(InputMessage {
                role: Role::Assistant,
                content: MessageContent::Blocks(content),
            });

            // Execute tool calls in parallel if the model requested them.
            if stop_reason == Some(StopReason::ToolUse) && !tool_calls.is_empty() {
                debug!("Executing {} tool calls", tool_calls.len());
                // Phase 1: Check permissions and fire PreTool hooks.
                let mut approved = Vec::new();
                let mut results = Vec::new();
                for (id, name, input) in tool_calls {
                    if !callback.should_allow_tool(&name, &input) {
                        debug!("Tool permission denied: {}", name);
                        results.push(ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: "Tool execution denied by user".to_string(),
                            is_error: Some(true),
                        });
                        continue;
                    }

                    // Fire PreTool hooks.
                    let pre_hooks =
                        self.hook_registry.matching(HookEvent::PreTool, Some(&name));
                    for hook in &pre_hooks {
                        debug!("Running PreTool hook for {}: {}", name, hook.command);
                        match run_hook(hook, &self.context.working_dir).await {
                            Ok(result) => {
                                if !result.stdout.is_empty() {
                                    debug!("PreTool hook stdout: {}", result.stdout.trim());
                                }
                                if !result.success {
                                    warn!(
                                        "PreTool hook failed for {}: {}",
                                        name,
                                        result.stderr.trim()
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("PreTool hook error for {}: {}", name, e);
                            }
                        }
                    }

                    debug!("Tool permission granted: {}", name);
                    callback.on_tool_start(&name, &input);
                    approved.push((id, name, input));
                }

                // Phase 2: Execute all approved tools in parallel.
                let futures: Vec<_> = approved
                    .iter()
                    .map(|(_, name, input)| {
                        self.registry.execute(name, input.clone(), &self.context)
                    })
                    .collect();
                let exec_results = futures::future::join_all(futures).await;

                // Phase 3: Collect results, fire PostTool hooks, and fire callbacks.
                for ((id, name, _), result) in approved.into_iter().zip(exec_results) {
                    // Fire PostTool hooks.
                    let post_hooks =
                        self.hook_registry.matching(HookEvent::PostTool, Some(&name));
                    for hook in &post_hooks {
                        debug!("Running PostTool hook for {}: {}", name, hook.command);
                        match run_hook(hook, &self.context.working_dir).await {
                            Ok(hr) => {
                                if !hr.stdout.is_empty() {
                                    debug!("PostTool hook stdout: {}", hr.stdout.trim());
                                }
                                if !hr.success {
                                    warn!(
                                        "PostTool hook failed for {}: {}",
                                        name,
                                        hr.stderr.trim()
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("PostTool hook error for {}: {}", name, e);
                            }
                        }
                    }

                    callback.on_tool_end(&name, &result);
                    let (tool_content, is_error) = match result {
                        Ok(r) => (r.content, r.is_error),
                        Err(e) => (e.to_string(), true),
                    };
                    if is_error {
                        warn!("Tool {} failed: {}", name, tool_content);
                    } else {
                        debug!("Tool {} succeeded", name);
                    }
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

            debug!("Agent loop completed after {} turns", turn);
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
        debug!(
            "Starting stream_with_retry (max_retries: {})",
            self.retry_config.max_retries
        );
        let mut attempt = 0;

        loop {
            match self.client.stream_message(req.clone()).await {
                Ok(stream) => {
                    debug!("Successfully connected to API stream");
                    return self.consume_stream(stream, callback).await;
                }
                Err(ccx_api::Error::RateLimit { retry_after_secs }) => {
                    attempt += 1;
                    warn!(
                        "Rate limited (attempt {}/{}): retry after {}s",
                        attempt,
                        self.retry_config.max_retries,
                        retry_after_secs
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                    if attempt > self.retry_config.max_retries {
                        error!("Rate limit exhausted after {} attempts", attempt);
                        return Err(AgentLoopError::RateLimitExhausted(attempt));
                    }

                    let delay_ms = if let Some(secs) = retry_after_secs {
                        secs * 1000
                    } else {
                        // Exponential backoff with jitter.
                        let base = self.retry_config.base_delay_ms * 2u64.pow(attempt - 1);
                        base.min(self.retry_config.max_delay_ms)
                    };

                    callback.on_retry(attempt, delay_ms, "rate limited");
                    debug!("Sleeping for {}ms before retry", delay_ms);
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                Err(ccx_api::Error::Overloaded) => {
                    attempt += 1;
                    warn!(
                        "API overloaded (attempt {}/{}): backing off",
                        attempt, self.retry_config.max_retries
                    );
                    if attempt > self.retry_config.max_retries {
                        error!("API overloaded after max retries");
                        return Err(AgentLoopError::Api(
                            "API overloaded after max retries".into(),
                        ));
                    }

                    let delay_ms = self.retry_config.base_delay_ms * 2u64.pow(attempt - 1);
                    let delay_ms = delay_ms.min(self.retry_config.max_delay_ms);

                    callback.on_retry(attempt, delay_ms, "overloaded");
                    debug!("Sleeping for {}ms before retry", delay_ms);
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                Err(e) => {
                    error!("API error: {}", e);
                    return Err(AgentLoopError::Api(e.to_string()));
                }
            }
        }
    }

    /// Consume a stream of SSE events into pending blocks.
    async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamEvent, ccx_api::Error>> + Send>,
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
        debug!("Starting to consume API stream");
        let mut blocks: Vec<PendingBlock> = Vec::new();
        let mut stop_reason = None;
        let mut usage = None;
        let mut text_chunks = 0;
        let mut tool_calls_started = 0;
        let mut thinking_chunks = 0;

        while let Some(event) = stream.next().await {
            let event = event.map_err(|e| AgentLoopError::Api(e.to_string()))?;
            match event {
                StreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    while blocks.len() <= index {
                        blocks.push(PendingBlock::Other);
                    }
                    match &content_block {
                        ContentBlock::ToolUse { name, .. } => {
                            debug!("Tool use started: {} (index {})", name, index);
                            tool_calls_started += 1;
                        }
                        ContentBlock::Thinking { thinking, .. } => {
                            if !thinking.is_empty() {
                                debug!(
                                    "Thinking block started (index {}): {}...",
                                    index,
                                    &thinking[..50.min(thinking.len())]
                                );
                                thinking_chunks += 1;
                            }
                        }
                        ContentBlock::Text { text } => {
                            if !text.is_empty() {
                                debug!(
                                    "Text block started (index {}): {}...",
                                    index,
                                    &text[..50.min(text.len())]
                                );
                                text_chunks += 1;
                            }
                        }
                        _ => {}
                    }
                    blocks[index] = match content_block {
                        ContentBlock::Text { text } => PendingBlock::Text(text),
                        ContentBlock::ToolUse { id, name, .. } => PendingBlock::ToolUse {
                            id,
                            name,
                            json_buf: String::new(),
                        },
                        ContentBlock::Thinking { thinking, .. } => PendingBlock::Thinking {
                            text: thinking,
                            signature: None,
                        },
                        _ => PendingBlock::Other,
                    };
                }
                StreamEvent::ContentBlockDelta { index, delta } => {
                    if index < blocks.len() {
                        match (&mut blocks[index], delta) {
                            (PendingBlock::Text(buf), Delta::TextDelta { text }) => {
                                if !text.is_empty() {
                                    trace!("Text delta: {}...", &text[..20.min(text.len())]);
                                    buf.push_str(&text);
                                    callback.on_text(&text);
                                }
                            }
                            (
                                PendingBlock::ToolUse { json_buf, .. },
                                Delta::InputJsonDelta { partial_json },
                            ) => {
                                if !partial_json.is_empty() {
                                    trace!(
                                        "Tool input delta: {}...",
                                        &partial_json[..20.min(partial_json.len())]
                                    );
                                    json_buf.push_str(&partial_json);
                                }
                            }
                            (
                                PendingBlock::Thinking { text: buf, .. },
                                Delta::ThinkingDelta { thinking },
                            ) => {
                                if !thinking.is_empty() {
                                    trace!(
                                        "Thinking delta: {}...",
                                        &thinking[..20.min(thinking.len())]
                                    );
                                    buf.push_str(&thinking);
                                    callback.on_thinking(&thinking);
                                }
                            }
                            (
                                PendingBlock::Thinking { signature: sig, .. },
                                Delta::SignatureDelta { signature },
                            ) => {
                                trace!("Signature delta received");
                                if let Some(ref mut existing) = *sig {
                                    existing.push_str(&signature);
                                } else {
                                    *sig = Some(signature);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                StreamEvent::MessageDelta {
                    delta,
                    usage: msg_usage,
                } => {
                    debug!("Message delta: stop_reason={:?}", delta.stop_reason);
                    stop_reason = delta.stop_reason;
                    if let Some(u) = msg_usage {
                        debug!("Usage: {} in, {} out", u.input_tokens, u.output_tokens);
                        usage = Some(u);
                    }
                }
                StreamEvent::Error { error } => {
                    error!("Stream error: [{}] {}", error.error_type, error.message);
                    return Err(AgentLoopError::Api(format!(
                        "[{}] {}",
                        error.error_type, error.message
                    )));
                }
                _ => {}
            }
        }

        debug!(
            "Stream consumed: {} text chunks, {} tool calls, {} thinking chunks",
            text_chunks, tool_calls_started, thinking_chunks
        );
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

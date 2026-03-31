use ccx_api::{
    ClaudeClient, ContentBlock, Delta, InputMessage, MessageContent, MessageRequest, Role,
    StopReason, StreamEvent,
};
use futures::StreamExt;

use crate::context::ToolContext;
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

/// The main agent loop: message -> API -> tool_use -> execute -> loop.
pub struct AgentLoop {
    client: ClaudeClient,
    registry: ToolRegistry,
    context: ToolContext,
    system_prompt: String,
    messages: Vec<InputMessage>,
    max_turns: usize,
}

/// Callback for streaming events.
pub trait AgentCallback: Send {
    fn on_text(&mut self, _text: &str) {}
    fn on_tool_start(&mut self, _name: &str, _input: &serde_json::Value) {}
    fn on_tool_end(&mut self, _name: &str, _result: &Result<ToolResult, ToolError>) {}
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
}

impl AgentLoop {
    pub fn new(
        client: ClaudeClient,
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
        }
    }

    pub fn set_max_turns(&mut self, max: usize) {
        self.max_turns = max;
    }

    pub fn messages(&self) -> &[InputMessage] {
        &self.messages
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
                max_tokens: 8192,
                messages: self.messages.clone(),
                system: Some(self.system_prompt.clone()),
                temperature: None,
                tools: Some(self.registry.tool_definitions()),
                stream: Some(true),
            };

            let mut stream = self
                .client
                .stream_message(req)
                .await
                .map_err(|e| AgentLoopError::Api(e.to_string()))?;

            let mut blocks: Vec<PendingBlock> = Vec::new();
            let mut stop_reason = None;

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
                        blocks[index] = match content_block {
                            ContentBlock::Text { text } => PendingBlock::Text(text),
                            ContentBlock::ToolUse { id, name, .. } => PendingBlock::ToolUse {
                                id,
                                name,
                                json_buf: String::new(),
                            },
                            ContentBlock::Thinking { thinking } => {
                                PendingBlock::Thinking(thinking)
                            }
                            _ => PendingBlock::Other,
                        };
                    }
                    StreamEvent::ContentBlockDelta { index, delta } => {
                        if index < blocks.len() {
                            match (&mut blocks[index], delta) {
                                (PendingBlock::Text(buf), Delta::TextDelta { text }) => {
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
                                }
                                _ => {}
                            }
                        }
                    }
                    StreamEvent::MessageDelta { delta, .. } => {
                        stop_reason = delta.stop_reason;
                    }
                    _ => {}
                }
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
                    callback.on_tool_start(&name, &input);
                    let result = self.registry.execute(&name, input, &self.context).await;
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
                continue;
            }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_loop_creation() {
        let client = ClaudeClient::new("test-key", "test-model");
        let registry = ToolRegistry::new();
        let ctx = ToolContext::new(std::path::PathBuf::from("/tmp"));
        let agent = AgentLoop::new(client, registry, ctx, "system".into());
        assert!(agent.messages().is_empty());
    }

    #[test]
    fn test_noop_callback() {
        let mut cb = NoopCallback;
        cb.on_text("test");
        cb.on_tool_start("tool", &serde_json::json!({}));
        // Should not panic.
    }
}

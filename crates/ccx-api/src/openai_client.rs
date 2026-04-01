//! OpenAI-compatible API client for OpenRouter and similar services.
//!
//! Converts between Anthropic message format and OpenAI format transparently,
//! so the agent loop works unchanged regardless of provider.

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;

use futures::stream::{self, BoxStream, Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::client::byte_stream_to_lines;
use crate::error::Error;
use crate::types::{
    ContentBlock, Delta, InputMessage, MessageContent, MessageDelta, MessageRequest, Role,
    StopReason, StreamEvent, Tool, Usage,
};

/// OpenAI-compatible API client (works with OpenRouter, Together, etc.)
pub struct OpenAiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiClient {
    /// Create a client configured for OpenRouter.
    pub fn openrouter(api_key: &str, model: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.trim().to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: model.trim().to_string(),
        }
    }

    /// Returns the configured model.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a streaming message request, converting from Anthropic format internally.
    pub async fn stream_message(
        &self,
        req: MessageRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, Error>> + Send>>, Error> {
        let messages = convert_messages(&req.messages, req.system.as_deref());
        let tools = req.tools.as_ref().map(|t| convert_tools(t));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "max_tokens": req.max_tokens,
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::to_value(tools).unwrap_or_default();
            }
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|e| Error::InvalidHeader(format!("Authorization: {e}")))?;
        headers.insert("Authorization", auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://github.com/anton-abyzov/ccx-rs"),
        );
        headers.insert("X-Title", HeaderValue::from_static("CCX-RS"));

        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Auth("invalid OpenRouter API key".into()));
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            return Err(Error::RateLimit {
                retry_after_secs: retry_after,
            });
        }
        if status.as_u16() == 529 || status == StatusCode::SERVICE_UNAVAILABLE {
            return Err(Error::Overloaded);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let byte_stream = response.bytes_stream();
        Ok(openai_sse_to_events(byte_stream))
    }
}

// ── OpenAI request types ────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCallOut>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OpenAiToolCallOut {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Serialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAiToolDef {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunctionDef,
}

#[derive(Serialize)]
struct OpenAiFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── OpenAI response types (streaming) ──────────────────────────────

#[derive(Deserialize)]
struct OpenAiChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
    #[allow(dead_code)]
    role: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Deserialize)]
struct OpenAiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// ── Message conversion (Anthropic → OpenAI) ─────────────────────

fn convert_messages(messages: &[InputMessage], system: Option<&str>) -> Vec<OpenAiMessage> {
    let mut result = Vec::new();

    if let Some(sys) = system {
        result.push(OpenAiMessage {
            role: "system".into(),
            content: Some(sys.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    for msg in messages {
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        match &msg.content {
            MessageContent::Text(text) => {
                result.push(OpenAiMessage {
                    role: role_str.into(),
                    content: Some(text.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            MessageContent::Blocks(blocks) => match msg.role {
                Role::User => {
                    let mut text_parts = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                if !text_parts.is_empty() {
                                    result.push(OpenAiMessage {
                                        role: "user".into(),
                                        content: Some(text_parts.join("\n")),
                                        tool_calls: None,
                                        tool_call_id: None,
                                    });
                                    text_parts.clear();
                                }
                                result.push(OpenAiMessage {
                                    role: "tool".into(),
                                    content: Some(content.clone()),
                                    tool_calls: None,
                                    tool_call_id: Some(tool_use_id.clone()),
                                });
                            }
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            _ => {}
                        }
                    }
                    if !text_parts.is_empty() {
                        result.push(OpenAiMessage {
                            role: "user".into(),
                            content: Some(text_parts.join("\n")),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                Role::Assistant => {
                    let mut text_content = String::new();
                    let mut tool_calls = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_content.push_str(text);
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OpenAiToolCallOut {
                                    id: id.clone(),
                                    call_type: "function".into(),
                                    function: OpenAiFunctionCall {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input)
                                            .unwrap_or_default(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    result.push(OpenAiMessage {
                        role: "assistant".into(),
                        content: if text_content.is_empty() {
                            None
                        } else {
                            Some(text_content)
                        },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        tool_call_id: None,
                    });
                }
            },
        }
    }

    result
}

fn convert_tools(tools: &[Tool]) -> Vec<OpenAiToolDef> {
    tools
        .iter()
        .map(|t| OpenAiToolDef {
            tool_type: "function".into(),
            function: OpenAiFunctionDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

// ── Stream conversion (OpenAI SSE → Anthropic StreamEvent) ──────

struct OpenAiStreamState {
    lines: BoxStream<'static, Result<String, reqwest::Error>>,
    data_buf: String,
    text_started: bool,
    tool_blocks: HashMap<usize, usize>,
    next_idx: usize,
    pending: VecDeque<Result<StreamEvent, Error>>,
}

fn openai_sse_to_events(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> Pin<Box<dyn Stream<Item = Result<StreamEvent, Error>> + Send>> {
    let state = OpenAiStreamState {
        lines: byte_stream_to_lines(byte_stream).boxed(),
        data_buf: String::new(),
        text_started: false,
        tool_blocks: HashMap::new(),
        next_idx: 0,
        pending: VecDeque::new(),
    };

    Box::pin(stream::unfold(state, |mut s| async move {
        // Drain pending events first.
        if let Some(event) = s.pending.pop_front() {
            return Some((event, s));
        }

        loop {
            match s.lines.next().await {
                None => return None,
                Some(Err(e)) => return Some((Err(Error::Http(e)), s)),
                Some(Ok(line)) => {
                    if line.is_empty() {
                        if !s.data_buf.is_empty() {
                            let data = std::mem::take(&mut s.data_buf);
                            if data == "[DONE]" {
                                return None;
                            }
                            match serde_json::from_str::<OpenAiChunk>(&data) {
                                Ok(chunk) => {
                                    process_chunk(chunk, &mut s);
                                    if let Some(event) = s.pending.pop_front() {
                                        return Some((event, s));
                                    }
                                }
                                Err(e) => {
                                    return Some((
                                        Err(Error::SseParse(format!("{e}: {data}"))),
                                        s,
                                    ));
                                }
                            }
                        }
                    } else if let Some(data) = line.strip_prefix("data: ") {
                        if !s.data_buf.is_empty() {
                            s.data_buf.push('\n');
                        }
                        s.data_buf.push_str(data);
                    }
                }
            }
        }
    }))
}

fn process_chunk(chunk: OpenAiChunk, state: &mut OpenAiStreamState) {
    let usage_copy = chunk.usage;

    if chunk.choices.is_empty() {
        if let Some(usage) = usage_copy {
            state.pending.push_back(Ok(StreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: None,
                },
                usage: Some(Usage {
                    input_tokens: usage.prompt_tokens,
                    output_tokens: usage.completion_tokens,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                }),
            }));
        }
        return;
    }

    let choice = &chunk.choices[0];

    // Text content.
    if let Some(text) = &choice.delta.content {
        if !text.is_empty() {
            if !state.text_started {
                state.text_started = true;
                state.pending.push_back(Ok(StreamEvent::ContentBlockStart {
                    index: state.next_idx,
                    content_block: ContentBlock::Text {
                        text: String::new(),
                    },
                }));
                state.next_idx += 1;
            }
            state.pending.push_back(Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: Delta::TextDelta { text: text.clone() },
            }));
        }
    }

    // Tool calls.
    if let Some(tool_calls) = &choice.delta.tool_calls {
        for tc in tool_calls {
            let openai_idx = tc.index.unwrap_or(0);

            if !state.tool_blocks.contains_key(&openai_idx) {
                let anthropic_idx = state.next_idx;
                state.tool_blocks.insert(openai_idx, anthropic_idx);
                state.next_idx += 1;

                let id = tc
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("toolu_{openai_idx}"));
                let name = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .unwrap_or_default();

                state.pending.push_back(Ok(StreamEvent::ContentBlockStart {
                    index: anthropic_idx,
                    content_block: ContentBlock::ToolUse {
                        id,
                        name,
                        input: serde_json::Value::Object(Default::default()),
                    },
                }));
            }

            if let Some(func) = &tc.function {
                if let Some(args) = &func.arguments {
                    if !args.is_empty() {
                        let anthropic_idx = state.tool_blocks[&openai_idx];
                        state
                            .pending
                            .push_back(Ok(StreamEvent::ContentBlockDelta {
                                index: anthropic_idx,
                                delta: Delta::InputJsonDelta {
                                    partial_json: args.clone(),
                                },
                            }));
                    }
                }
            }
        }
    }

    // Finish reason.
    if let Some(reason) = &choice.finish_reason {
        let stop_reason = match reason.as_str() {
            "stop" => Some(StopReason::EndTurn),
            "tool_calls" => Some(StopReason::ToolUse),
            "length" => Some(StopReason::MaxTokens),
            _ => Some(StopReason::EndTurn),
        };
        state.pending.push_back(Ok(StreamEvent::MessageDelta {
            delta: MessageDelta { stop_reason },
            usage: usage_copy.map(|u| Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            }),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_client_creation() {
        let client = OpenAiClient::openrouter("test-key", "deepseek/deepseek-r1-0528:free");
        assert_eq!(client.model(), "deepseek/deepseek-r1-0528:free");
    }

    #[test]
    fn test_convert_simple_messages() {
        let messages = vec![InputMessage {
            role: Role::User,
            content: MessageContent::Text("Hello".into()),
        }];
        let converted = convert_messages(&messages, Some("You are helpful"));
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[1].content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![Tool {
            name: "Bash".into(),
            description: "Run commands".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        let converted = super::convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].tool_type, "function");
        assert_eq!(converted[0].function.name, "Bash");
    }

    #[test]
    fn test_convert_assistant_with_tool_use() {
        let messages = vec![InputMessage {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Let me check".into(),
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "Bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                },
            ]),
        }];
        let converted = convert_messages(&messages, None);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "assistant");
        assert!(converted[0].tool_calls.is_some());
        assert_eq!(converted[0].tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_convert_tool_result() {
        let messages = vec![InputMessage {
            role: Role::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: "file.txt".into(),
                is_error: None,
            }]),
        }];
        let converted = convert_messages(&messages, None);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "tool");
        assert_eq!(converted[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[tokio::test]
    async fn test_openai_sse_parsing() {
        let raw = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Four\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        let byte_stream = futures::stream::iter(vec![Ok(bytes::Bytes::from(raw))]);
        let mut event_stream = openai_sse_to_events(byte_stream);
        let mut events = Vec::new();
        while let Some(event) = event_stream.next().await {
            events.push(event.unwrap());
        }

        // Should have: ContentBlockStart, ContentBlockDelta("Four"), MessageDelta(EndTurn)
        assert!(events.len() >= 2);
        assert!(matches!(events.last(), Some(StreamEvent::MessageDelta { .. })));
    }
}

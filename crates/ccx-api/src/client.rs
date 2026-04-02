use std::pin::Pin;

use futures::stream::{self, BoxStream, Stream, StreamExt};
use reqwest::StatusCode;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};

use crate::error::Error;
use crate::types::{MessageRequest, StreamEvent};

const API_BASE: &str = "https://api.anthropic.com/v1";
const API_VERSION: &str = "2023-06-01";

/// Claude API client for streaming message requests.
pub struct ClaudeClient {
    http: reqwest::Client,
    api_key: String,
    use_oauth: bool,
    model: String,
}

impl ClaudeClient {
    /// Create a new client with the given API key and default model.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            use_oauth: false,
            model: model.into(),
        }
    }

    /// Create a client from a resolved AuthMethod.
    pub fn with_auth(auth: &ccx_auth::AuthMethod, model: impl Into<String>) -> Self {
        match auth {
            ccx_auth::AuthMethod::ApiKey(resolved) => Self {
                http: reqwest::Client::new(),
                api_key: resolved.key.clone(),
                use_oauth: false,
                model: model.into(),
            },
            ccx_auth::AuthMethod::OAuthToken { access_token, .. } => Self {
                http: reqwest::Client::new(),
                api_key: access_token.clone(),
                use_oauth: true,
                model: model.into(),
            },
            ccx_auth::AuthMethod::None => Self {
                http: reqwest::Client::new(),
                api_key: String::new(),
                use_oauth: false,
                model: model.into(),
            },
        }
    }

    /// Returns the configured model.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Change the model at runtime.
    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    /// Send a streaming message request and return a stream of events.
    pub async fn stream_message(
        &self,
        mut req: MessageRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, Error>> + Send>>, Error> {
        // Override model and ensure streaming is on.
        req.model = self.model.clone();
        req.stream = Some(true);

        // Build beta features list.
        let mut betas = vec!["prompt-caching-2024-07-31"];
        if self.use_oauth {
            betas.push("oauth-2025-04-20");
        }
        // Extended thinking graduated to GA — no beta header needed.

        let mut headers = HeaderMap::new();
        // Collect beta headers — OAuth requires an additional beta flag.
        let mut all_betas: Vec<&str> = betas.to_vec();
        if self.use_oauth {
            headers.insert(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {}", self.api_key)).unwrap(),
            );
            // OAuth tokens REQUIRE this beta header — without it the API rejects/rate-limits
            all_betas.push("oauth-2025-04-20");
        } else {
            headers.insert("x-api-key", HeaderValue::from_str(&self.api_key).unwrap());
        }
        headers.insert("anthropic-version", HeaderValue::from_static(API_VERSION));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !all_betas.is_empty() {
            headers.insert(
                "anthropic-beta",
                HeaderValue::from_str(&all_betas.join(",")).unwrap(),
            );
        }

        // Build JSON body with structured system prompt for cache_control.
        let mut body = serde_json::to_value(&req).unwrap_or_default();
        if let Some(system_text) = &req.system {
            body["system"] = serde_json::json!([{
                "type": "text",
                "text": system_text,
                "cache_control": {"type": "ephemeral"}
            }]);
        }
        // Strip temperature when thinking is enabled (API requirement).
        if req.thinking.is_some() {
            body.as_object_mut().map(|o| o.remove("temperature"));
        }

        let response = self
            .http
            .post(format!("{API_BASE}/messages"))
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Auth("invalid API key".into()));
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
        if status == StatusCode::from_u16(529).unwrap() {
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

        Ok(Box::pin(parse_sse_stream(byte_stream)))
    }
}

/// Parse a byte stream of SSE into typed StreamEvents.
fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<StreamEvent, Error>> + Send {
    let line_stream: BoxStream<'static, Result<String, reqwest::Error>> =
        byte_stream_to_lines(byte_stream).boxed();

    stream::unfold(
        (line_stream, String::new(), String::new()),
        |(mut lines, mut event_type, mut data_buf)| async move {
            loop {
                match lines.next().await {
                    None => return None,
                    Some(Err(e)) => {
                        return Some((Err(Error::Http(e)), (lines, event_type, data_buf)));
                    }
                    Some(Ok(line)) => {
                        if line.is_empty() {
                            // End of event — dispatch if we have data.
                            if !data_buf.is_empty() {
                                let result = serde_json::from_str::<StreamEvent>(&data_buf)
                                    .map_err(|e| Error::SseParse(format!("{e}: {data_buf}")));
                                event_type.clear();
                                data_buf.clear();
                                return Some((result, (lines, event_type, data_buf)));
                            }
                            continue;
                        }
                        if let Some(stripped) = line.strip_prefix("event: ") {
                            event_type = stripped.to_string();
                        } else if let Some(stripped) = line.strip_prefix("data: ") {
                            if !data_buf.is_empty() {
                                data_buf.push('\n');
                            }
                            data_buf.push_str(stripped);
                        }
                        // Ignore other SSE fields (id:, retry:, comments).
                    }
                }
            }
        },
    )
}

/// Convert a byte stream into a stream of lines.
pub(crate) fn byte_stream_to_lines(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<String, reqwest::Error>> + Send {
    let boxed: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>> = byte_stream.boxed();

    stream::unfold((boxed, String::new()), |(mut bytes, mut buf)| async move {
        loop {
            if let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim_end_matches('\r').to_string();
                buf = buf[pos + 1..].to_string();
                return Some((Ok(line), (bytes, buf)));
            }
            match bytes.next().await {
                None => {
                    if buf.is_empty() {
                        return None;
                    }
                    let line = std::mem::take(&mut buf);
                    return Some((Ok(line), (bytes, buf)));
                }
                Some(Err(e)) => return Some((Err(e), (bytes, buf))),
                Some(Ok(chunk)) => {
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn test_client_new() {
        let client = ClaudeClient::new("test-key", "claude-sonnet-4-6");
        assert_eq!(client.model(), "claude-sonnet-4-6");
    }

    #[tokio::test]
    async fn test_sse_parsing() {
        let raw = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"usage\":null}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

        let byte_stream = stream::iter(vec![Ok(bytes::Bytes::from(raw))]);
        let mut event_stream = Box::pin(parse_sse_stream(byte_stream));
        let mut events = Vec::new();
        while let Some(event) = event_stream.next().await {
            events.push(event.unwrap());
        }

        assert_eq!(events.len(), 6);
        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        assert!(matches!(events[1], StreamEvent::ContentBlockStart { .. }));
        assert!(matches!(events[2], StreamEvent::ContentBlockDelta { .. }));
        assert!(matches!(events[3], StreamEvent::ContentBlockStop { .. }));
        assert!(matches!(events[4], StreamEvent::MessageDelta { .. }));
        assert!(matches!(events[5], StreamEvent::MessageStop));
    }

    #[test]
    fn test_message_request_serialization() {
        use crate::types::{InputMessage, MessageContent, Role};

        let req = MessageRequest {
            model: "claude-sonnet-4-6".into(),
            max_tokens: 1024,
            messages: vec![InputMessage {
                role: Role::User,
                content: MessageContent::Text("Hello".into()),
            }],
            system: None,
            temperature: None,
            tools: None,
            stream: Some(true),
            thinking: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "claude-sonnet-4-6");
        assert_eq!(json["max_tokens"], 1024);
        assert!(json.get("system").is_none());
    }
}

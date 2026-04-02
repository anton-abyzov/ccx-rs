use crate::types::ApiErrorBody;

/// Errors from the Claude API client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("rate limited{}", match retry_after_secs {
        Some(s) => format!(" (retry after {s}s)"),
        None => String::new(),
    })]
    RateLimit { retry_after_secs: Option<u64> },

    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("API returned error event: [{error_type}] {message}")]
    ApiEvent { error_type: String, message: String },

    #[error("request overloaded, please retry")]
    Overloaded,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SSE parse error: {0}")]
    SseParse(String),

    #[error("invalid header value: {0}")]
    InvalidHeader(String),
}

impl From<ApiErrorBody> for Error {
    fn from(body: ApiErrorBody) -> Self {
        Error::ApiEvent {
            error_type: body.error_type,
            message: body.message,
        }
    }
}

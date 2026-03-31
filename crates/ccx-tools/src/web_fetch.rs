use std::time::Duration;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

/// Default timeout for HTTP requests.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default maximum response body size (5 MB).
const DEFAULT_MAX_BODY_SIZE: usize = 5 * 1024 * 1024;

/// Maximum redirect hops.
const MAX_REDIRECTS: usize = 10;

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return the content as text"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 30)"
                },
                "max_size": {
                    "type": "integer",
                    "description": "Max response body size in bytes (default 5MB)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("url is required".into()))?;

        let timeout_secs = input["timeout"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        let max_size = input["max_size"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_BODY_SIZE as u64) as usize;

        // Build client with timeout, redirect policy, and user-agent.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .user_agent("ccx/0.1 (Claude Code Rust)")
            .build()
            .map_err(|e| ToolError::Execution(format!("HTTP client error: {e}")))?;

        let response = client.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                ToolError::Timeout(timeout_secs * 1000)
            } else if e.is_redirect() {
                ToolError::Execution(format!(
                    "too many redirects (>{MAX_REDIRECTS}) for {url}"
                ))
            } else {
                ToolError::Execution(format!("fetch failed: {e}"))
            }
        })?;

        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !status.is_success() {
            return Ok(ToolResult {
                content: format!("HTTP {status}"),
                is_error: true,
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::Execution(format!("failed to read body: {e}")))?;

        // Truncate if over max size.
        let mut truncated = false;
        let body = if body.len() > max_size {
            truncated = true;
            // Truncate at a UTF-8 boundary.
            let mut end = max_size;
            while end > 0 && !body.is_char_boundary(end) {
                end -= 1;
            }
            body[..end].to_string()
        } else {
            body
        };

        // Process based on content type.
        let text = if is_html_content_type(&content_type) {
            strip_html(&body)
        } else {
            body
        };

        let mut result = text;
        if truncated {
            result.push_str("\n\n[Response truncated — exceeded max size]");
        }

        Ok(ToolResult {
            content: result,
            is_error: false,
        })
    }
}

/// Check if a content-type header indicates HTML.
fn is_html_content_type(ct: &str) -> bool {
    ct.contains("text/html") || ct.contains("application/xhtml")
}

/// Strip HTML tags with script/style block removal and whitespace normalization.
pub fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_tag = false;

    while i < len {
        // Check for <script...> or <style...> blocks and skip them entirely.
        if i + 7 < len && chars[i] == '<' {
            let remaining: String = lower_chars[i..].iter().take(8).collect();
            let (is_skip, end_tag) = if remaining.starts_with("<script") {
                (true, "</script>")
            } else if remaining.starts_with("<style") {
                (true, "</style>")
            } else {
                (false, "")
            };

            if is_skip {
                let rest: String = lower_chars[i..].iter().collect();
                if let Some(end_pos) = rest.find(end_tag) {
                    i += end_pos + end_tag.len();
                    continue;
                } else {
                    break; // Malformed — skip rest.
                }
            }
        }

        let c = chars[i];
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
        i += 1;
    }

    // Normalize whitespace: collapse runs of whitespace, preserve paragraph breaks.
    let mut normalized = String::with_capacity(result.len());
    let mut consecutive_newlines = 0u32;
    let mut prev_was_space = false;

    for c in result.chars() {
        if c == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                normalized.push('\n');
            }
            prev_was_space = false;
        } else if c.is_whitespace() {
            if !prev_was_space && consecutive_newlines == 0 {
                normalized.push(' ');
                prev_was_space = true;
            }
        } else {
            normalized.push(c);
            prev_was_space = false;
            consecutive_newlines = 0;
        }
    }

    normalized.trim().to_string()
}

/// Decode common HTML entities.
#[allow(dead_code)]
fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn test_strip_html_whitespace() {
        let html = "<div>  hello   world  </div>";
        let text = strip_html(html);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_strip_html_script_removal() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = strip_html(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("xss"));
    }

    #[test]
    fn test_strip_html_style_removal() {
        let html = "<style>.red{color:red}</style><p>Visible</p>";
        let text = strip_html(html);
        assert!(text.contains("Visible"));
        assert!(!text.contains("red"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn test_strip_html_nested_script() {
        let html = "<div>Keep<script type=\"text/javascript\">var x = 1; // remove</script>This</div>";
        let text = strip_html(html);
        assert!(text.contains("Keep"));
        assert!(text.contains("This"));
        assert!(!text.contains("var x"));
    }

    #[test]
    fn test_strip_html_preserves_paragraph_breaks() {
        let html = "<p>Para 1</p>\n\n<p>Para 2</p>";
        let text = strip_html(html);
        assert!(text.contains("Para 1"));
        assert!(text.contains("Para 2"));
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn test_strip_html_no_tags() {
        assert_eq!(strip_html("plain text"), "plain text");
    }

    #[test]
    fn test_is_html_content_type() {
        assert!(is_html_content_type("text/html; charset=utf-8"));
        assert!(is_html_content_type("application/xhtml+xml"));
        assert!(!is_html_content_type("application/json"));
        assert!(!is_html_content_type("text/plain"));
    }

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(
            decode_html_entities("&lt;div&gt;&amp;hello&quot;"),
            "<div>&hello\""
        );
    }
}

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

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

        let response = reqwest::get(url)
            .await
            .map_err(|e| ToolError::Execution(format!("fetch failed: {e}")))?;

        let status = response.status();
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

        let text = strip_html_tags(&body);

        Ok(ToolResult {
            content: text,
            is_error: false,
        })
    }
}

/// Naive HTML tag stripper.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    // Collapse whitespace.
    let mut collapsed = String::with_capacity(result.len());
    let mut prev_was_space = false;
    for c in result.chars() {
        if c.is_whitespace() {
            if !prev_was_space {
                collapsed.push(' ');
                prev_was_space = true;
            }
        } else {
            collapsed.push(c);
            prev_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn test_strip_html_whitespace() {
        let html = "<div>  hello   world  </div>";
        let text = strip_html_tags(html);
        assert_eq!(text, "hello world");
    }
}

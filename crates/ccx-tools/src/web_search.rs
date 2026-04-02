use std::time::Duration;

use async_trait::async_trait;
use ccx_core::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::json;

/// Default search timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 15;

/// Max response body size for search results (1 MB).
const MAX_BODY_SIZE: usize = 1024 * 1024;

pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web and return results"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("query is required".into()))?;

        // Try Brave Search API first if key is available.
        if let Ok(brave_key) = std::env::var("BRAVE_SEARCH_API_KEY") {
            return brave_search(query, &brave_key).await;
        }

        // Fall back to a simple DuckDuckGo HTML scrape.
        duckduckgo_search(query).await
    }
}

/// Search using Brave Search API.
async fn brave_search(query: &str, api_key: &str) -> Result<ToolResult, ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .user_agent("ccx/0.1 (Claude Code Extended)")
        .build()
        .map_err(|e| ToolError::Execution(format!("HTTP client error: {e}")))?;

    let response = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .query(&[("q", query), ("count", "10")])
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ToolError::Timeout(DEFAULT_TIMEOUT_SECS * 1000)
            } else {
                ToolError::Execution(format!("search failed: {e}"))
            }
        })?;

    if !response.status().is_success() {
        return Ok(ToolResult {
            content: format!("Brave Search API error: HTTP {}", response.status()),
            is_error: true,
        });
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to parse response: {e}")))?;

    let mut results = Vec::new();
    if let Some(web) = body["web"]["results"].as_array() {
        for item in web.iter().take(10) {
            let title = item["title"].as_str().unwrap_or("");
            let url = item["url"].as_str().unwrap_or("");
            let description = item["description"].as_str().unwrap_or("");
            results.push(format!("**{title}**\n{url}\n{description}\n"));
        }
    }

    if results.is_empty() {
        Ok(ToolResult {
            content: format!("No results found for: {query}"),
            is_error: false,
        })
    } else {
        Ok(ToolResult {
            content: results.join("\n"),
            is_error: false,
        })
    }
}

/// Fallback: scrape DuckDuckGo HTML lite.
async fn duckduckgo_search(query: &str) -> Result<ToolResult, ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("ccx/0.1 (Claude Code Extended)")
        .build()
        .map_err(|e| ToolError::Execution(format!("HTTP client error: {e}")))?;

    let response = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ToolError::Timeout(DEFAULT_TIMEOUT_SECS * 1000)
            } else {
                ToolError::Execution(format!("search failed: {e}"))
            }
        })?;

    if !response.status().is_success() {
        return Ok(ToolResult {
            content: format!("Search error: HTTP {}", response.status()),
            is_error: true,
        });
    }

    let body = response
        .text()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read body: {e}")))?;

    // Truncate body before processing.
    let body = if body.len() > MAX_BODY_SIZE {
        &body[..MAX_BODY_SIZE]
    } else {
        &body
    };

    let results = extract_ddg_results(body);
    if results.is_empty() {
        Ok(ToolResult {
            content: format!("No results found for: {query}"),
            is_error: false,
        })
    } else {
        Ok(ToolResult {
            content: results,
            is_error: false,
        })
    }
}

/// Extract search results from DuckDuckGo HTML.
fn extract_ddg_results(html: &str) -> String {
    let mut results = Vec::new();
    let mut pos = 0;

    while let Some(start) = html[pos..].find("class=\"result__a\"") {
        let abs_start = pos + start;

        // Find the href.
        let before = &html[..abs_start];
        if let Some(href_pos) = before.rfind("href=\"") {
            let href_start = href_pos + 6;
            if let Some(href_end) = html[href_start..].find('"') {
                let url = &html[href_start..href_start + href_end];

                // Find the link text (between > and </a>).
                if let Some(gt) = html[abs_start..].find('>') {
                    let text_start = abs_start + gt + 1;
                    if let Some(end_a) = html[text_start..].find("</a>") {
                        let title =
                            crate::web_fetch::strip_html(&html[text_start..text_start + end_a]);
                        if !title.is_empty() && !url.is_empty() {
                            results.push(format!("**{}**\n{}", title.trim(), url));
                        }
                    }
                }
            }
        }

        pos = abs_start + 1;
        if results.len() >= 10 {
            break;
        }
    }

    // Also try to extract snippets.
    let mut snippet_pos = 0;
    let mut snippet_idx = 0;
    while let Some(start) = html[snippet_pos..].find("class=\"result__snippet\"") {
        let abs_start = snippet_pos + start;
        if let Some(gt) = html[abs_start..].find('>') {
            let text_start = abs_start + gt + 1;
            // Find the closing tag.
            if let Some(end) = html[text_start..].find("</") {
                let snippet = crate::web_fetch::strip_html(&html[text_start..text_start + end]);
                if snippet_idx < results.len() && !snippet.is_empty() {
                    results[snippet_idx].push('\n');
                    results[snippet_idx].push_str(snippet.trim());
                }
                snippet_idx += 1;
            }
        }
        snippet_pos = abs_start + 1;
        if snippet_idx >= 10 {
            break;
        }
    }

    results.join("\n\n")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("/tmp"))
    }

    #[test]
    fn test_web_search_schema() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "WebSearch");
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "query"));
    }

    #[tokio::test]
    async fn test_web_search_missing_query() {
        let tool = WebSearchTool;
        let err = tool.execute(json!({}), &test_ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[test]
    fn test_extract_ddg_results_empty() {
        let html = "<html><body>No results</body></html>";
        let results = extract_ddg_results(html);
        assert!(results.is_empty());
    }

    #[test]
    fn test_extract_ddg_results_with_match() {
        let html = r#"<a rel="nofollow" href="https://example.com" class="result__a">Example Title</a><span class="result__snippet">Example snippet text</span>"#;
        let results = extract_ddg_results(html);
        assert!(results.contains("Example Title"));
        assert!(results.contains("https://example.com"));
    }
}

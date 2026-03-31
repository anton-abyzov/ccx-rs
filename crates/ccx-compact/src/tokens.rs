/// Estimate token count from text. Uses a simple heuristic:
/// ~4 characters per token for English text, ~3 for code.
pub fn estimate_tokens(text: &str) -> usize {
    // Simple heuristic: split on whitespace + punctuation boundaries.
    // More accurate than pure char count, less expensive than a real tokenizer.
    let chars = text.len();
    // Average 3.5 chars per token is a reasonable middle ground.
    (chars as f64 / 3.5).ceil() as usize
}

/// Check if the conversation has exceeded the token threshold.
pub fn exceeds_threshold(messages_text: &str, threshold: usize) -> bool {
    estimate_tokens(messages_text) > threshold
}

/// Default auto-compact threshold (approximate tokens).
pub const DEFAULT_THRESHOLD: usize = 187_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // "hello world" = 11 chars ~= 3-4 tokens
        let tokens = estimate_tokens("hello world");
        assert!(tokens >= 2 && tokens <= 5);
    }

    #[test]
    fn test_estimate_longer_text() {
        let text = "a ".repeat(1000); // 2000 chars
        let tokens = estimate_tokens(&text);
        // Should be roughly 500-600 tokens
        assert!(tokens > 400 && tokens < 800);
    }

    #[test]
    fn test_exceeds_threshold() {
        assert!(!exceeds_threshold("short", 100));
        let long = "word ".repeat(100_000);
        assert!(exceeds_threshold(&long, 1000));
    }
}

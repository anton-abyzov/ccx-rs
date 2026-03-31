use crate::tokens::{estimate_tokens, DEFAULT_THRESHOLD};

/// Summary of a compacted conversation.
#[derive(Debug, Clone)]
pub struct CompactSummary {
    pub summary_text: String,
    pub original_tokens: usize,
    pub compacted_tokens: usize,
}

/// AutoCompact: summarize conversation when it exceeds the token threshold.
/// Returns None if the conversation is within threshold.
pub fn should_compact(conversation_text: &str) -> bool {
    estimate_tokens(conversation_text) > DEFAULT_THRESHOLD
}

/// Create a compact summary placeholder for the conversation.
/// In a real implementation, this would call the LLM to summarize.
/// For now, it takes the first and last messages to preserve context.
pub fn create_summary(messages_text: &[String]) -> CompactSummary {
    let total_text: String = messages_text.join("\n");
    let original_tokens = estimate_tokens(&total_text);

    let summary = if messages_text.len() <= 4 {
        total_text.clone()
    } else {
        let first_two: String = messages_text[..2].join("\n");
        let last_two: String = messages_text[messages_text.len() - 2..].join("\n");
        format!(
            "[Conversation compacted: {} messages summarized]\n\n{}\n\n[...{} messages omitted...]\n\n{}",
            messages_text.len(),
            first_two,
            messages_text.len() - 4,
            last_two
        )
    };

    let compacted_tokens = estimate_tokens(&summary);

    CompactSummary {
        summary_text: summary,
        original_tokens,
        compacted_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_compact_short() {
        assert!(!should_compact("hello world"));
    }

    #[test]
    fn test_should_compact_long() {
        let long = "word ".repeat(200_000);
        assert!(should_compact(&long));
    }

    #[test]
    fn test_create_summary_short() {
        let msgs = vec!["Hello".into(), "World".into()];
        let summary = create_summary(&msgs);
        assert!(summary.compacted_tokens <= summary.original_tokens);
    }

    #[test]
    fn test_create_summary_long() {
        let msgs: Vec<String> = (0..10).map(|i| format!("Message {i}")).collect();
        let summary = create_summary(&msgs);
        assert!(summary.summary_text.contains("compacted"));
        assert!(summary.summary_text.contains("Message 0"));
        assert!(summary.summary_text.contains("Message 9"));
    }
}

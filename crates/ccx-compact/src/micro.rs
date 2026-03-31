use serde_json::Value;

/// MicroCompact: strip large tool results to reduce context size.
/// Replaces tool result content exceeding `max_chars` with a truncation notice.
pub fn micro_compact(messages: &mut Vec<Value>, max_chars: usize) {
    for msg in messages.iter_mut() {
        if let Some(content) = msg.get_mut("content") {
            match content {
                Value::Array(blocks) => {
                    for block in blocks.iter_mut() {
                        compact_block(block, max_chars);
                    }
                }
                Value::String(s) if s.len() > max_chars => {
                    let truncated = format!(
                        "{}... [truncated, {} chars total]",
                        &s[..max_chars.min(200)],
                        s.len()
                    );
                    *s = truncated;
                }
                _ => {}
            }
        }
    }
}

fn compact_block(block: &mut Value, max_chars: usize) {
    if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
        if let Some(content) = block.get_mut("content") {
            if let Value::String(s) = content {
                if s.len() > max_chars {
                    let preview = &s[..max_chars.min(200)];
                    *s = format!("{preview}... [truncated, {} chars total]", s.len());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_micro_compact_leaves_small() {
        let mut msgs = vec![json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "1", "content": "small"}]
        })];
        micro_compact(&mut msgs, 100);
        let content = &msgs[0]["content"][0]["content"];
        assert_eq!(content, "small");
    }

    #[test]
    fn test_micro_compact_truncates_large() {
        let large = "x".repeat(1000);
        let mut msgs = vec![json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "1", "content": large}]
        })];
        micro_compact(&mut msgs, 100);
        let content = msgs[0]["content"][0]["content"].as_str().unwrap();
        assert!(content.contains("truncated"));
        assert!(content.len() < 1000);
    }
}

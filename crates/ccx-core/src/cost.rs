use serde::{Deserialize, Serialize};

/// Tracks API usage costs for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostTracker {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub api_calls: u64,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record token usage from an API response.
    pub fn record(&mut self, usage: &ccx_api::Usage) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        if let Some(cc) = usage.cache_creation_input_tokens {
            self.total_cache_creation_tokens += cc as u64;
        }
        if let Some(cr) = usage.cache_read_input_tokens {
            self.total_cache_read_tokens += cr as u64;
        }
        self.api_calls += 1;
    }

    /// Estimate cost in USD based on Claude pricing.
    /// Uses approximate Sonnet pricing as default.
    pub fn estimated_cost_usd(&self) -> f64 {
        let input_cost = self.total_input_tokens as f64 * 3.0 / 1_000_000.0;
        let output_cost = self.total_output_tokens as f64 * 15.0 / 1_000_000.0;
        let cache_write_cost = self.total_cache_creation_tokens as f64 * 3.75 / 1_000_000.0;
        let cache_read_cost = self.total_cache_read_tokens as f64 * 0.30 / 1_000_000.0;
        input_cost + output_cost + cache_write_cost + cache_read_cost
    }

    /// Format a summary string.
    pub fn summary(&self) -> String {
        format!(
            "Tokens: {} in / {} out | Cache: {} write / {} read | Calls: {} | Cost: ${:.4}",
            self.total_input_tokens,
            self.total_output_tokens,
            self.total_cache_creation_tokens,
            self.total_cache_read_tokens,
            self.api_calls,
            self.estimated_cost_usd()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker_new() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.total_input_tokens, 0);
        assert_eq!(tracker.api_calls, 0);
        assert_eq!(tracker.estimated_cost_usd(), 0.0);
    }

    #[test]
    fn test_cost_tracker_record() {
        let mut tracker = CostTracker::new();
        tracker.record(&ccx_api::Usage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: Some(200),
            cache_read_input_tokens: Some(100),
        });
        assert_eq!(tracker.total_input_tokens, 1000);
        assert_eq!(tracker.total_output_tokens, 500);
        assert_eq!(tracker.total_cache_creation_tokens, 200);
        assert_eq!(tracker.total_cache_read_tokens, 100);
        assert_eq!(tracker.api_calls, 1);
        assert!(tracker.estimated_cost_usd() > 0.0);
    }

    #[test]
    fn test_cost_tracker_summary() {
        let mut tracker = CostTracker::new();
        tracker.record(&ccx_api::Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        });
        let summary = tracker.summary();
        assert!(summary.contains("100 in"));
        assert!(summary.contains("50 out"));
        assert!(summary.contains("Calls: 1"));
    }
}

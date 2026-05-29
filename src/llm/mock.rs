use anyhow::Result;
use async_trait::async_trait;

use super::provider::{LlmDecisionResponse, LlmProvider};

pub struct MockLlmProvider {
    model: String,
    default_decision: String,
}

impl MockLlmProvider {
    pub fn new(default_decision: &str) -> Self {
        Self {
            model: "mock-llm".to_string(),
            default_decision: default_decision.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn decide(&self, _prompt: &str) -> Result<LlmDecisionResponse> {
        Ok(LlmDecisionResponse {
            decision: self.default_decision.clone(),
            confidence: 0.80,
            should_update_topic_summary: true,
            should_create_timeline_event: true,
            timeline_event_type: Some("update".to_string()),
            reason: "mock decision".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_default_decision() {
        let provider = MockLlmProvider::new("same_story_update");
        let response = provider.decide("any prompt").await.unwrap();
        assert_eq!(response.decision, "same_story_update");
        assert!((response.confidence - 0.80).abs() < f64::EPSILON);
        assert!(response.should_update_topic_summary);
        assert!(response.should_create_timeline_event);
        assert_eq!(response.reason, "mock decision");
    }

    #[tokio::test]
    async fn mock_model_name() {
        let provider = MockLlmProvider::new("new_topic");
        assert_eq!(provider.model_name(), "mock-llm");
    }
}

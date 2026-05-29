use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmDecisionResponse {
    pub decision: String,
    pub confidence: f64,
    pub should_update_topic_summary: bool,
    pub should_create_timeline_event: bool,
    pub timeline_event_type: Option<String>,
    pub reason: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn model_name(&self) -> &str;
    async fn decide(&self, prompt: &str) -> Result<LlmDecisionResponse>;
}

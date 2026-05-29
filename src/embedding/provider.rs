use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimension(&self) -> usize;
    async fn embed_text(&self, input: &str) -> anyhow::Result<Vec<f32>>;
    async fn embed_batch(&self, inputs: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
}

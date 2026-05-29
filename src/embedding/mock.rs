use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::provider::EmbeddingProvider;

pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    fn generate_deterministic_embedding(&self, input: &str) -> Vec<f32> {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash = hasher.finalize();

        let mut embedding = Vec::with_capacity(self.dimension);

        for chunk_idx in 0..self.dimension {
            let byte_idx = chunk_idx % hash.len();
            let seed = hash[byte_idx];
            let offset = (chunk_idx / hash.len()) as u8;
            let value = ((seed.wrapping_add(offset)) as f32) / 255.0 * 2.0 - 1.0;
            embedding.push(value);
        }

        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in embedding.iter_mut() {
                *v /= norm;
            }
        }

        embedding
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    fn model_name(&self) -> &str {
        "mock-embedding-model"
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn embed_text(&self, input: &str) -> Result<Vec<f32>> {
        Ok(self.generate_deterministic_embedding(input))
    }

    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(inputs
            .iter()
            .map(|input| self.generate_deterministic_embedding(input))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deterministic_embeddings() {
        let provider = MockEmbeddingProvider::new(4096);

        let emb1 = provider.embed_text("hello world").await.unwrap();
        let emb2 = provider.embed_text("hello world").await.unwrap();

        assert_eq!(emb1, emb2);
        assert_eq!(emb1.len(), 4096);
    }

    #[tokio::test]
    async fn test_different_inputs_different_embeddings() {
        let provider = MockEmbeddingProvider::new(4096);

        let emb1 = provider.embed_text("hello").await.unwrap();
        let emb2 = provider.embed_text("world").await.unwrap();

        assert_ne!(emb1, emb2);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let provider = MockEmbeddingProvider::new(4096);

        let inputs = vec![
            "first text".to_string(),
            "second text".to_string(),
            "third text".to_string(),
        ];

        let embeddings = provider.embed_batch(&inputs).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 4096);
        assert_eq!(embeddings[1].len(), 4096);
        assert_eq!(embeddings[2].len(), 4096);

        assert_ne!(embeddings[0], embeddings[1]);
        assert_ne!(embeddings[1], embeddings[2]);
    }

    #[tokio::test]
    async fn test_embedding_is_normalized() {
        let provider = MockEmbeddingProvider::new(4096);

        let emb = provider.embed_text("test normalization").await.unwrap();
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();

        assert!((norm - 1.0).abs() < 1e-4, "norm was {}", norm);
    }
}

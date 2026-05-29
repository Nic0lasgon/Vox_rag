use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, info};

use super::provider::EmbeddingProvider;

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: String,
    dimension: usize,
    input_type: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    code: i32,
    msg: String,
    data: Option<EmbeddingData>,
    meta: Option<EmbeddingMeta>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EmbeddingData {
    results: Vec<EmbeddingResult>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResult {
    index: usize,
    embedding: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingMeta {
    usage: Option<EmbeddingUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingUsage {
    input_tokens: Option<i64>,
}

pub struct OctenEmbeddingProvider {
    client: Client,
    api_key: String,
    api_base_url: String,
    model: String,
    dimension: usize,
}

impl OctenEmbeddingProvider {
    pub fn new(
        api_key: String,
        api_base_url: String,
        model: String,
        dimension: usize,
        timeout_ms: u64,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            api_key,
            api_base_url,
            model,
            dimension,
        })
    }

    async fn call_api(&self, inputs: &[String], input_type: &str) -> Result<EmbeddingResponse> {
        let url = format!("{}/embedding", self.api_base_url);

        let request = EmbeddingRequest {
            input: inputs.to_vec(),
            model: self.model.clone(),
            dimension: self.dimension,
            input_type: input_type.to_string(),
        };

        let start = Instant::now();

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send embedding request to Octen API")?;

        let status = response.status();
        let elapsed = start.elapsed();

        let body = response
            .text()
            .await
            .context("Failed to read Octen API response body")?;

        if !status.is_success() {
            error!(
                status = %status,
                elapsed_ms = %elapsed.as_millis(),
                body = %body,
                "Octen API returned error status"
            );
            anyhow::bail!("Octen API error ({}): {}", status, body);
        }

        let parsed: EmbeddingResponse =
            serde_json::from_str(&body).context("Failed to parse Octen API response")?;

        if parsed.code != 0 {
            error!(
                code = parsed.code,
                msg = %parsed.msg,
                "Octen API returned business error"
            );
            anyhow::bail!("Octen API error (code={}): {}", parsed.code, parsed.msg);
        }

        let token_count = parsed
            .meta
            .as_ref()
            .and_then(|m| m.usage.as_ref())
            .and_then(|u| u.input_tokens);

        info!(
            inputs = inputs.len(),
            elapsed_ms = %elapsed.as_millis(),
            tokens = ?token_count,
            "Octen embedding request completed"
        );

        Ok(parsed)
    }
}

#[async_trait]
impl EmbeddingProvider for OctenEmbeddingProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn embed_text(&self, input: &str) -> Result<Vec<f32>> {
        let inputs = vec![input.to_string()];
        let response = self.call_api(&inputs, "document").await?;

        let data = response.data.context("Missing data in Octen response")?;
        let result = data
            .results
            .into_iter()
            .next()
            .context("No embedding results returned")?;

        let embedding: Vec<f32> = result.embedding.iter().map(|&v| v as f32).collect();

        if embedding.len() != self.dimension {
            anyhow::bail!(
                "Expected embedding dimension {}, got {}",
                self.dimension,
                embedding.len()
            );
        }

        Ok(embedding)
    }

    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(vec![]);
        }

        let response = self.call_api(inputs, "document").await?;

        let data = response.data.context("Missing data in Octen response")?;

        let mut results: Vec<(usize, Vec<f32>)> = data
            .results
            .into_iter()
            .map(|r| {
                let embedding: Vec<f32> = r.embedding.iter().map(|&v| v as f32).collect();
                (r.index, embedding)
            })
            .collect();

        results.sort_by_key(|(idx, _)| *idx);

        let embeddings: Vec<Vec<f32>> = results.into_iter().map(|(_, emb)| emb).collect();

        for (i, emb) in embeddings.iter().enumerate() {
            if emb.len() != self.dimension {
                anyhow::bail!(
                    "Embedding at index {} has dimension {}, expected {}",
                    i,
                    emb.len(),
                    self.dimension
                );
            }
        }

        Ok(embeddings)
    }
}

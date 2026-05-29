use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use crate::db::{article_queries, chunk_queries, embedding_queries, job_queries};
use crate::embedding::EmbeddingProvider;
use crate::pipeline::chunker::{ChunkConfig, chunk_text};
use crate::pipeline::profile::{build_article_profile_text, normalize_for_embedding, should_embed};

const MAX_RETRIES: i32 = 3;
const BASE_BACKOFF_SECS: u64 = 2;

pub struct EmbeddingWorker {
    pool: PgPool,
    provider: Arc<dyn EmbeddingProvider>,
    poll_interval: Duration,
    batch_size: i64,
    worker_id: String,
}

impl EmbeddingWorker {
    pub fn new(
        pool: PgPool,
        provider: Arc<dyn EmbeddingProvider>,
        poll_interval_secs: u64,
        batch_size: i64,
    ) -> Self {
        let worker_id = format!("worker-{}", Uuid::new_v4());
        Self {
            pool,
            provider,
            poll_interval: Duration::from_secs(poll_interval_secs),
            batch_size,
            worker_id,
        }
    }

    pub async fn run(&self) {
        info!(worker_id = %self.worker_id, "Starting embedding worker");

        loop {
            match self.process_batch().await {
                Ok(processed) => {
                    if processed == 0 {
                        debug!("No pending jobs, sleeping");
                        sleep(self.poll_interval).await;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Error processing job batch");
                    sleep(self.poll_interval).await;
                }
            }
        }
    }

    async fn process_batch(&self) -> Result<usize> {
        let jobs = job_queries::pick_pending_jobs(&self.pool, self.batch_size)
            .await
            .context("Failed to pick pending jobs")?;

        if jobs.is_empty() {
            return Ok(0);
        }

        info!(count = jobs.len(), "Processing embedding jobs");

        for job in &jobs {
            if let Err(e) = self.process_job(job).await {
                error!(job_id = %job.id, error = %e, "Failed to process job");

                let attempts = job.attempts.unwrap_or(0);
                if attempts + 1 >= MAX_RETRIES {
                    job_queries::mark_job_failed(&self.pool, job.id, &format!("{:#}", e))
                        .await
                        .context("Failed to mark job as failed")?;
                } else {
                    let delay = BASE_BACKOFF_SECS.pow(attempts as u32 + 1);
                    job_queries::reschedule_job(&self.pool, job.id, delay as i64)
                        .await
                        .context("Failed to reschedule job")?;
                }
            }
        }

        Ok(jobs.len())
    }

    #[instrument(skip(self, job), fields(job_id = %job.id, target_id = %job.target_id))]
    async fn process_job(&self, job: &crate::db::schema::EmbeddingJob) -> Result<()> {
        job_queries::mark_job_running(&self.pool, job.id, &self.worker_id)
            .await
            .context("Failed to mark job running")?;

        match job.target_type.as_str() {
            "article_profile" => self.process_profile_job(job).await,
            "article_chunk" => self.process_chunk_job(job).await,
            _ => anyhow::bail!("Unknown target_type: {}", job.target_type),
        }
    }

    #[instrument(skip(self, job), fields(job_id = %job.id, target_id = %job.target_id))]
    async fn process_profile_job(&self, job: &crate::db::schema::EmbeddingJob) -> Result<()> {
        let start = Instant::now();

        let article = article_queries::get_article_by_id(&self.pool, job.target_id)
            .await
            .context("Failed to load article")?
            .context("Article not found")?;

        if !should_embed(&article) {
            info!("Article does not meet embedding criteria, skipping");
            job_queries::mark_job_done(&self.pool, job.id)
                .await
                .context("Failed to mark job done")?;
            return Ok(());
        }

        let profile_text = build_article_profile_text(&article);
        let normalized_text = normalize_for_embedding(&profile_text);

        let mut hasher = Sha256::new();
        hasher.update(normalized_text.as_bytes());
        let input_hash = hex::encode(hasher.finalize());

        if input_hash != job.input_hash {
            debug!(
                expected_hash = %job.input_hash,
                actual_hash = %input_hash,
                "Input hash mismatch, using current content"
            );
        }

        let is_dup = job_queries::is_already_embedded(
            &self.pool,
            &job.target_type,
            job.target_id,
            &input_hash,
        )
        .await
        .context("Failed to check dedup")?;

        if is_dup {
            info!("Already embedded with same input_hash, skipping");
            job_queries::mark_job_done(&self.pool, job.id)
                .await
                .context("Failed to mark job done")?;
            return Ok(());
        }

        let embed_start = Instant::now();
        let embedding = self
            .provider
            .embed_text(&normalized_text)
            .await
            .context("Failed to generate embedding")?;
        let embed_elapsed = embed_start.elapsed();

        info!(
            dimension = embedding.len(),
            embed_ms = %embed_elapsed.as_millis(),
            "Embedding generated"
        );

        let dimension = self.provider.dimension() as i32;
        let model_name = self.provider.model_name();

        embedding_queries::upsert_article_profile(
            &self.pool,
            article.id,
            &profile_text,
            model_name,
            dimension,
            embedding,
        )
        .await
        .context("Failed to store embedding")?;

        job_queries::mark_job_done(&self.pool, job.id)
            .await
            .context("Failed to mark job done")?;

        let total_elapsed = start.elapsed();
        info!(
            total_ms = %total_elapsed.as_millis(),
            "Job processed successfully"
        );

        Ok(())
    }

    #[instrument(skip(self, job), fields(job_id = %job.id, target_id = %job.target_id))]
    async fn process_chunk_job(&self, job: &crate::db::schema::EmbeddingJob) -> Result<()> {
        let start = Instant::now();

        let article = article_queries::get_article_by_id(&self.pool, job.target_id)
            .await
            .context("Failed to load article")?
            .context("Article not found")?;

        let cleaned_text = match &article.cleaned_text {
            Some(text) if !text.is_empty() => text.as_str(),
            _ => {
                info!("Article has no cleaned_text, skipping chunk job");
                job_queries::mark_job_done(&self.pool, job.id)
                    .await
                    .context("Failed to mark job done")?;
                return Ok(());
            }
        };

        let chunk_config = ChunkConfig::default();
        let chunks = chunk_text(cleaned_text, &chunk_config);

        if chunks.is_empty() {
            info!("No chunks produced, skipping");
            job_queries::mark_job_done(&self.pool, job.id)
                .await
                .context("Failed to mark job done")?;
            return Ok(());
        }

        let chunk_texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        let token_counts: Vec<i32> = chunks.iter().map(|c| c.token_count as i32).collect();

        let embed_start = Instant::now();
        let embeddings = self
            .provider
            .embed_batch(&chunk_texts)
            .await
            .context("Failed to generate chunk embeddings")?;
        let embed_elapsed = embed_start.elapsed();

        info!(
            chunk_count = chunks.len(),
            embed_ms = %embed_elapsed.as_millis(),
            "Chunk embeddings generated"
        );

        let dimension = self.provider.dimension() as i32;
        let model_name = self.provider.model_name();

        let chunks_with_embeddings: Vec<(String, i32, Vec<f32>)> = chunk_texts
            .into_iter()
            .zip(token_counts)
            .zip(embeddings)
            .map(|((text, tc), emb)| (text, tc, emb))
            .collect();

        chunk_queries::insert_chunks(
            &self.pool,
            article.id,
            model_name,
            dimension,
            &chunks_with_embeddings,
        )
        .await
        .context("Failed to store chunks")?;

        job_queries::mark_job_done(&self.pool, job.id)
            .await
            .context("Failed to mark job done")?;

        let total_elapsed = start.elapsed();
        info!(
            total_ms = %total_elapsed.as_millis(),
            chunks = chunks.len(),
            "Chunk job processed successfully"
        );

        Ok(())
    }
}

pub fn compute_input_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_input_hash_deterministic() {
        let h1 = compute_input_hash("hello world");
        let h2 = compute_input_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_input_hash_different_inputs() {
        let h1 = compute_input_hash("hello");
        let h2 = compute_input_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compute_input_hash_empty_string() {
        let hash = compute_input_hash("");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_input_hash_consistent_length() {
        let short = compute_input_hash("a");
        let long = compute_input_hash(&"text ".repeat(1000));
        assert_eq!(short.len(), 64);
        assert_eq!(long.len(), 64);
    }
}

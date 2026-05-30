use anyhow::{Context, Result};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::db::{article_queries, embedding_queries, job_queries, topic_queries};
use crate::embedding::EmbeddingProvider;
use crate::llm::prompt;
use crate::llm::provider::LlmProvider;
use crate::topic::decision::{DecisionResult, TopicThresholds};
use crate::topic::{candidate, decision, embedding, markdown, mutation, timeline};

const MAX_RETRIES: i32 = 3;
const BASE_BACKOFF_SECS: u64 = 2;

pub struct TopicWorker {
    pool: PgPool,
    provider: Arc<dyn EmbeddingProvider>,
    llm_provider: Option<Arc<dyn LlmProvider>>,
    poll_interval: Duration,
    batch_size: i64,
    worker_id: String,
    topic_thresholds: TopicThresholds,
    max_words: usize,
    active_days: i64,
    archive_days: i64,
}

impl TopicWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        provider: Arc<dyn EmbeddingProvider>,
        llm_provider: Option<Arc<dyn LlmProvider>>,
        poll_interval_secs: u64,
        batch_size: i64,
        topic_thresholds: TopicThresholds,
        max_words: usize,
        active_days: i64,
        archive_days: i64,
    ) -> Self {
        let worker_id = format!("topic-worker-{}", Uuid::new_v4());
        Self {
            pool,
            provider,
            llm_provider,
            poll_interval: Duration::from_secs(poll_interval_secs),
            batch_size,
            worker_id,
            topic_thresholds,
            max_words,
            active_days,
            archive_days,
        }
    }

    pub async fn run(&self) {
        info!(worker_id = %self.worker_id, "Starting topic worker");

        loop {
            match self.process_batch().await {
                Ok(processed) => {
                    if processed == 0 {
                        info!("No pending topic jobs, sleeping");
                        sleep(self.poll_interval).await;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Error processing topic job batch");
                    sleep(self.poll_interval).await;
                }
            }
        }
    }

    async fn process_batch(&self) -> Result<usize> {
        let mut jobs = Vec::new();
        for target_type in &[
            "process_article_batch",
            "refresh_topic_embedding",
            "archive_inactive_topics",
        ] {
            let batch = job_queries::pick_pending_jobs(
                &self.pool,
                self.batch_size - jobs.len() as i64,
                Some(target_type),
            )
            .await
            .context("Failed to pick pending jobs")?;
            jobs.extend(batch);
            if jobs.len() >= self.batch_size as usize {
                break;
            }
        }

        if jobs.is_empty() {
            return Ok(0);
        }

        info!(count = jobs.len(), "Processing topic jobs");

        for job in &jobs {
            if let Err(e) = self.process_job(job).await {
                error!(job_id = %job.id, error = %e, "Failed to process topic job");

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
            "process_article_batch" => self.process_article_batch_job(job).await,
            "refresh_topic_embedding" => self.process_refresh_embedding_job(job).await,
            "archive_inactive_topics" => self.process_archive_job(job).await,
            _ => anyhow::bail!("Unknown target_type for TopicWorker: {}", job.target_type),
        }
    }

    #[instrument(skip(self, job), fields(job_id = %job.id, target_id = %job.target_id))]
    async fn process_article_batch_job(&self, job: &crate::db::schema::EmbeddingJob) -> Result<()> {
        let article = article_queries::get_article_by_id(&self.pool, job.target_id)
            .await
            .context("Failed to load article")?
            .context("Article not found")?;

        let candidates = candidate::find_candidates(
            &self.pool,
            article.id,
            10,
            self.topic_thresholds.topic_weak_match,
        )
        .await
        .context("Failed to find topic candidates")?;

        let decision =
            decision::decide_article_topic_relation(&candidates, None, &self.topic_thresholds);

        match decision {
            DecisionResult::LinkToTopic {
                topic_id,
                relation_type,
                confidence,
            } => {
                mutation::update_topic_on_new_article(
                    &self.pool,
                    topic_id,
                    &article,
                    &relation_type,
                    confidence,
                    None,
                    "rules",
                )
                .await
                .context("Failed to update topic on new article")?;

                timeline::add_article_to_timeline(
                    &self.pool,
                    topic_id,
                    &article,
                    &relation_type,
                    confidence,
                )
                .await
                .context("Failed to add article to timeline")?;

                let md = markdown::build_topic_markdown(&self.pool, topic_id, self.max_words)
                    .await
                    .context("Failed to build topic markdown")?;
                let hash = markdown::compute_markdown_hash(&md);

                topic_queries::update_topic_summary(
                    &self.pool,
                    topic_id,
                    None,
                    None,
                    Some(&md),
                    Some(&hash),
                )
                .await
                .context("Failed to update topic summary")?;

                embedding::ensure_topic_embedding(
                    &self.pool,
                    self.provider.as_ref(),
                    topic_id,
                    self.max_words,
                    true,
                )
                .await
                .context("Failed to ensure topic embedding")?;

                info!(
                    article_id = %article.id,
                    topic_id = %topic_id,
                    relation_type = relation_type,
                    confidence = confidence,
                    decision_method = "rules",
                    "article_linked_to_topic"
                );
            }
            DecisionResult::CreateNewTopic => {
                let profile = embedding_queries::get_article_profile(&self.pool, article.id)
                    .await
                    .context("Failed to get article profile")?;

                let embedding_vec = profile.and_then(|p| p.embedding.map(|v| v.into()));

                let topic = mutation::create_topic_from_article(
                    &self.pool,
                    &article,
                    embedding_vec,
                    Some(self.provider.model_name()),
                )
                .await
                .context("Failed to create topic from article")?;

                topic_queries::link_article_to_topic(
                    &self.pool,
                    topic.id,
                    article.id,
                    "same_story_update",
                    1.0,
                    None,
                    "auto_create",
                    true,
                    true,
                )
                .await
                .context("Failed to link article to new topic")?;

                let md = markdown::build_topic_markdown(&self.pool, topic.id, self.max_words)
                    .await
                    .context("Failed to build topic markdown")?;
                let hash = markdown::compute_markdown_hash(&md);

                topic_queries::update_topic_summary(
                    &self.pool,
                    topic.id,
                    None,
                    None,
                    Some(&md),
                    Some(&hash),
                )
                .await
                .context("Failed to update topic summary")?;

                embedding::ensure_topic_embedding(
                    &self.pool,
                    self.provider.as_ref(),
                    topic.id,
                    self.max_words,
                    true,
                )
                .await
                .context("Failed to ensure topic embedding")?;

                info!(topic_id = %topic.id, article_id = %article.id, "topic_created");
            }
            DecisionResult::Ambiguous { candidates } => {
                info!(
                    article_id = %article.id,
                    candidate_count = candidates.len(),
                    "ambiguous_topic_decision"
                );

                let final_decision = if let Some(ref llm) = self.llm_provider {
                    let best_candidate = candidates.first();
                    let topic_details = if let Some(c) = best_candidate {
                        topic_queries::get_topic_by_id(&self.pool, c.topic_id)
                            .await
                            .context("Failed to fetch topic details for LLM")?
                    } else {
                        None
                    };

                    let timeline_summary = if let Some(ref topic) = topic_details {
                        let events = topic_queries::get_timeline_events(&self.pool, topic.id)
                            .await
                            .context("Failed to fetch timeline events")?;
                        events
                            .iter()
                            .take(5)
                            .map(|e| format!("- {} : {}", e.event_date.format("%Y-%m-%d"), e.title))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        String::new()
                    };

                    let user_prompt = prompt::build_user_prompt(
                        &article,
                        &candidates,
                        topic_details.as_ref(),
                        &timeline_summary,
                    );

                    match llm.decide(&user_prompt).await {
                        Ok(llm_response) => {
                            info!(
                                article_id = %article.id,
                                decision = %llm_response.decision,
                                confidence = llm_response.confidence,
                                reason = %llm_response.reason,
                                "llm_arbitration"
                            );
                            decision::apply_llm_decision(&llm_response, best_candidate)
                        }
                        Err(e) => {
                            error!(error = %e, "LLM arbitration failed, falling back to CreateNewTopic");
                            DecisionResult::CreateNewTopic
                        }
                    }
                } else {
                    DecisionResult::CreateNewTopic
                };

                match final_decision {
                    DecisionResult::LinkToTopic {
                        topic_id,
                        relation_type,
                        confidence,
                    } => {
                        mutation::update_topic_on_new_article(
                            &self.pool,
                            topic_id,
                            &article,
                            &relation_type,
                            confidence,
                            None,
                            "llm_arbitration",
                        )
                        .await
                        .context("Failed to update topic on LLM-linked article")?;

                        timeline::add_article_to_timeline(
                            &self.pool,
                            topic_id,
                            &article,
                            &relation_type,
                            confidence,
                        )
                        .await
                        .context("Failed to add article to timeline")?;

                        let md =
                            markdown::build_topic_markdown(&self.pool, topic_id, self.max_words)
                                .await
                                .context("Failed to build topic markdown")?;
                        let hash = markdown::compute_markdown_hash(&md);

                        topic_queries::update_topic_summary(
                            &self.pool,
                            topic_id,
                            None,
                            None,
                            Some(&md),
                            Some(&hash),
                        )
                        .await
                        .context("Failed to update topic summary")?;

                        embedding::ensure_topic_embedding(
                            &self.pool,
                            self.provider.as_ref(),
                            topic_id,
                            self.max_words,
                            true,
                        )
                        .await
                        .context("Failed to ensure topic embedding")?;

                        info!(
                            article_id = %article.id,
                            topic_id = %topic_id,
                            relation_type = relation_type,
                            confidence = confidence,
                            decision_method = "llm_arbitration",
                            "article_linked_to_topic_via_llm"
                        );
                    }
                    _ => {
                        let profile =
                            embedding_queries::get_article_profile(&self.pool, article.id)
                                .await
                                .context("Failed to get article profile")?;

                        let embedding_vec = profile.and_then(|p| p.embedding.map(|v| v.into()));

                        let topic = mutation::create_topic_from_article(
                            &self.pool,
                            &article,
                            embedding_vec,
                            Some(self.provider.model_name()),
                        )
                        .await
                        .context("Failed to create topic from article")?;

                        topic_queries::link_article_to_topic(
                            &self.pool,
                            topic.id,
                            article.id,
                            "same_story_update",
                            candidates.first().map(|c| c.score).unwrap_or(0.5),
                            None,
                            "auto_create_ambiguous",
                            true,
                            true,
                        )
                        .await
                        .context("Failed to link article to new topic")?;

                        let md =
                            markdown::build_topic_markdown(&self.pool, topic.id, self.max_words)
                                .await
                                .context("Failed to build topic markdown")?;
                        let hash = markdown::compute_markdown_hash(&md);

                        topic_queries::update_topic_summary(
                            &self.pool,
                            topic.id,
                            None,
                            None,
                            Some(&md),
                            Some(&hash),
                        )
                        .await
                        .context("Failed to update topic summary")?;

                        embedding::ensure_topic_embedding(
                            &self.pool,
                            self.provider.as_ref(),
                            topic.id,
                            self.max_words,
                            true,
                        )
                        .await
                        .context("Failed to ensure topic embedding")?;

                        info!(topic_id = %topic.id, article_id = %article.id, "topic_created_from_ambiguous");
                    }
                }
            }
        }

        job_queries::mark_job_done(&self.pool, job.id)
            .await
            .context("Failed to mark job done")?;

        Ok(())
    }

    #[instrument(skip(self, job), fields(job_id = %job.id))]
    async fn process_refresh_embedding_job(
        &self,
        job: &crate::db::schema::EmbeddingJob,
    ) -> Result<()> {
        let count = embedding::refresh_topic_embeddings_batch(
            &self.pool,
            self.provider.as_ref(),
            50,
            self.max_words,
        )
        .await
        .context("Failed to refresh topic embeddings batch")?;

        info!(count = count, "Refreshed topic embeddings batch");

        job_queries::mark_job_done(&self.pool, job.id)
            .await
            .context("Failed to mark job done")?;

        Ok(())
    }

    #[instrument(skip(self, job), fields(job_id = %job.id))]
    async fn process_archive_job(&self, job: &crate::db::schema::EmbeddingJob) -> Result<()> {
        let count =
            mutation::archive_inactive_topics(&self.pool, self.active_days, self.archive_days)
                .await
                .context("Failed to archive inactive topics")?;

        info!(count = count, "Archived inactive topics");

        job_queries::mark_job_done(&self.pool, job.id)
            .await
            .context("Failed to mark job done")?;

        Ok(())
    }
}

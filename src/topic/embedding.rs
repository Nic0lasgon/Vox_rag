use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::topic_queries;
use crate::embedding::EmbeddingProvider;
use crate::topic::markdown;

#[instrument(skip(pool, provider))]
pub async fn ensure_topic_embedding(
    pool: &PgPool,
    provider: &dyn EmbeddingProvider,
    topic_id: Uuid,
    max_words: usize,
    reembed_on_change: bool,
) -> Result<bool> {
    let mut topic = topic_queries::get_topic_by_id(pool, topic_id)
        .await?
        .context("Topic not found")?;

    if topic.topic_markdown.as_deref().is_none_or(|s| s.is_empty()) {
        let md_text = markdown::build_topic_markdown(pool, topic_id, max_words).await?;
        let hash = markdown::compute_markdown_hash(&md_text);
        topic_queries::update_topic_summary(
            pool,
            topic_id,
            None,
            None,
            Some(&md_text),
            Some(&hash),
        )
        .await?;
        topic.topic_markdown = Some(md_text);
        topic.topic_markdown_hash = Some(hash);
    }

    let current_hash = markdown::compute_markdown_hash(
        topic
            .topic_markdown
            .as_ref()
            .context("topic_markdown is unexpectedly empty")?,
    );

    if current_hash == topic.topic_markdown_hash.as_deref().unwrap_or("")
        && topic.topic_embedding.is_some()
        && (!reembed_on_change
            || current_hash == topic.topic_markdown_hash.as_deref().unwrap_or(""))
    {
        return Ok(false);
    }

    let embedding = provider
        .embed_text(
            topic
                .topic_markdown
                .as_ref()
                .context("topic_markdown is unexpectedly empty")?,
        )
        .await?;

    topic_queries::update_topic_embedding(pool, topic_id, &embedding, provider.model_name())
        .await?;

    info!(topic_id = %topic_id, "Topic embedding updated");

    Ok(true)
}

#[instrument(skip(pool, provider))]
pub async fn refresh_topic_embeddings_batch(
    pool: &PgPool,
    provider: &dyn EmbeddingProvider,
    limit: i32,
    max_words: usize,
) -> Result<usize> {
    let topics = topic_queries::find_topics_needing_embedding(pool, limit).await?;

    let mut processed = 0usize;

    for topic in &topics {
        match ensure_topic_embedding(pool, provider, topic.id, max_words, true).await {
            Ok(true) => {
                processed += 1;
            }
            Ok(false) => {
                info!(topic_id = %topic.id, "Topic embedding already up to date");
            }
            Err(e) => {
                info!(topic_id = %topic.id, error = %e, "Failed to ensure topic embedding, continuing batch");
            }
        }
    }

    info!(
        total = processed,
        scanned = topics.len(),
        "Topic embedding batch completed"
    );

    Ok(processed)
}

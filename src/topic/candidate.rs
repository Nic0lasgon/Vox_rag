use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::embedding_queries;
use crate::db::schema::TopicCandidate;
use crate::db::topic_queries;

#[instrument(skip(pool))]
pub async fn find_candidates(
    pool: &PgPool,
    article_id: Uuid,
    top_k: i64,
    min_score: f64,
) -> Result<Vec<TopicCandidate>> {
    let profile = embedding_queries::get_article_profile(pool, article_id)
        .await
        .context("Failed to fetch article profile")?;

    let profile = profile.context("No embedding profile found for article")?;

    let embedding_vec: Vec<f32> = profile
        .embedding
        .context("Article profile has no embedding")?
        .into();

    let candidates = topic_queries::find_similar_topics(pool, embedding_vec, top_k)
        .await
        .context("Failed to find similar topics")?;

    let filtered: Vec<TopicCandidate> = candidates
        .into_iter()
        .filter(|c| c.score >= min_score)
        .collect();

    info!(
        article_id = %article_id,
        found = filtered.len(),
        min_score = min_score,
        "Found topic candidates"
    );

    Ok(filtered)
}

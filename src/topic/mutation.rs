use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::schema::{Article, Topic};
use crate::db::topic_queries;

#[instrument(skip(pool))]
pub async fn create_topic_from_article(
    pool: &PgPool,
    article: &Article,
    topic_embedding: Option<Vec<f32>>,
    embedding_model: Option<&str>,
) -> Result<Topic> {
    let title = if article.title.len() > 200 {
        &article.title[..200]
    } else {
        &article.title
    };

    let slug = format!(
        "{}-{}",
        slugify(&article.title),
        &article.id.to_string()[..8]
    );

    let language = article.language.clone();
    let entities = serde_json::json!([]);
    let short_summary = article.description.clone();

    let topic = topic_queries::insert_topic(
        pool,
        title,
        &slug,
        None,
        language.as_deref(),
        &entities,
        short_summary.as_deref(),
        None,
        None,
        embedding_model,
        topic_embedding,
    )
    .await
    .context("Failed to create topic from article")?;

    info!(
        topic_id = %topic.id,
        article_id = %article.id,
        slug = slug,
        "Created topic from article"
    );

    Ok(topic)
}

#[instrument(skip(pool))]
pub async fn update_topic_on_new_article(
    pool: &PgPool,
    topic_id: Uuid,
    article: &Article,
    relation_type: &str,
    confidence_score: f64,
    similarity_score: Option<f64>,
    decision_method: &str,
) -> Result<()> {
    topic_queries::link_article_to_topic(
        pool,
        topic_id,
        article.id,
        relation_type,
        confidence_score,
        similarity_score,
        decision_method,
        false,
        relation_type == "same_story_update",
    )
    .await
    .context("Failed to link article to topic")?;

    topic_queries::update_topic_counts(pool, topic_id)
        .await
        .context("Failed to update topic counts")?;

    if relation_type == "same_story_update"
        && let Some(topic) = topic_queries::get_topic_by_id(pool, topic_id)
            .await
            .context("Failed to fetch topic")?
        && topic.status == "new"
    {
        topic_queries::update_topic_status(pool, topic_id, "active")
            .await
            .context("Failed to update topic status")?;
    }

    info!(
        topic_id = %topic_id,
        article_id = %article.id,
        relation_type = relation_type,
        "Updated topic with new article"
    );

    Ok(())
}

#[instrument(skip(pool))]
pub async fn archive_inactive_topics(
    pool: &PgPool,
    active_days: i64,
    archive_days: i64,
) -> Result<usize> {
    let stale = topic_queries::find_stale_topics(pool, active_days, archive_days)
        .await
        .context("Failed to find stale topics")?;

    let count = stale.len();

    for topic in &stale {
        topic_queries::update_topic_status(pool, topic.id, "cooling_down")
            .await
            .context("Failed to update stale topic status")?;
    }

    info!(count = count, "Archived inactive topics");

    Ok(count)
}

pub fn slugify(text: &str) -> String {
    let lower = text.to_lowercase();
    let cleaned: String = lower
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect();
    let with_hyphens = cleaned.replace(' ', "-");
    let collapsed = with_hyphens.chars().fold(String::new(), |mut acc, c| {
        if c == '-' && acc.ends_with('-') {
            acc
        } else {
            acc.push(c);
            acc
        }
    });
    let trimmed = collapsed.trim_matches('-');
    if trimmed.is_empty() {
        "topic".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("Hello, World! 123"), "hello-world-123");
    }

    #[test]
    fn slugify_empty_string() {
        assert_eq!(slugify(""), "topic");
    }

    #[test]
    fn slugify_unicode() {
        assert_eq!(slugify("Israël"), "isral");
    }
}

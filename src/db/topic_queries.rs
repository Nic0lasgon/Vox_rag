use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::PgPool;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::schema::{Topic, TopicArticle, TopicCandidate, TopicTimelineEvent};

#[allow(clippy::too_many_arguments)]
#[instrument(skip(pool, main_entities, topic_embedding))]
pub async fn insert_topic(
    pool: &PgPool,
    title: &str,
    stable_slug: &str,
    category: Option<&str>,
    language: Option<&str>,
    main_entities: &serde_json::Value,
    short_summary: Option<&str>,
    topic_markdown: Option<&str>,
    topic_markdown_hash: Option<&str>,
    embedding_model: Option<&str>,
    topic_embedding: Option<Vec<f32>>,
) -> Result<Topic> {
    let vector = topic_embedding.map(Vector::from);

    let topic = sqlx::query_as::<_, Topic>(
        r#"
        INSERT INTO topics (
            title, stable_slug, status, category, language,
            first_seen_at, last_seen_at, main_entities,
            short_summary, topic_markdown, topic_markdown_hash,
            embedding_model, embedding_dimension, topic_embedding,
            embedded_at, created_at, updated_at
        ) VALUES (
            $1, $2, 'new', $3, $4,
            NOW(), NOW(), $5,
            $6, $7, $8,
            $9, 4096, $10,
            CASE WHEN $10 IS NOT NULL THEN NOW() ELSE NULL END,
            NOW(), NOW()
        )
        RETURNING id, title, stable_slug, status, category, language,
                  first_seen_at, last_seen_at, main_entities,
                  secondary_entities, keywords, short_summary, long_summary,
                  topic_markdown, topic_markdown_hash, embedding_model,
                  embedding_dimension, topic_embedding, embedded_at,
                  importance_score, confidence_score, article_count,
                  source_count, merged_into_topic_id, created_at, updated_at
        "#,
    )
    .bind(title)
    .bind(stable_slug)
    .bind(category)
    .bind(language)
    .bind(main_entities)
    .bind(short_summary)
    .bind(topic_markdown)
    .bind(topic_markdown_hash)
    .bind(embedding_model)
    .bind(&vector)
    .fetch_one(pool)
    .await
    .context("Failed to insert topic")?;

    info!(topic_id = %topic.id, title = title, "Topic inserted");

    Ok(topic)
}

#[instrument(skip(pool))]
pub async fn get_topic_by_id(pool: &PgPool, topic_id: Uuid) -> Result<Option<Topic>> {
    let topic = sqlx::query_as::<_, Topic>(
        r#"
        SELECT id, title, stable_slug, status, category, language,
               first_seen_at, last_seen_at, main_entities,
               secondary_entities, keywords, short_summary, long_summary,
               topic_markdown, topic_markdown_hash, embedding_model,
               embedding_dimension, topic_embedding, embedded_at,
               importance_score, confidence_score, article_count,
               source_count, merged_into_topic_id, created_at, updated_at
        FROM topics
        WHERE id = $1
        "#,
    )
    .bind(topic_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch topic by ID")?;

    Ok(topic)
}

#[instrument(skip(pool))]
pub async fn get_topic_by_slug(pool: &PgPool, slug: &str) -> Result<Option<Topic>> {
    let topic = sqlx::query_as::<_, Topic>(
        r#"
        SELECT id, title, stable_slug, status, category, language,
               first_seen_at, last_seen_at, main_entities,
               secondary_entities, keywords, short_summary, long_summary,
               topic_markdown, topic_markdown_hash, embedding_model,
               embedding_dimension, topic_embedding, embedded_at,
               importance_score, confidence_score, article_count,
               source_count, merged_into_topic_id, created_at, updated_at
        FROM topics
        WHERE stable_slug = $1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch topic by slug")?;

    Ok(topic)
}

#[instrument(skip(pool, embedding))]
pub async fn find_similar_topics(
    pool: &PgPool,
    embedding: Vec<f32>,
    limit: i64,
) -> Result<Vec<TopicCandidate>> {
    let vector = Vector::from(embedding);

    let results = sqlx::query_as::<_, TopicCandidate>(
        r#"
        SELECT
            id as topic_id,
            title,
            status,
            1 - (topic_embedding <=> $1) as score,
            last_seen_at
        FROM topics
        WHERE topic_embedding IS NOT NULL
          AND status NOT IN ('merged', 'discarded')
        ORDER BY topic_embedding <=> $1
        LIMIT $2
        "#,
    )
    .bind(&vector)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to find similar topics")?;

    debug!(count = results.len(), limit = limit, "Found similar topics");

    Ok(results)
}

#[instrument(skip(pool))]
pub async fn update_topic_summary(
    pool: &PgPool,
    topic_id: Uuid,
    short_summary: Option<&str>,
    long_summary: Option<&str>,
    topic_markdown: Option<&str>,
    topic_markdown_hash: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE topics
        SET short_summary = COALESCE($2, short_summary),
            long_summary = COALESCE($3, long_summary),
            topic_markdown = COALESCE($4, topic_markdown),
            topic_markdown_hash = COALESCE($5, topic_markdown_hash),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(topic_id)
    .bind(short_summary)
    .bind(long_summary)
    .bind(topic_markdown)
    .bind(topic_markdown_hash)
    .execute(pool)
    .await
    .context("Failed to update topic summary")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Topic not found: {}", topic_id);
    }

    debug!(topic_id = %topic_id, "Topic summary updated");

    Ok(())
}

#[instrument(skip(pool, embedding))]
pub async fn update_topic_embedding(
    pool: &PgPool,
    topic_id: Uuid,
    embedding: &[f32],
    model: &str,
) -> Result<()> {
    let vector = Vector::from(embedding.to_owned());

    let result = sqlx::query(
        r#"
        UPDATE topics
        SET topic_embedding = $2,
            embedding_model = $3,
            embedding_dimension = 4096,
            embedded_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(topic_id)
    .bind(&vector)
    .bind(model)
    .execute(pool)
    .await
    .context("Failed to update topic embedding")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Topic not found: {}", topic_id);
    }

    info!(topic_id = %topic_id, model = model, "Topic embedding updated");

    Ok(())
}

#[instrument(skip(pool))]
pub async fn update_topic_status(pool: &PgPool, topic_id: Uuid, status: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE topics
        SET status = $2, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(topic_id)
    .bind(status)
    .execute(pool)
    .await
    .context("Failed to update topic status")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Topic not found: {}", topic_id);
    }

    info!(topic_id = %topic_id, status = status, "Topic status updated");

    Ok(())
}

#[instrument(skip(pool))]
pub async fn update_topic_counts(pool: &PgPool, topic_id: Uuid) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE topics
        SET article_count = (
                SELECT COUNT(*) FROM topic_articles WHERE topic_id = $1
            ),
            source_count = (
                SELECT COUNT(DISTINCT a.source_name)
                FROM topic_articles ta
                JOIN articles a ON a.id = ta.article_id
                WHERE ta.topic_id = $1
            ),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(topic_id)
    .execute(pool)
    .await
    .context("Failed to update topic counts")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Topic not found: {}", topic_id);
    }

    debug!(topic_id = %topic_id, "Topic counts updated");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip(pool))]
pub async fn link_article_to_topic(
    pool: &PgPool,
    topic_id: Uuid,
    article_id: Uuid,
    relation_type: &str,
    confidence_score: f64,
    similarity_score: Option<f64>,
    decision_method: &str,
    is_primary: bool,
    is_timeline_source: bool,
) -> Result<TopicArticle> {
    let link = sqlx::query_as::<_, TopicArticle>(
        r#"
        INSERT INTO topic_articles (
            topic_id, article_id, relation_type, confidence_score,
            similarity_score, decision_method, is_primary, is_timeline_source,
            added_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (topic_id, article_id) DO UPDATE SET
            relation_type = EXCLUDED.relation_type,
            confidence_score = EXCLUDED.confidence_score,
            similarity_score = EXCLUDED.similarity_score,
            decision_method = EXCLUDED.decision_method,
            is_primary = EXCLUDED.is_primary,
            is_timeline_source = EXCLUDED.is_timeline_source
        RETURNING id, topic_id, article_id, relation_type, confidence_score,
                  similarity_score, decision_method, is_primary,
                  is_timeline_source, added_at
        "#,
    )
    .bind(topic_id)
    .bind(article_id)
    .bind(relation_type)
    .bind(confidence_score)
    .bind(similarity_score)
    .bind(decision_method)
    .bind(is_primary)
    .bind(is_timeline_source)
    .fetch_one(pool)
    .await
    .context("Failed to link article to topic")?;

    info!(
        topic_id = %topic_id,
        article_id = %article_id,
        relation_type = relation_type,
        "Article linked to topic"
    );

    Ok(link)
}

#[instrument(skip(pool))]
pub async fn get_topic_articles(pool: &PgPool, topic_id: Uuid) -> Result<Vec<TopicArticle>> {
    let articles = sqlx::query_as::<_, TopicArticle>(
        r#"
        SELECT id, topic_id, article_id, relation_type, confidence_score,
               similarity_score, decision_method, is_primary,
               is_timeline_source, added_at
        FROM topic_articles
        WHERE topic_id = $1
        ORDER BY added_at DESC
        "#,
    )
    .bind(topic_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch topic articles")?;

    debug!(topic_id = %topic_id, count = articles.len(), "Fetched topic articles");

    Ok(articles)
}

#[instrument(skip(pool))]
pub async fn get_article_topics(pool: &PgPool, article_id: Uuid) -> Result<Vec<TopicArticle>> {
    let topics = sqlx::query_as::<_, TopicArticle>(
        r#"
        SELECT id, topic_id, article_id, relation_type, confidence_score,
               similarity_score, decision_method, is_primary,
               is_timeline_source, added_at
        FROM topic_articles
        WHERE article_id = $1
        ORDER BY confidence_score DESC
        "#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch article topics")?;

    debug!(article_id = %article_id, count = topics.len(), "Fetched article topics");

    Ok(topics)
}

#[instrument(skip(pool))]
pub async fn find_stale_topics(
    pool: &PgPool,
    active_days: i64,
    cooling_days: i64,
) -> Result<Vec<Topic>> {
    let topics = sqlx::query_as::<_, Topic>(
        r#"
        SELECT id, title, stable_slug, status, category, language,
               first_seen_at, last_seen_at, main_entities,
               secondary_entities, keywords, short_summary, long_summary,
               topic_markdown, topic_markdown_hash, embedding_model,
               embedding_dimension, topic_embedding, embedded_at,
               importance_score, confidence_score, article_count,
               source_count, merged_into_topic_id, created_at, updated_at
        FROM topics
        WHERE status IN ('active', 'new')
          AND last_seen_at < NOW() - INTERVAL '1 day' * $1
          AND last_seen_at > NOW() - INTERVAL '1 day' * $2
        ORDER BY last_seen_at ASC
        "#,
    )
    .bind(active_days)
    .bind(cooling_days)
    .fetch_all(pool)
    .await
    .context("Failed to find stale topics")?;

    debug!(count = topics.len(), "Found stale topics");

    Ok(topics)
}

#[instrument(skip(pool))]
pub async fn get_timeline_events(pool: &PgPool, topic_id: Uuid) -> Result<Vec<TopicTimelineEvent>> {
    let events = sqlx::query_as::<_, TopicTimelineEvent>(
        r#"
        SELECT id, topic_id, event_date, title, summary, importance,
               event_type, source_article_ids, confidence_score,
               created_at, updated_at
        FROM topic_timeline_events
        WHERE topic_id = $1
        ORDER BY event_date DESC
        "#,
    )
    .bind(topic_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch timeline events")?;

    debug!(topic_id = %topic_id, count = events.len(), "Fetched timeline events");

    Ok(events)
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip(pool, source_article_ids))]
pub async fn insert_timeline_event(
    pool: &PgPool,
    topic_id: Uuid,
    event_date: DateTime<Utc>,
    title: &str,
    summary: &str,
    importance: &str,
    event_type: Option<&str>,
    source_article_ids: &[Uuid],
    confidence_score: f64,
) -> Result<TopicTimelineEvent> {
    let event = sqlx::query_as::<_, TopicTimelineEvent>(
        r#"
        INSERT INTO topic_timeline_events (
            topic_id, event_date, title, summary, importance,
            event_type, source_article_ids, confidence_score,
            created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
        RETURNING id, topic_id, event_date, title, summary, importance,
                  event_type, source_article_ids, confidence_score,
                  created_at, updated_at
        "#,
    )
    .bind(topic_id)
    .bind(event_date)
    .bind(title)
    .bind(summary)
    .bind(importance)
    .bind(event_type)
    .bind(source_article_ids)
    .bind(confidence_score)
    .fetch_one(pool)
    .await
    .context("Failed to insert timeline event")?;

    info!(
        event_id = %event.id,
        topic_id = %topic_id,
        title = title,
        "Timeline event inserted"
    );

    Ok(event)
}

#[instrument(skip(pool))]
pub async fn find_topics_needing_embedding(pool: &PgPool, limit: i32) -> Result<Vec<Topic>> {
    let topics = sqlx::query_as::<_, Topic>(
        r#"
        SELECT id, title, stable_slug, status, category, language,
               first_seen_at, last_seen_at, main_entities,
               secondary_entities, keywords, short_summary, long_summary,
               topic_markdown, topic_markdown_hash, embedding_model,
               embedding_dimension, topic_embedding, embedded_at,
               importance_score, confidence_score, article_count,
               source_count, merged_into_topic_id, created_at, updated_at
        FROM topics
        WHERE topic_embedding IS NULL
           OR (topic_markdown_hash IS NOT NULL AND topic_markdown_hash != '' AND embedded_at IS NULL)
           OR (topic_markdown IS NOT NULL AND topic_markdown_hash IS NULL)
           OR (topic_markdown IS NOT NULL AND topic_markdown_hash IS NOT NULL AND embedded_at IS NULL)
        ORDER BY updated_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to find topics needing embedding")?;

    debug!(
        count = topics.len(),
        limit = limit,
        "Found topics needing embedding"
    );

    Ok(topics)
}

#[instrument(skip(pool))]
pub async fn merge_topic(
    pool: &PgPool,
    source_topic_id: Uuid,
    target_topic_id: Uuid,
) -> Result<()> {
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    sqlx::query(
        r#"
        UPDATE topic_articles
        SET topic_id = $2
        WHERE topic_id = $1
        ON CONFLICT (topic_id, article_id) DO NOTHING
        "#,
    )
    .bind(source_topic_id)
    .bind(target_topic_id)
    .execute(&mut *tx)
    .await
    .context("Failed to move topic_articles")?;

    sqlx::query(
        r#"
        UPDATE topic_timeline_events
        SET topic_id = $2, updated_at = NOW()
        WHERE topic_id = $1
        "#,
    )
    .bind(source_topic_id)
    .bind(target_topic_id)
    .execute(&mut *tx)
    .await
    .context("Failed to move timeline events")?;

    sqlx::query(
        r#"
        UPDATE topics
        SET status = 'merged',
            merged_into_topic_id = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(source_topic_id)
    .bind(target_topic_id)
    .execute(&mut *tx)
    .await
    .context("Failed to mark source topic as merged")?;

    tx.commit()
        .await
        .context("Failed to commit merge transaction")?;

    info!(
        source_topic_id = %source_topic_id,
        target_topic_id = %target_topic_id,
        "Topic merged"
    );

    Ok(())
}

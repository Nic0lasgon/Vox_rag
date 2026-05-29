use anyhow::{Context, Result};
use chrono::Utc;
use pgvector::Vector;
use sqlx::PgPool;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::schema::{ArticleProfile, SimilarArticleResult};

#[instrument(skip(pool, embedding))]
pub async fn upsert_article_profile(
    pool: &PgPool,
    article_id: Uuid,
    profile_text: &str,
    model: &str,
    dimension: i32,
    embedding: Vec<f32>,
) -> Result<()> {
    let vector = Vector::from(embedding);
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO article_profiles (
            article_id, profile_text, embedding_model, embedding_dimension,
            embedding, embedded_at, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (article_id) DO UPDATE SET
            profile_text = EXCLUDED.profile_text,
            embedding_model = EXCLUDED.embedding_model,
            embedding_dimension = EXCLUDED.embedding_dimension,
            embedding = EXCLUDED.embedding,
            embedded_at = EXCLUDED.embedded_at,
            updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(article_id)
    .bind(profile_text)
    .bind(model)
    .bind(dimension)
    .bind(&vector)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .context("Failed to upsert article profile")?;

    info!(
        article_id = %article_id,
        model = model,
        dimension = dimension,
        "Article profile upserted"
    );

    Ok(())
}

#[instrument(skip(pool))]
pub async fn get_article_profile(
    pool: &PgPool,
    article_id: Uuid,
) -> Result<Option<ArticleProfile>> {
    let profile = sqlx::query_as::<_, ArticleProfile>(
        r#"
        SELECT article_id, profile_text, short_summary, main_event,
               entities, keywords, embedding_model, embedding_dimension,
               embedding, embedded_at, created_at, updated_at
        FROM article_profiles
        WHERE article_id = $1
        "#,
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch article profile")?;

    Ok(profile)
}

#[instrument(skip(pool, embedding))]
pub async fn find_similar_articles(
    pool: &PgPool,
    embedding: Vec<f32>,
    top_k: i32,
    exclude_ids: &[Uuid],
    min_score: f64,
) -> Result<Vec<SimilarArticleResult>> {
    let vector = Vector::from(embedding);

    let results = sqlx::query_as::<_, SimilarArticleResult>(
        r#"
        SELECT
            ap.article_id,
            1 - (ap.embedding <=> $1) as score,
            a.title,
            a.source_name,
            a.canonical_url as url,
            a.published_at
        FROM article_profiles ap
        JOIN articles a ON a.id = ap.article_id
        WHERE ap.article_id != ALL($2)
          AND ap.embedding IS NOT NULL
          AND 1 - (ap.embedding <=> $1) >= $3
        ORDER BY ap.embedding <=> $1
        LIMIT $4
        "#,
    )
    .bind(&vector)
    .bind(exclude_ids)
    .bind(min_score)
    .bind(top_k)
    .fetch_all(pool)
    .await
    .context("Failed to find similar articles by embedding")?;

    debug!(
        count = results.len(),
        top_k = top_k,
        min_score = min_score,
        "Found similar articles"
    );

    Ok(results)
}

#[instrument(skip(pool, embedding))]
pub async fn find_historical_by_embedding(
    pool: &PgPool,
    embedding: Vec<f32>,
    lookback_days: i32,
    top_k: i32,
    min_score: f64,
) -> Result<Vec<SimilarArticleResult>> {
    let vector = Vector::from(embedding);

    let results = sqlx::query_as::<_, SimilarArticleResult>(
        r#"
        SELECT
            ap.article_id,
            1 - (ap.embedding <=> $1) as score,
            a.title,
            a.source_name,
            a.canonical_url as url,
            a.published_at
        FROM article_profiles ap
        JOIN articles a ON a.id = ap.article_id
        WHERE ap.embedding IS NOT NULL
          AND 1 - (ap.embedding <=> $1) >= $4
          AND a.published_at IS NOT NULL
          AND a.published_at < NOW() - INTERVAL '1 day' * $2
        ORDER BY ap.embedding <=> $1
        LIMIT $3
        "#,
    )
    .bind(&vector)
    .bind(lookback_days)
    .bind(top_k)
    .bind(min_score)
    .fetch_all(pool)
    .await
    .context("Failed to find historical articles by embedding")?;

    debug!(
        count = results.len(),
        lookback_days = lookback_days,
        min_score = min_score,
        "Found historical articles"
    );

    Ok(results)
}

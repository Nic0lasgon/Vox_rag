use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::schema::Article;

#[instrument(skip(pool))]
pub async fn get_article_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Article>> {
    let article = sqlx::query_as::<_, Article>(
        r#"
        SELECT id, source_name, source_url, canonical_url, title, description,
               author, language, country, published_at, fetched_at,
               raw_text, cleaned_text, title_hash, content_hash, simhash,
               status, created_at, updated_at
        FROM articles
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch article by ID")?;

    Ok(article)
}

#[instrument(skip(pool))]
pub async fn get_articles_without_embedding(pool: &PgPool, limit: i64) -> Result<Vec<Article>> {
    let articles = sqlx::query_as::<_, Article>(
        r#"
        SELECT a.id, a.source_name, a.source_url, a.canonical_url, a.title, a.description,
               a.author, a.language, a.country, a.published_at, a.fetched_at,
               a.raw_text, a.cleaned_text, a.title_hash, a.content_hash, a.simhash,
               a.status, a.created_at, a.updated_at
        FROM articles a
        LEFT JOIN article_profiles ap ON ap.article_id = a.id
        WHERE ap.article_id IS NULL
          AND a.cleaned_text IS NOT NULL
          AND a.cleaned_text != ''
        ORDER BY a.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch articles without embedding")?;

    debug!(
        count = articles.len(),
        "Fetched articles without embeddings"
    );

    Ok(articles)
}

#[instrument(skip(pool, article))]
pub async fn insert_article(pool: &PgPool, article: &Article) -> Result<Article> {
    let inserted = sqlx::query_as::<_, Article>(
        r#"
        INSERT INTO articles (
            id, source_name, source_url, canonical_url, title, description,
            author, language, country, published_at, fetched_at,
            raw_text, cleaned_text, title_hash, content_hash, simhash, status
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        ON CONFLICT (canonical_url) DO UPDATE SET
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            raw_text = EXCLUDED.raw_text,
            cleaned_text = EXCLUDED.cleaned_text,
            content_hash = EXCLUDED.content_hash,
            simhash = EXCLUDED.simhash,
            updated_at = NOW()
        RETURNING id, source_name, source_url, canonical_url, title, description,
                  author, language, country, published_at, fetched_at,
                  raw_text, cleaned_text, title_hash, content_hash, simhash,
                  status, created_at, updated_at
        "#,
    )
    .bind(article.id)
    .bind(&article.source_name)
    .bind(&article.source_url)
    .bind(&article.canonical_url)
    .bind(&article.title)
    .bind(&article.description)
    .bind(&article.author)
    .bind(&article.language)
    .bind(&article.country)
    .bind(article.published_at)
    .bind(article.fetched_at)
    .bind(&article.raw_text)
    .bind(&article.cleaned_text)
    .bind(&article.title_hash)
    .bind(&article.content_hash)
    .bind(&article.simhash)
    .bind(&article.status)
    .fetch_one(pool)
    .await
    .context("Failed to insert article")?;

    info!(article_id = %inserted.id, "Article inserted/updated");

    Ok(inserted)
}

#[instrument(skip(pool, text))]
pub async fn update_article_cleaned_text(pool: &PgPool, id: Uuid, text: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE articles
        SET cleaned_text = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(text)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update article cleaned_text")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Article not found: {}", id);
    }

    debug!(article_id = %id, "Updated article cleaned_text");

    Ok(())
}

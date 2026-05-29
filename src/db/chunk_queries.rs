use anyhow::{Context, Result};
use chrono::Utc;
use pgvector::Vector;
use sqlx::PgPool;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::schema::{ArticleChunk, RagChunkResult};

#[instrument(skip(pool, chunks))]
pub async fn insert_chunks(
    pool: &PgPool,
    article_id: Uuid,
    model: &str,
    dimension: i32,
    chunks: &[(String, i32, Vec<f32>)],
) -> Result<()> {
    let now = Utc::now();
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    for (idx, (chunk_text, token_count, embedding_vec)) in chunks.iter().enumerate() {
        let vector = Vector::from(embedding_vec.clone());

        sqlx::query(
            r#"
            INSERT INTO article_chunks (
                article_id, chunk_index, chunk_text, token_count,
                embedding_model, embedding_dimension, embedding, embedded_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (article_id, chunk_index) DO UPDATE SET
                chunk_text = EXCLUDED.chunk_text,
                token_count = EXCLUDED.token_count,
                embedding_model = EXCLUDED.embedding_model,
                embedding_dimension = EXCLUDED.embedding_dimension,
                embedding = EXCLUDED.embedding,
                embedded_at = EXCLUDED.embedded_at
            "#,
        )
        .bind(article_id)
        .bind(idx as i32)
        .bind(chunk_text)
        .bind(*token_count)
        .bind(model)
        .bind(dimension)
        .bind(&vector)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .context("Failed to insert chunk")?;
    }

    tx.commit().await.context("Failed to commit transaction")?;

    info!(
        article_id = %article_id,
        chunk_count = chunks.len(),
        model = model,
        dimension = dimension,
        "Chunks inserted"
    );

    Ok(())
}

#[instrument(skip(pool))]
pub async fn get_chunks_by_article(pool: &PgPool, article_id: Uuid) -> Result<Vec<ArticleChunk>> {
    let chunks = sqlx::query_as::<_, ArticleChunk>(
        r#"
        SELECT id, article_id, chunk_index, chunk_text, token_count,
               section_title, embedding_model, embedding_dimension,
               embedding, embedded_at, created_at
        FROM article_chunks
        WHERE article_id = $1
        ORDER BY chunk_index ASC
        "#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch chunks by article")?;

    debug!(
        article_id = %article_id,
        count = chunks.len(),
        "Fetched chunks by article"
    );

    Ok(chunks)
}

#[instrument(skip(pool, embedding))]
pub async fn find_similar_chunks(
    pool: &PgPool,
    embedding: Vec<f32>,
    top_k: i32,
    min_score: f64,
) -> Result<Vec<RagChunkResult>> {
    let vector = Vector::from(embedding);

    let results = sqlx::query_as::<_, RagChunkResult>(
        r#"
        SELECT
            ac.id as chunk_id,
            ac.article_id,
            1 - (ac.embedding <=> $1) as score,
            ac.chunk_text,
            a.title,
            a.source_name,
            a.canonical_url as url,
            a.published_at
        FROM article_chunks ac
        JOIN articles a ON a.id = ac.article_id
        WHERE ac.embedding IS NOT NULL
          AND 1 - (ac.embedding <=> $1) >= $2
        ORDER BY ac.embedding <=> $1
        LIMIT $3
        "#,
    )
    .bind(&vector)
    .bind(min_score)
    .bind(top_k)
    .fetch_all(pool)
    .await
    .context("Failed to find similar chunks")?;

    debug!(
        count = results.len(),
        top_k = top_k,
        min_score = min_score,
        "Found similar chunks"
    );

    Ok(results)
}

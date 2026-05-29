use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::PgPool;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use super::schema::EmbeddingJob;

#[instrument(skip(pool))]
pub async fn create_embedding_job(
    pool: &PgPool,
    target_type: &str,
    target_id: Uuid,
    model: &str,
    input_hash: &str,
) -> Result<EmbeddingJob> {
    let job = sqlx::query_as::<_, EmbeddingJob>(
        r#"
        INSERT INTO embedding_jobs (id, target_type, target_id, model, input_hash, status, attempts, created_at)
        VALUES ($1, $2, $3, $4, $5, 'pending', 0, $6)
        RETURNING id, target_type, target_id, model, input_hash, status, attempts,
                  last_error, scheduled_at, started_at, completed_at, created_at
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(target_type)
    .bind(target_id)
    .bind(model)
    .bind(input_hash)
    .bind(Utc::now())
    .fetch_one(pool)
    .await
    .context("Failed to create embedding job")?;

    info!(
        job_id = %job.id,
        target_type = target_type,
        target_id = %target_id,
        "Embedding job created"
    );

    Ok(job)
}

#[instrument(skip(pool))]
pub async fn pick_pending_jobs(pool: &PgPool, limit: i64) -> Result<Vec<EmbeddingJob>> {
    let jobs = sqlx::query_as::<_, EmbeddingJob>(
        r#"
        SELECT id, target_type, target_id, model, input_hash, status, attempts,
               last_error, scheduled_at, started_at, completed_at, created_at
        FROM embedding_jobs
        WHERE status = 'pending'
          AND (scheduled_at IS NULL OR scheduled_at <= NOW())
        ORDER BY created_at ASC
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to pick pending jobs")?;

    debug!(count = jobs.len(), "Picked pending embedding jobs");

    Ok(jobs)
}

#[instrument(skip(pool))]
pub async fn mark_job_running(pool: &PgPool, id: Uuid, locked_by: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE embedding_jobs
        SET status = 'running',
            started_at = NOW(),
            attempts = COALESCE(attempts, 0) + 1
        WHERE id = $1 AND status IN ('pending', 'running')
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to mark job running")?;

    if result.rows_affected() == 0 {
        warn!(job_id = %id, "Job not found or not in expected state");
    } else {
        debug!(job_id = %id, locked_by = locked_by, "Job marked as running");
    }

    Ok(())
}

#[instrument(skip(pool))]
pub async fn mark_job_done(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE embedding_jobs
        SET status = 'done', completed_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to mark job done")?;

    if result.rows_affected() == 0 {
        warn!(job_id = %id, "Job not found when marking done");
    } else {
        info!(job_id = %id, "Job marked as done");
    }

    Ok(())
}

#[instrument(skip(pool, error))]
pub async fn mark_job_failed(pool: &PgPool, id: Uuid, error: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE embedding_jobs
        SET status = 'failed', last_error = $1
        WHERE id = $2
        "#,
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to mark job failed")?;

    if result.rows_affected() == 0 {
        warn!(job_id = %id, "Job not found when marking failed");
    } else {
        warn!(job_id = %id, error = error, "Job marked as failed");
    }

    Ok(())
}

#[instrument(skip(pool))]
pub async fn reschedule_job(pool: &PgPool, id: Uuid, delay_seconds: i64) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE embedding_jobs
        SET status = 'pending',
            scheduled_at = NOW() + INTERVAL '1 second' * $1
        WHERE id = $2
        "#,
    )
    .bind(delay_seconds)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to reschedule job")?;

    if result.rows_affected() == 0 {
        warn!(job_id = %id, "Job not found when rescheduling");
    } else {
        info!(job_id = %id, delay_seconds = delay_seconds, "Job rescheduled");
    }

    Ok(())
}

#[instrument(skip(pool))]
pub async fn is_already_embedded(
    pool: &PgPool,
    target_type: &str,
    target_id: Uuid,
    input_hash: &str,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM embedding_jobs
            WHERE target_type = $1
              AND target_id = $2
              AND input_hash = $3
              AND status = 'done'
        )
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(input_hash)
    .fetch_one(pool)
    .await
    .context("Failed to check if already embedded")?;

    Ok(exists)
}

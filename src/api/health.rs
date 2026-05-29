use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::AppState;
use crate::db::{article_queries, job_queries};
use crate::pipeline::profile::{build_article_profile_text, normalize_for_embedding, should_embed};
use crate::pipeline::similarity::{find_historical_context, find_similar_articles};
use crate::workers::embedding_worker::compute_input_hash;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Serialize)]
pub struct SimilarResponse {
    pub article_id: String,
    pub candidates: Vec<CandidateEntry>,
}

#[derive(Serialize)]
pub struct CandidateEntry {
    pub article_id: String,
    pub score: f64,
    pub relation_hint: String,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Deserialize)]
pub struct SimilarQuery {
    pub top_k: Option<i32>,
}

pub async fn similar_articles(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SimilarQuery>,
) -> Result<Json<SimilarResponse>, (StatusCode, String)> {
    let top_k = query.top_k.unwrap_or(30);

    let options = crate::pipeline::similarity::SimilarityOptions::default();

    let candidates =
        find_similar_articles(&state.pool, article_id, top_k, &options, &state.thresholds)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<CandidateEntry> = candidates
        .into_iter()
        .map(|c| CandidateEntry {
            article_id: c.article_id.to_string(),
            score: c.score,
            relation_hint: c.relation_hint.to_string(),
            title: c.title,
            source_name: c.source_name,
            url: c.url,
            published_at: c.published_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(SimilarResponse {
        article_id: article_id.to_string(),
        candidates: entries,
    }))
}

#[derive(Deserialize)]
pub struct HistoricalQuery {
    pub lookback_days: Option<i32>,
    pub top_k: Option<i32>,
}

pub async fn historical_context(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<HistoricalQuery>,
) -> Result<Json<SimilarResponse>, (StatusCode, String)> {
    let lookback_days = query.lookback_days.unwrap_or(30);
    let top_k = query.top_k.unwrap_or(20);

    let candidates = find_historical_context(
        &state.pool,
        article_id,
        lookback_days,
        top_k,
        &state.thresholds,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<CandidateEntry> = candidates
        .into_iter()
        .map(|c| CandidateEntry {
            article_id: c.article_id.to_string(),
            score: c.score,
            relation_hint: c.relation_hint.to_string(),
            title: c.title,
            source_name: c.source_name,
            url: c.url,
            published_at: c.published_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(SimilarResponse {
        article_id: article_id.to_string(),
        candidates: entries,
    }))
}

#[derive(Serialize)]
pub struct EmbedProfileResponse {
    pub status: String,
    pub article_id: String,
    pub job_id: Option<String>,
}

pub async fn create_article_profile_embedding(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
) -> Result<Json<EmbedProfileResponse>, (StatusCode, String)> {
    let article = article_queries::get_article_by_id(&state.pool, article_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Article {} not found", article_id),
            )
        })?;

    if !should_embed(&article) {
        return Ok(Json(EmbedProfileResponse {
            status: "skipped".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let profile_text = build_article_profile_text(&article);
    let normalized = normalize_for_embedding(&profile_text);
    let input_hash = compute_input_hash(&normalized);

    let is_dup =
        job_queries::is_already_embedded(&state.pool, "article_profile", article_id, &input_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_dup {
        return Ok(Json(EmbedProfileResponse {
            status: "already_embedded".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let job = job_queries::create_embedding_job(
        &state.pool,
        "article_profile",
        article_id,
        state.provider.model_name(),
        &input_hash,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(EmbedProfileResponse {
        status: "job_created".to_string(),
        article_id: article_id.to_string(),
        job_id: Some(job.id.to_string()),
    }))
}

pub async fn create_article_chunks_embeddings(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
) -> Result<Json<EmbedProfileResponse>, (StatusCode, String)> {
    let article = article_queries::get_article_by_id(&state.pool, article_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Article {} not found", article_id),
            )
        })?;

    let cleaned_text = match &article.cleaned_text {
        Some(text) if !text.is_empty() => text.as_str(),
        _ => {
            return Ok(Json(EmbedProfileResponse {
                status: "skipped".to_string(),
                article_id: article_id.to_string(),
                job_id: None,
            }));
        }
    };

    let word_count = cleaned_text.split_whitespace().count();
    if word_count < 500 {
        return Ok(Json(EmbedProfileResponse {
            status: "skipped".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let input_hash = compute_input_hash(cleaned_text);

    let is_dup =
        job_queries::is_already_embedded(&state.pool, "article_chunk", article_id, &input_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_dup {
        return Ok(Json(EmbedProfileResponse {
            status: "already_embedded".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let job = job_queries::create_embedding_job(
        &state.pool,
        "article_chunk",
        article_id,
        state.provider.model_name(),
        &input_hash,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(EmbedProfileResponse {
        status: "job_created".to_string(),
        article_id: article_id.to_string(),
        job_id: Some(job.id.to_string()),
    }))
}

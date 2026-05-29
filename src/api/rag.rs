use axum::{Json, extract::State, http::StatusCode};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::AppState;
use crate::pipeline::rag_search::{RagSearchOptions, search_relevant_chunks};

#[derive(Deserialize)]
pub struct RagSearchRequest {
    pub query: String,
    pub top_k: Option<i32>,
    pub language: Option<String>,
    pub published_after: Option<String>,
    pub published_before: Option<String>,
    pub sources: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct RagSearchResponse {
    pub query: String,
    pub results: Vec<RagResultEntry>,
}

#[derive(Serialize)]
pub struct RagResultEntry {
    pub chunk_id: String,
    pub article_id: String,
    pub score: f64,
    pub chunk_text: String,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
}

pub async fn rag_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RagSearchRequest>,
) -> Result<Json<RagSearchResponse>, (StatusCode, String)> {
    let published_after = request
        .published_after
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc());

    let published_before = request
        .published_before
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc());

    let options = RagSearchOptions {
        top_k: request.top_k.unwrap_or(20),
        min_score: 0.70,
        language: request.language,
        published_after,
        published_before,
        sources: request.sources,
    };

    let results = search_relevant_chunks(
        &state.pool,
        state.provider.as_ref(),
        &request.query,
        &options,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<RagResultEntry> = results
        .into_iter()
        .map(|c| RagResultEntry {
            chunk_id: c.chunk_id.to_string(),
            article_id: c.article_id.to_string(),
            score: c.score,
            chunk_text: c.chunk_text,
            title: c.title,
            source_name: c.source_name,
            url: c.url,
            published_at: c.published_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(RagSearchResponse {
        query: request.query,
        results: entries,
    }))
}

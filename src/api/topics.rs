use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::AppState;
use crate::db::{job_queries, topic_queries};
use crate::topic::{candidate, embedding, markdown, mutation};

#[derive(Serialize)]
pub struct TopicDetailResponse {
    pub id: String,
    pub title: String,
    pub status: String,
    pub category: Option<String>,
    pub language: Option<String>,
    pub short_summary: Option<String>,
    pub main_entities: serde_json::Value,
    pub article_count: Option<i32>,
    pub source_count: Option<i32>,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub topic_markdown: Option<String>,
    pub articles: Vec<TopicArticleEntry>,
    pub timeline: Vec<TimelineEventEntry>,
}

#[derive(Serialize)]
pub struct TopicArticleEntry {
    pub article_id: String,
    pub relation_type: String,
    pub confidence_score: f64,
    pub decision_method: Option<String>,
    pub is_primary: Option<bool>,
    pub is_timeline_source: Option<bool>,
    pub added_at: Option<String>,
}

#[derive(Serialize)]
pub struct TimelineEventEntry {
    pub id: String,
    pub event_date: String,
    pub title: String,
    pub summary: String,
    pub importance: Option<String>,
    pub event_type: Option<String>,
    pub confidence_score: Option<f64>,
}

pub async fn get_topic(
    State(state): State<Arc<AppState>>,
    Path(topic_id): Path<Uuid>,
) -> Result<Json<TopicDetailResponse>, (StatusCode, String)> {
    let topic = topic_queries::get_topic_by_id(&state.pool, topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Topic {} not found", topic_id),
            )
        })?;

    let articles = topic_queries::get_topic_articles(&state.pool, topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let timeline = topic_queries::get_timeline_events(&state.pool, topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let article_entries = articles
        .into_iter()
        .map(|a| TopicArticleEntry {
            article_id: a.article_id.to_string(),
            relation_type: a.relation_type,
            confidence_score: a.confidence_score,
            decision_method: a.decision_method,
            is_primary: a.is_primary,
            is_timeline_source: a.is_timeline_source,
            added_at: a.added_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    let timeline_entries = timeline
        .into_iter()
        .map(|e| TimelineEventEntry {
            id: e.id.to_string(),
            event_date: e.event_date.to_rfc3339(),
            title: e.title,
            summary: e.summary,
            importance: e.importance,
            event_type: e.event_type,
            confidence_score: e.confidence_score,
        })
        .collect();

    Ok(Json(TopicDetailResponse {
        id: topic.id.to_string(),
        title: topic.title,
        status: topic.status,
        category: topic.category,
        language: topic.language,
        short_summary: topic.short_summary,
        main_entities: topic.main_entities,
        article_count: topic.article_count,
        source_count: topic.source_count,
        first_seen_at: topic.first_seen_at.map(|dt| dt.to_rfc3339()),
        last_seen_at: topic.last_seen_at.map(|dt| dt.to_rfc3339()),
        topic_markdown: topic.topic_markdown,
        articles: article_entries,
        timeline: timeline_entries,
    }))
}

#[derive(Deserialize)]
pub struct CreateTopicRequest {
    pub title: String,
    pub article_ids: Option<Vec<Uuid>>,
    pub category: Option<String>,
    pub language: Option<String>,
}

#[derive(Serialize)]
pub struct CreateTopicResponse {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub status: String,
}

pub async fn create_topic(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateTopicRequest>,
) -> Result<Json<CreateTopicResponse>, (StatusCode, String)> {
    let slug = format!(
        "{}-{}",
        mutation::slugify(&request.title),
        &Uuid::new_v4().to_string()[..8]
    );

    let entities = serde_json::json!([]);

    let topic = topic_queries::insert_topic(
        &state.pool,
        &request.title,
        &slug,
        request.category.as_deref(),
        request.language.as_deref(),
        &entities,
        None,
        None,
        None,
        Some(state.provider.model_name()),
        None,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(article_ids) = request.article_ids {
        for article_id in article_ids {
            topic_queries::link_article_to_topic(
                &state.pool,
                topic.id,
                article_id,
                "manual",
                1.0,
                None,
                "manual",
                true,
                true,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
    }

    Ok(Json(CreateTopicResponse {
        id: topic.id.to_string(),
        title: topic.title,
        slug: topic.stable_slug.unwrap_or_default(),
        status: topic.status,
    }))
}

#[derive(Serialize)]
pub struct RebuildMarkdownResponse {
    pub topic_id: String,
    pub hash: String,
    pub word_count: usize,
}

pub async fn rebuild_markdown(
    State(state): State<Arc<AppState>>,
    Path(topic_id): Path<Uuid>,
) -> Result<Json<RebuildMarkdownResponse>, (StatusCode, String)> {
    let _topic = topic_queries::get_topic_by_id(&state.pool, topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Topic {} not found", topic_id),
            )
        })?;

    let md = markdown::build_topic_markdown(&state.pool, topic_id, state.topic_max_words)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let hash = markdown::compute_markdown_hash(&md);
    let word_count = md.split_whitespace().count();

    topic_queries::update_topic_summary(&state.pool, topic_id, None, None, Some(&md), Some(&hash))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RebuildMarkdownResponse {
        topic_id: topic_id.to_string(),
        hash,
        word_count,
    }))
}

#[derive(Serialize)]
pub struct ReembedResponse {
    pub topic_id: String,
    pub updated: bool,
}

pub async fn reembed_topic(
    State(state): State<Arc<AppState>>,
    Path(topic_id): Path<Uuid>,
) -> Result<Json<ReembedResponse>, (StatusCode, String)> {
    let _topic = topic_queries::get_topic_by_id(&state.pool, topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Topic {} not found", topic_id),
            )
        })?;

    let updated = embedding::ensure_topic_embedding(
        &state.pool,
        state.provider.as_ref(),
        topic_id,
        state.topic_max_words,
        true,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ReembedResponse {
        topic_id: topic_id.to_string(),
        updated,
    }))
}

#[derive(Deserialize)]
pub struct MergeTopicRequest {
    pub target_topic_id: Uuid,
}

#[derive(Serialize)]
pub struct MergeResponse {
    pub source_topic_id: String,
    pub target_topic_id: String,
    pub status: String,
}

pub async fn merge_topic(
    State(state): State<Arc<AppState>>,
    Path(source_topic_id): Path<Uuid>,
    Json(request): Json<MergeTopicRequest>,
) -> Result<Json<MergeResponse>, (StatusCode, String)> {
    let _source = topic_queries::get_topic_by_id(&state.pool, source_topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Source topic {} not found", source_topic_id),
            )
        })?;

    let _target = topic_queries::get_topic_by_id(&state.pool, request.target_topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Target topic {} not found", request.target_topic_id),
            )
        })?;

    topic_queries::merge_topic(&state.pool, source_topic_id, request.target_topic_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let md =
        markdown::build_topic_markdown(&state.pool, request.target_topic_id, state.topic_max_words)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let hash = markdown::compute_markdown_hash(&md);

    topic_queries::update_topic_summary(
        &state.pool,
        request.target_topic_id,
        None,
        None,
        Some(&md),
        Some(&hash),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    embedding::ensure_topic_embedding(
        &state.pool,
        state.provider.as_ref(),
        request.target_topic_id,
        state.topic_max_words,
        true,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(MergeResponse {
        source_topic_id: source_topic_id.to_string(),
        target_topic_id: request.target_topic_id.to_string(),
        status: "merged".to_string(),
    }))
}

#[derive(Serialize)]
pub struct TopicCandidatesResponse {
    pub article_id: String,
    pub candidates: Vec<CandidateEntry>,
}

#[derive(Serialize)]
pub struct CandidateEntry {
    pub topic_id: String,
    pub title: String,
    pub status: String,
    pub score: f64,
}

pub async fn get_topic_candidates(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
) -> Result<Json<TopicCandidatesResponse>, (StatusCode, String)> {
    let candidates = candidate::find_candidates(
        &state.pool,
        article_id,
        10,
        state.topic_thresholds.topic_weak_match,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries = candidates
        .into_iter()
        .map(|c| CandidateEntry {
            topic_id: c.topic_id.to_string(),
            title: c.title,
            status: c.status,
            score: c.score,
        })
        .collect();

    Ok(Json(TopicCandidatesResponse {
        article_id: article_id.to_string(),
        candidates: entries,
    }))
}

#[derive(Deserialize)]
pub struct AssignTopicRequest {
    pub topic_id: Option<Uuid>,
    pub relation_type: Option<String>,
    pub confidence: Option<f64>,
    pub create_new: Option<bool>,
}

#[derive(Serialize)]
pub struct AssignTopicResponse {
    pub article_id: String,
    pub topic_id: String,
    pub relation_type: String,
    pub confidence: f64,
    pub decision_method: String,
}

pub async fn assign_topic(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
    Json(request): Json<AssignTopicRequest>,
) -> Result<Json<AssignTopicResponse>, (StatusCode, String)> {
    let article = crate::db::article_queries::get_article_by_id(&state.pool, article_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Article {} not found", article_id),
            )
        })?;

    if request.create_new == Some(true) {
        let profile = crate::db::embedding_queries::get_article_profile(&state.pool, article_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let embedding_vec = profile.and_then(|p| p.embedding.map(|v| v.into()));

        let topic = mutation::create_topic_from_article(
            &state.pool,
            &article,
            embedding_vec,
            Some(state.provider.model_name()),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let link = topic_queries::link_article_to_topic(
            &state.pool,
            topic.id,
            article_id,
            request.relation_type.as_deref().unwrap_or("manual"),
            request.confidence.unwrap_or(1.0),
            None,
            "manual",
            true,
            true,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(Json(AssignTopicResponse {
            article_id: article_id.to_string(),
            topic_id: topic.id.to_string(),
            relation_type: link.relation_type,
            confidence: link.confidence_score,
            decision_method: link.decision_method.unwrap_or_default(),
        }))
    } else if let Some(topic_id) = request.topic_id {
        let _topic = topic_queries::get_topic_by_id(&state.pool, topic_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("Topic {} not found", topic_id),
                )
            })?;

        let link = topic_queries::link_article_to_topic(
            &state.pool,
            topic_id,
            article_id,
            request.relation_type.as_deref().unwrap_or("manual"),
            request.confidence.unwrap_or(1.0),
            None,
            "manual",
            true,
            true,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(Json(AssignTopicResponse {
            article_id: article_id.to_string(),
            topic_id: topic_id.to_string(),
            relation_type: link.relation_type,
            confidence: link.confidence_score,
            decision_method: link.decision_method.unwrap_or_default(),
        }))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Either topic_id or create_new=true must be provided".to_string(),
        ))
    }
}

#[derive(Deserialize)]
pub struct ProcessBatchRequest {
    pub article_ids: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct ProcessBatchResponse {
    pub job_ids: Vec<String>,
    pub count: usize,
}

pub async fn process_article_batch(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ProcessBatchRequest>,
) -> Result<Json<ProcessBatchResponse>, (StatusCode, String)> {
    let mut job_ids = Vec::new();

    for article_id in request.article_ids {
        let input_hash = format!("topic-batch-{}", article_id);
        let job = job_queries::create_embedding_job(
            &state.pool,
            "process_article_batch",
            article_id,
            state.provider.model_name(),
            &input_hash,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        job_ids.push(job.id.to_string());
    }

    Ok(Json(ProcessBatchResponse {
        count: job_ids.len(),
        job_ids,
    }))
}

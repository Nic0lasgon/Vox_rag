pub mod health;
pub mod rag;
pub mod topics;

use axum::{Router, routing::get};
use sqlx::PgPool;
use std::sync::Arc;

use crate::embedding::EmbeddingProvider;
use crate::pipeline::similarity::Thresholds;
use crate::topic::decision::TopicThresholds;

pub struct AppState {
    pub pool: PgPool,
    pub provider: Arc<dyn EmbeddingProvider>,
    pub thresholds: Thresholds,
    pub topic_thresholds: TopicThresholds,
    pub topic_max_words: usize,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route(
            "/internal/articles/{article_id}/similar",
            get(crate::api::health::similar_articles),
        )
        .route(
            "/internal/articles/{article_id}/historical-context",
            get(crate::api::health::historical_context),
        )
        .route(
            "/internal/embeddings/article-profile/{article_id}",
            axum::routing::post(crate::api::health::create_article_profile_embedding),
        )
        .route(
            "/internal/embeddings/article-chunks/{article_id}",
            axum::routing::post(crate::api::health::create_article_chunks_embeddings),
        )
        .route(
            "/internal/rag/search",
            axum::routing::post(crate::api::rag::rag_search),
        )
        .route(
            "/internal/topics",
            axum::routing::post(crate::api::topics::create_topic),
        )
        .route(
            "/internal/topics/{topic_id}",
            axum::routing::get(crate::api::topics::get_topic),
        )
        .route(
            "/internal/topics/{topic_id}/rebuild-markdown",
            axum::routing::post(crate::api::topics::rebuild_markdown),
        )
        .route(
            "/internal/topics/{topic_id}/reembed",
            axum::routing::post(crate::api::topics::reembed_topic),
        )
        .route(
            "/internal/topics/{topic_id}/merge",
            axum::routing::post(crate::api::topics::merge_topic),
        )
        .route(
            "/internal/articles/{article_id}/topic-candidates",
            axum::routing::get(crate::api::topics::get_topic_candidates),
        )
        .route(
            "/internal/articles/{article_id}/assign-topic",
            axum::routing::post(crate::api::topics::assign_topic),
        )
        .route(
            "/internal/jobs/process-article-batch",
            axum::routing::post(crate::api::topics::process_article_batch),
        )
        .with_state(state)
}

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tower::ServiceExt;
use vox_rag::api::{AppState, create_router};
use vox_rag::embedding::MockEmbeddingProvider;
use vox_rag::pipeline::similarity::Thresholds;
use vox_rag::topic::decision::TopicThresholds;

fn create_test_state() -> Arc<AppState> {
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost:5432/nonexistent")
        .expect("Failed to create lazy pool");

    let provider = Arc::new(MockEmbeddingProvider::new(4096));

    let thresholds = Thresholds::default();
    let topic_thresholds = TopicThresholds::default();

    Arc::new(AppState {
        pool,
        provider,
        thresholds,
        topic_thresholds,
        topic_max_words: 1500,
    })
}

#[tokio::test]
async fn chunk_embedding_endpoint_exists() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/embeddings/article-chunks/00000000-0000-0000-0000-000000000001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn chunk_embedding_requires_valid_uuid() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/embeddings/article-chunks/not-a-uuid")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_client_error());
}

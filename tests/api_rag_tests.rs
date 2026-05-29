use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
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
async fn rag_search_endpoint_exists() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"query": "test"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rag_search_requires_json_body() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn rag_search_with_valid_body() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"query": "what is the latest news"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn rag_search_with_all_options() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "query": "climate change",
                        "top_k": 10,
                        "language": "en",
                        "published_after": "2024-01-01",
                        "sources": ["Reuters", "AP"]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn rag_search_missing_query_field() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"not_query": "value"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

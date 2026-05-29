use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
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
async fn health_endpoint_returns_200() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_endpoint_returns_correct_json_body() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn health_endpoint_returns_json_content_type() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header")
        .to_str()
        .unwrap();

    assert!(content_type.starts_with("application/json"));
}

#[tokio::test]
async fn health_endpoint_idempotent() {
    let state = create_test_state();
    let app = create_router(state);

    let response1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let response2 = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
        .await
        .unwrap();
    let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();

    assert_eq!(body1, body2);
}

#[tokio::test]
async fn health_handler_directly_returns_ok() {
    let response = vox_rag::api::health::health_check().await;
    assert_eq!(response.status, "ok");
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn similar_articles_endpoint_exists() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/articles/00000000-0000-0000-0000-000000000001/similar")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(!response.status().is_success());
}

#[tokio::test]
async fn historical_context_endpoint_exists() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/articles/00000000-0000-0000-0000-000000000001/historical-context")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(!response.status().is_success());
}

#[tokio::test]
async fn create_article_profile_endpoint_exists() {
    let state = create_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/embeddings/article-profile/00000000-0000-0000-0000-000000000001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(!response.status().is_success());
}

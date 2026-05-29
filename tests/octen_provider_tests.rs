use httpmock::prelude::*;
use serde_json::json;
use vox_rag::embedding::{EmbeddingProvider, OctenEmbeddingProvider};

fn mock_success_response() -> serde_json::Value {
    json!({
        "code": 0,
        "msg": "success",
        "data": {
            "results": [
                {
                    "index": 0,
                    "embedding": [0.1, 0.2, 0.3, 0.4]
                }
            ],
            "model": "octen-embedding-8b"
        },
        "meta": {
            "usage": {
                "input_tokens": 128
            }
        }
    })
}

fn mock_batch_success_response() -> serde_json::Value {
    json!({
        "code": 0,
        "msg": "success",
        "data": {
            "results": [
                { "index": 0, "embedding": [0.1, 0.2, 0.3, 0.4] },
                { "index": 1, "embedding": [0.5, 0.6, 0.7, 0.8] },
                { "index": 2, "embedding": [0.9, 1.0, 1.1, 1.2] }
            ],
            "model": "octen-embedding-8b"
        },
        "meta": {
            "usage": {
                "input_tokens": 384
            }
        }
    })
}

#[tokio::test]
async fn successful_single_embedding() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST)
            .path("/embedding")
            .header("x-api-key", "test-api-key")
            .header("Content-Type", "application/json");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(mock_success_response());
    });

    let provider = OctenEmbeddingProvider::new(
        "test-api-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("Test article content").await;
    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    let embedding = result.unwrap();
    assert_eq!(embedding.len(), 4);
    assert!((embedding[0] - 0.1_f32).abs() < 1e-6);
    assert!((embedding[1] - 0.2_f32).abs() < 1e-6);
}

#[tokio::test]
async fn successful_batch_embedding() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST)
            .path("/embedding")
            .header("x-api-key", "test-api-key");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(mock_batch_success_response());
    });

    let provider = OctenEmbeddingProvider::new(
        "test-api-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let inputs = vec![
        "first text".to_string(),
        "second text".to_string(),
        "third text".to_string(),
    ];

    let result = provider.embed_batch(&inputs).await;
    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    let embeddings = result.unwrap();
    assert_eq!(embeddings.len(), 3);
    assert_eq!(embeddings[0].len(), 4);
    assert_eq!(embeddings[1].len(), 4);
    assert_eq!(embeddings[2].len(), 4);
}

#[tokio::test]
async fn error_401_unauthorized() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(401)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 401,
                "msg": "Unauthorized",
                "data": null,
                "meta": null
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "invalid-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(result.is_err(), "Expected error for 401, got: {:?}", result);
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Octen API error") || err.contains("401"),
        "Error message should mention 401, got: {}",
        err
    );
}

#[tokio::test]
async fn error_429_rate_limited() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(429)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 429,
                "msg": "Too Many Requests",
                "data": null,
                "meta": null
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(result.is_err(), "Expected error for 429, got: {:?}", result);
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Octen API error") || err.contains("429"),
        "Error message should mention 429, got: {}",
        err
    );
}

#[tokio::test]
async fn error_500_internal_server_error() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(500)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 500,
                "msg": "Internal Server Error",
                "data": null,
                "meta": null
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(result.is_err(), "Expected error for 500, got: {:?}", result);
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Octen API error") || err.contains("500"),
        "Error message should mention 500, got: {}",
        err
    );
}

#[tokio::test]
async fn invalid_response_body() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "text/html")
            .body("<!DOCTYPE html><html><body>Not JSON</body></html>");
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(
        result.is_err(),
        "Expected parse error for invalid JSON, got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("parse") || err.to_lowercase().contains("json"),
        "Error should be about parsing, got: {}",
        err
    );
}

#[tokio::test]
async fn dimension_mismatch_detection() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 0,
                "msg": "success",
                "data": {
                    "results": [
                        {
                            "index": 0,
                            "embedding": [0.1, 0.2, 0.3]
                        }
                    ],
                    "model": "octen-embedding-8b"
                },
                "meta": { "usage": { "input_tokens": 64 } }
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4096,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(
        result.is_err(),
        "Expected dimension mismatch error, got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("dimension"),
        "Error should mention dimension mismatch, got: {}",
        err
    );
}

#[tokio::test]
async fn batch_dimension_mismatch_detection() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 0,
                "msg": "success",
                "data": {
                    "results": [
                        { "index": 0, "embedding": [0.1, 0.2] },
                        { "index": 1, "embedding": [0.3, 0.4, 0.5, 0.6, 0.7] }
                    ],
                    "model": "octen-embedding-8b"
                },
                "meta": { "usage": { "input_tokens": 128 } }
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let inputs = vec!["text a".to_string(), "text b".to_string()];

    let result = provider.embed_batch(&inputs).await;
    assert!(
        result.is_err(),
        "Expected dimension mismatch error in batch, got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("dimension"),
        "Error should mention dimension mismatch, got: {}",
        err
    );
}

#[tokio::test]
async fn empty_batch_returns_empty_result() {
    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        "https://api.example.com".to_string(),
        "octen-embedding-8b".to_string(),
        4096,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let inputs: Vec<String> = vec![];
    let result = provider.embed_batch(&inputs).await;

    assert!(
        result.is_ok(),
        "Empty batch should succeed, got: {:?}",
        result
    );
    let embeddings = result.unwrap();
    assert!(embeddings.is_empty());
}

#[tokio::test]
async fn business_error_code_nonzero() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 1001,
                "msg": "Invalid model name",
                "data": null,
                "meta": null
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(
        result.is_err(),
        "Expected business error, got: {:?}",
        result
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("1001") || err.to_lowercase().contains("code"));
}

#[tokio::test]
async fn missing_data_field_in_response() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 0,
                "msg": "success",
                "data": null,
                "meta": null
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(
        result.is_err(),
        "Expected error for missing data, got: {:?}",
        result
    );
}

#[tokio::test]
async fn response_with_empty_results_array() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(POST).path("/embedding");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "code": 0,
                "msg": "success",
                "data": {
                    "results": [],
                    "model": "octen-embedding-8b"
                },
                "meta": { "usage": { "input_tokens": 0 } }
            }));
    });

    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        server.base_url(),
        "octen-embedding-8b".to_string(),
        4,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    let result = provider.embed_text("some text").await;
    assert!(
        result.is_err(),
        "Expected error for empty results, got: {:?}",
        result
    );
}

#[tokio::test]
async fn model_name_returns_correct_value() {
    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        "https://api.example.com".to_string(),
        "octen-embedding-8b".to_string(),
        4096,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    assert_eq!(provider.model_name(), "octen-embedding-8b");
}

#[tokio::test]
async fn dimension_returns_correct_value() {
    let provider = OctenEmbeddingProvider::new(
        "test-key".to_string(),
        "https://api.example.com".to_string(),
        "octen-embedding-8b".to_string(),
        4096,
        60000,
    )
    .expect("Failed to create OctenEmbeddingProvider");

    assert_eq!(provider.dimension(), 4096);
}

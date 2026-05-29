use chrono::Utc;
use uuid::Uuid;
use vox_rag::db::schema::RagChunkResult;
use vox_rag::pipeline::rag_search::{DiversifyConfig, diversify_results};

fn make_chunk(article_id: Uuid, source: &str, score: f64) -> RagChunkResult {
    RagChunkResult {
        chunk_id: Uuid::new_v4(),
        article_id,
        score,
        chunk_text: format!("chunk with score {}", score),
        title: Some("Title".to_string()),
        source_name: Some(source.to_string()),
        url: Some("https://example.com".to_string()),
        published_at: Some(Utc::now()),
    }
}

#[test]
fn limits_chunks_per_article() {
    let article_id = Uuid::new_v4();
    let raw: Vec<RagChunkResult> = (0..10)
        .map(|i| make_chunk(article_id, "Source", 0.9 - i as f64 * 0.01))
        .collect();

    let config = DiversifyConfig {
        max_chunks_per_article: 2,
        max_articles_per_source: 3,
        min_distinct_sources: 3,
    };

    let result = diversify_results(raw, &config);
    assert_eq!(result.len(), 2);
}

#[test]
fn limits_articles_per_source() {
    let articles: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    let mut raw: Vec<RagChunkResult> = Vec::new();
    for (i, article_id) in articles.iter().enumerate() {
        raw.push(make_chunk(*article_id, "Source", 0.9 - i as f64 * 0.01));
    }

    let config = DiversifyConfig {
        max_chunks_per_article: 2,
        max_articles_per_source: 3,
        min_distinct_sources: 3,
    };

    let result = diversify_results(raw, &config);
    assert_eq!(result.len(), 3);
}

#[test]
fn preserves_score_ordering() {
    let a1 = Uuid::new_v4();
    let a2 = Uuid::new_v4();
    let raw = vec![
        make_chunk(a1, "Source1", 0.95),
        make_chunk(a2, "Source2", 0.90),
        make_chunk(a1, "Source1", 0.85),
    ];

    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);

    for i in 0..result.len() - 1 {
        assert!(result[i].score >= result[i + 1].score);
    }
}

#[test]
fn empty_input_returns_empty() {
    let raw: Vec<RagChunkResult> = vec![];
    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);
    assert!(result.is_empty());
}

#[test]
fn mixed_sources_respects_limits() {
    let a1 = Uuid::new_v4();
    let a2 = Uuid::new_v4();
    let a3 = Uuid::new_v4();
    let a4 = Uuid::new_v4();

    let raw = vec![
        make_chunk(a1, "Reuters", 0.95),
        make_chunk(a2, "Reuters", 0.93),
        make_chunk(a3, "Reuters", 0.91),
        make_chunk(a4, "Reuters", 0.89),
        make_chunk(Uuid::new_v4(), "AP", 0.87),
    ];

    let config = DiversifyConfig {
        max_chunks_per_article: 2,
        max_articles_per_source: 3,
        min_distinct_sources: 2,
    };

    let result = diversify_results(raw, &config);
    let reuters_count = result
        .iter()
        .filter(|c| c.source_name.as_deref() == Some("Reuters"))
        .count();
    assert_eq!(reuters_count, 3);
    assert_eq!(result.len(), 4);
}

#[test]
fn no_source_name_still_included() {
    let article_id = Uuid::new_v4();
    let mut chunk = make_chunk(article_id, "Source", 0.95);
    chunk.source_name = None;

    let raw = vec![chunk];
    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);
    assert_eq!(result.len(), 1);
}

#[test]
fn multiple_sources_all_represented() {
    let a1 = Uuid::new_v4();
    let a2 = Uuid::new_v4();
    let a3 = Uuid::new_v4();

    let raw = vec![
        make_chunk(a1, "Reuters", 0.95),
        make_chunk(a2, "AP", 0.93),
        make_chunk(a3, "Bloomberg", 0.91),
    ];

    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);

    let sources: std::collections::HashSet<&str> = result
        .iter()
        .filter_map(|c| c.source_name.as_deref())
        .collect();
    assert_eq!(sources.len(), 3);
}

#[test]
fn single_source_many_articles() {
    let articles: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
    let mut raw: Vec<RagChunkResult> = Vec::new();
    for (i, article_id) in articles.iter().enumerate() {
        raw.push(make_chunk(*article_id, "Reuters", 0.9 - i as f64 * 0.01));
    }

    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);

    let unique_articles: std::collections::HashSet<Uuid> =
        result.iter().map(|c| c.article_id).collect();
    assert_eq!(unique_articles.len(), 3);
}

#[test]
fn diversify_respects_top_k_after_diversification() {
    let mut raw: Vec<RagChunkResult> = Vec::new();
    for i in 0..30 {
        raw.push(make_chunk(
            Uuid::new_v4(),
            &format!("Source{}", i % 6),
            0.99 - i as f64 * 0.001,
        ));
    }

    let config = DiversifyConfig::default();
    let diversified = diversify_results(raw, &config);

    let top_k: usize = 10;
    let result: Vec<_> = diversified.into_iter().take(top_k).collect();

    assert!(result.len() <= top_k);
    assert!(!result.is_empty());

    for i in 0..result.len() - 1 {
        assert!(result[i].score >= result[i + 1].score);
    }
}

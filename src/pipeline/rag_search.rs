use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::chunk_queries;
use crate::db::schema::RagChunkResult;
use crate::embedding::EmbeddingProvider;

#[derive(Debug, Clone)]
pub struct DiversifyConfig {
    pub max_chunks_per_article: usize,
    pub max_articles_per_source: usize,
    pub min_distinct_sources: usize,
}

impl Default for DiversifyConfig {
    fn default() -> Self {
        Self {
            max_chunks_per_article: 2,
            max_articles_per_source: 3,
            min_distinct_sources: 3,
        }
    }
}

pub fn diversify_results(
    raw: Vec<RagChunkResult>,
    config: &DiversifyConfig,
) -> Vec<RagChunkResult> {
    let mut result = Vec::new();
    let mut article_chunk_counts: HashMap<Uuid, usize> = HashMap::new();
    let mut source_article_counts: HashMap<String, HashSet<Uuid>> = HashMap::new();

    for chunk in raw {
        let count = article_chunk_counts.entry(chunk.article_id).or_insert(0);
        if *count >= config.max_chunks_per_article {
            continue;
        }

        if let Some(ref source) = chunk.source_name {
            let articles = source_article_counts.entry(source.clone()).or_default();
            if !articles.contains(&chunk.article_id)
                && articles.len() >= config.max_articles_per_source
            {
                continue;
            }
            articles.insert(chunk.article_id);
        }

        *count += 1;
        result.push(chunk);
    }

    result
}

#[derive(Debug, Clone)]
pub struct RagSearchOptions {
    pub top_k: i32,
    pub min_score: f64,
    pub language: Option<String>,
    pub published_after: Option<DateTime<Utc>>,
    pub published_before: Option<DateTime<Utc>>,
    pub sources: Option<Vec<String>>,
}

impl Default for RagSearchOptions {
    fn default() -> Self {
        Self {
            top_k: 20,
            min_score: 0.70,
            language: None,
            published_after: None,
            published_before: None,
            sources: None,
        }
    }
}

#[instrument(skip(pool, provider, options))]
pub async fn search_relevant_chunks(
    pool: &PgPool,
    provider: &dyn EmbeddingProvider,
    query: &str,
    options: &RagSearchOptions,
) -> Result<Vec<RagChunkResult>> {
    let embedding = provider
        .embed_text(query)
        .await
        .context("Failed to embed query")?;

    let fetch_limit = options.top_k * 3;
    let mut raw =
        chunk_queries::find_similar_chunks(pool, embedding, fetch_limit, options.min_score)
            .await
            .context("Failed to find similar chunks")?;

    if let Some(ref sources) = options.sources {
        let source_set: HashSet<&str> = sources.iter().map(|s| s.as_str()).collect();
        raw.retain(|c| {
            c.source_name
                .as_ref()
                .map(|s| source_set.contains(s.as_str()))
                .unwrap_or(false)
        });
    }

    if let Some(after) = options.published_after {
        raw.retain(|c| c.published_at.map(|dt| dt >= after).unwrap_or(false));
    }

    if let Some(before) = options.published_before {
        raw.retain(|c| c.published_at.map(|dt| dt <= before).unwrap_or(false));
    }

    let diversify_config = DiversifyConfig::default();
    let diversified = diversify_results(raw, &diversify_config);

    let results: Vec<RagChunkResult> = diversified
        .into_iter()
        .take(options.top_k as usize)
        .collect();

    let distinct_sources: HashSet<&str> = results
        .iter()
        .filter_map(|c| c.source_name.as_deref())
        .collect();

    info!(
        query = query,
        result_count = results.len(),
        distinct_sources = distinct_sources.len(),
        "RAG search completed"
    );

    Ok(results)
}

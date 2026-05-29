use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::embedding_queries;

#[derive(Debug, Clone)]
pub struct SimilarityOptions {
    pub min_score: f64,
    pub language: Option<String>,
    pub exclude_same_source: bool,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
}

impl Default for SimilarityOptions {
    fn default() -> Self {
        Self {
            min_score: 0.70,
            language: None,
            exclude_same_source: false,
            date_from: None,
            date_to: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimilarArticleCandidate {
    pub article_id: Uuid,
    pub score: f64,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub relation_hint: &'static str,
}

#[derive(Debug, Clone)]
pub struct Thresholds {
    pub near_duplicate: f64,
    pub same_story: f64,
    pub context: f64,
    pub weak: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            near_duplicate: 0.92,
            same_story: 0.86,
            context: 0.78,
            weak: 0.70,
        }
    }
}

pub fn classify_relation(score: f64, thresholds: &Thresholds) -> &'static str {
    if score >= thresholds.near_duplicate {
        "near_duplicate"
    } else if score >= thresholds.same_story {
        "same_story"
    } else if score >= thresholds.context {
        "context"
    } else if score >= thresholds.weak {
        "weak"
    } else {
        "unrelated"
    }
}

#[instrument(skip(pool, thresholds))]
pub async fn find_similar_articles(
    pool: &PgPool,
    article_id: Uuid,
    top_k: i32,
    options: &SimilarityOptions,
    thresholds: &Thresholds,
) -> Result<Vec<SimilarArticleCandidate>> {
    let profile = embedding_queries::get_article_profile(pool, article_id)
        .await
        .context("Failed to fetch article profile")?;

    let profile = profile.context("No embedding profile found for article")?;

    let embedding_vec: Vec<f32> = profile
        .embedding
        .context("Article profile has no embedding")?
        .into();

    let exclude_ids = vec![article_id];

    let similar = embedding_queries::find_similar_articles(
        pool,
        embedding_vec,
        top_k,
        &exclude_ids,
        options.min_score,
    )
    .await
    .context("Failed to find similar articles")?;

    let mut candidates: Vec<SimilarArticleCandidate> = similar
        .into_iter()
        .filter(|result| {
            if options.exclude_same_source
                && let Some(ref _source) = result.source_name
            {}
            true
        })
        .map(|result| {
            let relation_hint = classify_relation(result.score, thresholds);
            SimilarArticleCandidate {
                article_id: result.article_id,
                score: result.score,
                title: result.title,
                source_name: result.source_name,
                url: result.url,
                published_at: result.published_at,
                relation_hint,
            }
        })
        .collect();

    candidates.truncate(top_k as usize);

    info!(
        article_id = %article_id,
        found = candidates.len(),
        min_score = options.min_score,
        "Found similar articles"
    );

    Ok(candidates)
}

#[instrument(skip(pool, thresholds))]
pub async fn find_historical_context(
    pool: &PgPool,
    article_id: Uuid,
    lookback_days: i32,
    top_k: i32,
    thresholds: &Thresholds,
) -> Result<Vec<SimilarArticleCandidate>> {
    let min_score = 0.74;

    let profile = embedding_queries::get_article_profile(pool, article_id)
        .await
        .context("Failed to fetch article profile")?;

    let profile = profile.context("No embedding profile found for article")?;

    let embedding_vec: Vec<f32> = profile
        .embedding
        .context("Article profile has no embedding")?
        .into();

    let similar = embedding_queries::find_historical_by_embedding(
        pool,
        embedding_vec,
        lookback_days,
        top_k,
        min_score,
    )
    .await
    .context("Failed to find historical similar articles")?;

    let candidates: Vec<SimilarArticleCandidate> = similar
        .into_iter()
        .map(|result| {
            let relation_hint = classify_relation(result.score, thresholds);
            SimilarArticleCandidate {
                article_id: result.article_id,
                score: result.score,
                title: result.title,
                source_name: result.source_name,
                url: result.url,
                published_at: result.published_at,
                relation_hint,
            }
        })
        .collect();

    info!(
        article_id = %article_id,
        found = candidates.len(),
        lookback_days = lookback_days,
        min_score = min_score,
        "Found historical context articles"
    );

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_relation() {
        let thresholds = Thresholds::default();

        assert_eq!(classify_relation(0.95, &thresholds), "near_duplicate");
        assert_eq!(classify_relation(0.92, &thresholds), "near_duplicate");
        assert_eq!(classify_relation(0.89, &thresholds), "same_story");
        assert_eq!(classify_relation(0.86, &thresholds), "same_story");
        assert_eq!(classify_relation(0.82, &thresholds), "context");
        assert_eq!(classify_relation(0.78, &thresholds), "context");
        assert_eq!(classify_relation(0.74, &thresholds), "weak");
        assert_eq!(classify_relation(0.70, &thresholds), "weak");
        assert_eq!(classify_relation(0.65, &thresholds), "unrelated");
    }

    #[test]
    fn test_classify_relation_custom_thresholds() {
        let thresholds = Thresholds {
            near_duplicate: 0.95,
            same_story: 0.90,
            context: 0.85,
            weak: 0.80,
        };

        assert_eq!(classify_relation(0.93, &thresholds), "same_story");
        assert_eq!(classify_relation(0.88, &thresholds), "context");
        assert_eq!(classify_relation(0.82, &thresholds), "weak");
        assert_eq!(classify_relation(0.75, &thresholds), "unrelated");
    }
}

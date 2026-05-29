use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Article {
    pub id: Uuid,
    pub source_name: String,
    pub source_url: Option<String>,
    pub canonical_url: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub country: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub raw_text: Option<String>,
    pub cleaned_text: Option<String>,
    pub title_hash: Option<String>,
    pub content_hash: Option<String>,
    pub simhash: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleProfile {
    pub article_id: Uuid,
    pub profile_text: String,
    pub short_summary: Option<String>,
    pub main_event: Option<String>,
    pub entities: Option<Value>,
    pub keywords: Option<Value>,
    pub embedding_model: Option<String>,
    pub embedding_dimension: Option<i32>,
    pub embedding: Option<Vector>,
    pub embedded_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmbeddingJob {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub model: String,
    pub input_hash: String,
    pub status: String,
    pub attempts: Option<i32>,
    pub last_error: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SimilarArticleResult {
    pub article_id: Uuid,
    pub score: f64,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleChunk {
    pub id: Uuid,
    pub article_id: Uuid,
    pub chunk_index: i32,
    pub chunk_text: String,
    pub token_count: Option<i32>,
    pub section_title: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_dimension: Option<i32>,
    pub embedding: Option<Vector>,
    pub embedded_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RagChunkResult {
    pub chunk_id: Uuid,
    pub article_id: Uuid,
    pub score: f64,
    pub chunk_text: String,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Topic {
    pub id: Uuid,
    pub title: String,
    pub stable_slug: Option<String>,
    pub status: String,
    pub category: Option<String>,
    pub language: Option<String>,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub main_entities: Value,
    pub secondary_entities: Value,
    pub keywords: Value,
    pub short_summary: Option<String>,
    pub long_summary: Option<String>,
    pub topic_markdown: Option<String>,
    pub topic_markdown_hash: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_dimension: Option<i32>,
    pub topic_embedding: Option<Vector>,
    pub embedded_at: Option<DateTime<Utc>>,
    pub importance_score: Option<f64>,
    pub confidence_score: Option<f64>,
    pub article_count: Option<i32>,
    pub source_count: Option<i32>,
    pub merged_into_topic_id: Option<Uuid>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TopicArticle {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub article_id: Uuid,
    pub relation_type: String,
    pub confidence_score: f64,
    pub similarity_score: Option<f64>,
    pub decision_method: Option<String>,
    pub is_primary: Option<bool>,
    pub is_timeline_source: Option<bool>,
    pub added_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TopicTimelineEvent {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub event_date: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub importance: Option<String>,
    pub event_type: Option<String>,
    pub source_article_ids: Option<Vec<Uuid>>,
    pub confidence_score: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TopicCandidate {
    pub topic_id: Uuid,
    pub title: String,
    pub status: String,
    pub score: f64,
    pub last_seen_at: Option<DateTime<Utc>>,
}

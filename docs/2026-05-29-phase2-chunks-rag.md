# Phase 2 — Chunks RAG Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouper les articles en chunks, les embedder, et permettre la recherche RAG de passages precis via `POST /internal/rag/search`.

**Architecture:** Le chunking est une fonction pure (pas de DB). Les chunks sont stockes dans `article_chunks` avec leur embedding pgvector. Le service RAG embedde la query, cherche les chunks les plus proches, et diversifie les resultats par source/article. Le worker existant est etendu pour traiter les jobs `article_chunk`.

**Tech Stack:** Rust + Tokio + sqlx + pgvector + Octen API (meme stack que Phase 1)

---

## File Structure

| Action | Fichier | Responsabilite |
|--------|---------|----------------|
| Create | `src/pipeline/chunker.rs` | Decoupage texte en chunks (pure function) |
| Create | `migrations/006_article_chunks.sql` | Table article_chunks + index vectoriel |
| Create | `src/db/chunk_queries.rs` | CRUD chunks + recherche pgvector |
| Create | `src/pipeline/rag_search.rs` | Service RAG : embed query → search → diversify |
| Create | `src/api/rag.rs` | Endpoint POST /internal/rag/search |
| Modify | `src/db/schema.rs` | Ajouter struct ArticleChunk + RagChunkResult |
| Modify | `src/db/mod.rs` | Export chunk_queries |
| Modify | `src/pipeline/mod.rs` | Export chunker + rag_search |
| Modify | `src/workers/embedding_worker.rs` | Ajouter traitement jobs article_chunk |
| Modify | `src/api/mod.rs` | Ajouter route RAG + import module rag |
| Modify | `src/api/health.rs` | Ajouter endpoint POST /internal/embeddings/article-chunks/:id |
| Create | `tests/chunker_tests.rs` | Tests unitaires du chunker |
| Create | `tests/rag_search_tests.rs` | Tests de la diversification RAG |

---

## Task 1: Chunker (fonction pure)

**Files:**
- Create: `src/pipeline/chunker.rs`
- Test: `tests/chunker_tests.rs`

**Principe :** Decoupe un texte en chunks de 700-1200 tokens avec un overlap de 100-150 tokens. Le token counting MVP utilise le decoupage par whitespace (1 mot ≈ 1 token, approximation acceptable pour du texte journalistique).

- [ ] **Step 1: Ecrire le test du chunker**

```rust
// tests/chunker_tests.rs
use vox_rag::pipeline::chunker::{chunk_text, ChunkConfig};

#[test]
fn chunk_text_splits_long_text() {
    let words: Vec<String> = (0..2000).map(|i| format!("word{}", i)).collect();
    let text = words.join(" ");
    let config = ChunkConfig::default();
    let chunks = chunk_text(&text, &config);
    assert!(chunks.len() > 1, "Should produce multiple chunks");
    for chunk in &chunks {
        let token_count = chunk.text.split_whitespace().count();
        assert!(
            token_count >= config.min_chunk_size as usize
                && token_count <= config.max_chunk_size as usize,
            "Chunk has {} tokens, expected {}-{}",
            token_count,
            config.min_chunk_size,
            config.max_chunk_size
        );
    }
}

#[test]
fn chunk_text_short_text_returns_single_chunk() {
    let text = "Short text under 700 tokens.";
    let config = ChunkConfig::default();
    let chunks = chunk_text(text, &config);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_index, 0);
}

#[test]
fn chunk_text_has_overlap() {
    let words: Vec<String> = (0..2000).map(|i| format!("word{}", i)).to_vec();
    let text = words.join(" ");
    let config = ChunkConfig::default();
    let chunks = chunk_text(&text, &config);
    if chunks.len() > 1 {
        let overlap_words_first: Vec<&str> = chunks[0].text.split_whitespace().collect();
        let overlap_words_second: Vec<&str> = chunks[1].text.split_whitespace().collect();
        let tail_of_first: Vec<&&str> = overlap_words_first
            [overlap_words_first.len() - config.overlap as usize..]
            .iter()
            .collect();
        let head_of_second: Vec<&&str> =
            overlap_words_second[..config.overlap as usize].iter().collect();
        assert_eq!(tail_of_first, head_of_second);
    }
}

#[test]
fn chunk_text_empty_returns_empty() {
    let chunks = chunk_text("", &ChunkConfig::default());
    assert!(chunks.is_empty());
}

#[test]
fn chunk_text_preserves_content() {
    let words: Vec<String> = (0..500).map(|i| format!("word{}", i)).collect();
    let text = words.join(" ");
    let chunks = chunk_text(&text, &ChunkConfig::default());
    let first_words: Vec<&str> = chunks[0].text.split_whitespace().collect();
    assert_eq!(first_words[0], "word0");
}

#[test]
fn chunk_config_custom_values() {
    let config = ChunkConfig {
        min_chunk_size: 100,
        max_chunk_size: 200,
        overlap: 20,
    };
    let words: Vec<String> = (0..500).map(|i| format!("w{}", i)).collect();
    let text = words.join(" ");
    let chunks = chunk_text(&text, &config);
    assert!(chunks.len() > 1);
}
```

- [ ] **Step 2: Run test pour verifier qu'il echoue**

Run: `cargo test --test chunker_tests 2>&1`
Expected: compile error (module not found)

- [ ] **Step 3: Implementer le chunker**

```rust
// src/pipeline/chunker.rs
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    pub min_chunk_size: usize,
    pub max_chunk_size: usize,
    pub overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            min_chunk_size: 700,
            max_chunk_size: 1200,
            overlap: 150,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextChunk {
    pub chunk_index: usize,
    pub text: String,
    pub token_count: usize,
}

pub fn chunk_text(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    if text.trim().is_empty() {
        return vec![];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let total_words = words.len();

    if total_words <= config.max_chunk_size {
        return vec![TextChunk {
            chunk_index: 0,
            text: text.to_string(),
            token_count: total_words,
        }];
    }

    let mut chunks = Vec::new();
    let step = config.max_chunk_size.saturating_sub(config.overlap);
    let mut start = 0;
    let mut index = 0;

    while start < total_words {
        let end = (start + config.max_chunk_size).min(total_words);
        let chunk_words = &words[start..end];
        let chunk_text = chunk_words.join(" ");

        chunks.push(TextChunk {
            chunk_index: index,
            text: chunk_text,
            token_count: chunk_words.len(),
        });

        if end >= total_words {
            break;
        }

        start += step;
        index += 1;
    }

    chunks
}
```

- [ ] **Step 4: Ajouter `pub mod chunker;` dans `src/pipeline/mod.rs`**

- [ ] **Step 5: Run test pour verifier qu'il passe**

Run: `cargo test --test chunker_tests 2>&1`
Expected: 6 tests pass

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: add text chunker with configurable size and overlap"
```

---

## Task 2: Migration article_chunks

**Files:**
- Create: `migrations/006_article_chunks.sql`

- [ ] **Step 1: Creer la migration**

```sql
-- migrations/006_article_chunks.sql
CREATE TABLE article_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    token_count INTEGER,
    section_title TEXT,
    embedding_model TEXT,
    embedding_dimension INTEGER,
    embedding VECTOR(4096),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(article_id, chunk_index)
);

CREATE INDEX idx_article_chunks_article_id ON article_chunks (article_id);
CREATE INDEX idx_article_chunks_embedding_hnsw
    ON article_chunks USING hnsw (embedding vector_cosine_ops);
```

- [ ] **Step 2: Ajouter le struct dans schema.rs**

Ajouter a `src/db/schema.rs` :

```rust
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
```

- [ ] **Step 3: Verifier la compilation**

Run: `cargo check 2>&1`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add article_chunks table migration and schema types"
```

---

## Task 3: DB chunk queries

**Files:**
- Create: `src/db/chunk_queries.rs`
- Modify: `src/db/mod.rs` (ajouter `pub mod chunk_queries;`)

- [ ] **Step 1: Implementer chunk_queries.rs**

```rust
// src/db/chunk_queries.rs
use anyhow::{Context, Result};
use chrono::Utc;
use pgvector::Vector;
use sqlx::PgPool;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use super::schema::{ArticleChunk, RagChunkResult};

#[instrument(skip(pool))]
pub async fn insert_chunks(
    pool: &PgPool,
    article_id: Uuid,
    model: &str,
    dimension: i32,
    chunks: &[(String, i32, Vec<f32>)],
) -> Result<()> {
    for (chunk_index, (text, token_count, embedding)) in chunks.iter().enumerate() {
        let vector = Vector::from(embedding.clone());
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO article_chunks (
                article_id, chunk_index, chunk_text, token_count,
                embedding_model, embedding_dimension, embedding, embedded_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (article_id, chunk_index) DO UPDATE SET
                chunk_text = EXCLUDED.chunk_text,
                token_count = EXCLUDED.token_count,
                embedding_model = EXCLUDED.embedding_model,
                embedding_dimension = EXCLUDED.embedding_dimension,
                embedding = EXCLUDED.embedding,
                embedded_at = EXCLUDED.embedded_at
            "#,
        )
        .bind(article_id)
        .bind(chunk_index as i32)
        .bind(text)
        .bind(*token_count)
        .bind(model)
        .bind(dimension)
        .bind(&vector)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .context("Failed to insert chunk")?;
    }

    info!(
        article_id = %article_id,
        count = chunks.len(),
        "Chunks inserted"
    );
    Ok(())
}

#[instrument(skip(pool))]
pub async fn get_chunks_by_article(
    pool: &PgPool,
    article_id: Uuid,
) -> Result<Vec<ArticleChunk>> {
    let chunks = sqlx::query_as::<_, ArticleChunk>(
        r#"
        SELECT id, article_id, chunk_index, chunk_text, token_count,
               section_title, embedding_model, embedding_dimension,
               embedding, embedded_at, created_at
        FROM article_chunks
        WHERE article_id = $1
        ORDER BY chunk_index ASC
        "#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch chunks")?;

    Ok(chunks)
}

#[instrument(skip(pool, embedding))]
pub async fn find_similar_chunks(
    pool: &PgPool,
    embedding: Vec<f32>,
    top_k: i32,
    min_score: f64,
) -> Result<Vec<RagChunkResult>> {
    let vector = Vector::from(embedding);

    let results = sqlx::query_as::<_, RagChunkResult>(
        r#"
        SELECT
            ac.id as chunk_id,
            ac.article_id,
            1 - (ac.embedding <=> $1) as score,
            ac.chunk_text,
            a.title,
            a.source_name,
            a.canonical_url as url,
            a.published_at
        FROM article_chunks ac
        JOIN articles a ON a.id = ac.article_id
        WHERE ac.embedding IS NOT NULL
          AND 1 - (ac.embedding <=> $1) >= $2
        ORDER BY ac.embedding <=> $1
        LIMIT $3
        "#,
    )
    .bind(&vector)
    .bind(min_score)
    .bind(top_k)
    .fetch_all(pool)
    .await
    .context("Failed to find similar chunks")?;

    debug!(count = results.len(), "Found similar chunks");
    Ok(results)
}
```

- [ ] **Step 2: Ajouter `pub mod chunk_queries;` dans `src/db/mod.rs`**

- [ ] **Step 3: Verifier compilation**

Run: `cargo check 2>&1`

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add chunk DB queries (insert, get, search)"
```

---

## Task 4: Service RAG avec diversification

**Files:**
- Create: `src/pipeline/rag_search.rs`
- Test: `tests/rag_search_tests.rs`

**Principe :** La diversification garantit qu'on ne retourne pas 10 chunks du meme article. Regles MVP : max 2 chunks par article, max 3 articles par source, minimum 3 sources distinctes si possible.

- [ ] **Step 1: Ecrire les tests de diversification**

```rust
// tests/rag_search_tests.rs
use vox_rag::pipeline::rag_search::{diversify_results, DiversifyConfig};
use vox_rag::db::schema::RagChunkResult;
use chrono::Utc;
use uuid::Uuid;

fn make_chunk(article_id: Uuid, source: &str, score: f64) -> RagChunkResult {
    RagChunkResult {
        chunk_id: Uuid::new_v4(),
        article_id,
        score,
        chunk_text: "test".to_string(),
        title: Some("Test".to_string()),
        source_name: Some(source.to_string()),
        url: Some("https://example.com".to_string()),
        published_at: Some(Utc::now()),
    }
}

#[test]
fn diversify_limits_chunks_per_article() {
    let article_id = Uuid::new_v4();
    let raw: Vec<RagChunkResult> = (0..10)
        .map(|i| make_chunk(article_id, "Reuters", 0.9 - i as f64 * 0.01))
        .collect();
    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);
    let count = result.iter().filter(|r| r.article_id == article_id).count();
    assert!(count <= config.max_chunks_per_article);
}

#[test]
fn diversify_limits_articles_per_source() {
    let articles: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    let raw: Vec<RagChunkResult> = articles
        .iter()
        .flat_map(|id| {
            (0..3).map(move |i| make_chunk(*id, "Reuters", 0.9 - i as f64 * 0.05))
        })
        .collect();
    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);
    let reuters_articles: std::collections::HashSet<Uuid> = result
        .iter()
        .filter(|r| r.source_name.as_deref() == Some("Reuters"))
        .map(|r| r.article_id)
        .collect();
    assert!(reuters_articles.len() <= config.max_articles_per_source);
}

#[test]
fn diversify_preserves_score_ordering() {
    let a1 = Uuid::new_v4();
    let a2 = Uuid::new_v4();
    let a3 = Uuid::new_v4();
    let raw = vec![
        make_chunk(a1, "AFP", 0.95),
        make_chunk(a2, "Reuters", 0.90),
        make_chunk(a3, "BBC", 0.85),
        make_chunk(a1, "AFP", 0.80),
        make_chunk(a2, "Reuters", 0.75),
    ];
    let config = DiversifyConfig::default();
    let result = diversify_results(raw, &config);
    for i in 1..result.len() {
        assert!(result[i - 1].score >= result[i].score);
    }
}

#[test]
fn diversify_empty_input_returns_empty() {
    let result = diversify_results(vec![], &DiversifyConfig::default());
    assert!(result.is_empty());
}
```

- [ ] **Step 2: Run test pour verifier l'echec**

Run: `cargo test --test rag_search_tests 2>&1`

- [ ] **Step 3: Implementer rag_search.rs**

```rust
// src/pipeline/rag_search.rs
use std::collections::{HashMap, HashSet};

use crate::db::chunk_queries;
use crate::db::schema::RagChunkResult;
use crate::embedding::EmbeddingProvider;
use sqlx::PgPool;
use anyhow::{Context, Result};
use tracing::{info, instrument};

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
    let mut article_count: HashMap<uuid::Uuid, usize> = HashMap::new();
    let mut source_article_count: HashMap<String, HashSet<uuid::Uuid>> = HashMap::new();

    let mut selected = Vec::new();

    for chunk in raw {
        let article_id = chunk.article_id;
        let source = chunk
            .source_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let current_article_chunks = *article_count.get(&article_id).unwrap_or(&0);
        if current_article_chunks >= config.max_chunks_per_article {
            continue;
        }

        let source_articles = source_article_count.entry(source.clone()).or_default();
        if source_articles.len() >= config.max_articles_per_source
            && !source_articles.contains(&article_id)
        {
            continue;
        }

        article_count.entry(article_id).and_modify(|c| *c += 1).or_insert(1);
        source_articles.insert(article_id);
        selected.push(chunk);
    }

    selected
}

#[derive(Debug, Clone)]
pub struct RagSearchOptions {
    pub top_k: i32,
    pub min_score: f64,
    pub language: Option<String>,
    pub published_after: Option<chrono::DateTime<chrono::Utc>>,
    pub published_before: Option<chrono::DateTime<chrono::Utc>>,
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

#[instrument(skip(pool, provider))]
pub async fn search_relevant_chunks(
    pool: &PgPool,
    provider: &dyn EmbeddingProvider,
    query: &str,
    options: &RagSearchOptions,
) -> Result<Vec<RagChunkResult>> {
    let query_embedding = provider
        .embed_text(query)
        .await
        .context("Failed to embed RAG query")?;

    let raw = chunk_queries::find_similar_chunks(
        pool,
        query_embedding,
        options.top_k * 3,
        options.min_score,
    )
    .await
    .context("Failed to search chunks")?;

    let config = DiversifyConfig::default();
    let diversified = diversify_results(raw, &config);

    let result: Vec<RagChunkResult> = diversified
        .into_iter()
        .filter(|r| {
            if let Some(ref _lang) = options.language {
                true
            } else {
                true
            }
        })
        .take(options.top_k as usize)
        .collect();

    info!(
        query = query,
        result_count = result.len(),
        distinct_sources = result
            .iter()
            .filter_map(|r| r.source_name.clone())
            .collect::<HashSet<_>>()
            .len(),
        "RAG search completed"
    );

    Ok(result)
}
```

- [ ] **Step 4: Ajouter `pub mod rag_search;` dans `src/pipeline/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test --test rag_search_tests 2>&1`
Expected: 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: add RAG search service with source diversification"
```

---

## Task 5: API endpoint POST /internal/rag/search

**Files:**
- Create: `src/api/rag.rs`
- Modify: `src/api/mod.rs` (ajouter module + route)

- [ ] **Step 1: Creer rag.rs avec le handler**

```rust
// src/api/rag.rs
use axum::{
    Json,
    extract::State,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::AppState;
use crate::pipeline::rag_search::{self, RagSearchOptions};

#[derive(Deserialize)]
pub struct RagSearchRequest {
    pub query: String,
    pub top_k: Option<i32>,
    pub language: Option<String>,
    pub published_after: Option<String>,
    pub published_before: Option<String>,
    pub sources: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct RagSearchResponse {
    pub query: String,
    pub results: Vec<RagChunkEntry>,
}

#[derive(Serialize)]
pub struct RagChunkEntry {
    pub chunk_id: String,
    pub article_id: String,
    pub score: f64,
    pub chunk_text: String,
    pub title: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
}

pub async fn rag_search(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RagSearchRequest>,
) -> Result<Json<RagSearchResponse>, (StatusCode, String)> {
    let options = RagSearchOptions {
        top_k: body.top_k.unwrap_or(20),
        min_score: 0.70,
        language: body.language.clone(),
        published_after: body.published_after.and_then(|s| s.parse().ok()),
        published_before: body.published_before.and_then(|s| s.parse().ok()),
        sources: body.sources.clone(),
    };

    let results = rag_search::search_relevant_chunks(
        &state.pool,
        state.provider.as_ref(),
        &body.query,
        &options,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<RagChunkEntry> = results
        .into_iter()
        .map(|r| RagChunkEntry {
            chunk_id: r.chunk_id.to_string(),
            article_id: r.article_id.to_string(),
            score: r.score,
            chunk_text: r.chunk_text,
            title: r.title,
            source_name: r.source_name,
            url: r.url,
            published_at: r.published_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(RagSearchResponse {
        query: body.query,
        results: entries,
    }))
}
```

- [ ] **Step 2: Modifier src/api/mod.rs**

Ajouter `pub mod rag;` et ajouter la route :

```rust
.route(
    "/internal/rag/search",
    axum::routing::post(crate::api::rag::rag_search),
)
```

- [ ] **Step 3: Ajouter endpoint chunk embedding dans health.rs**

Ajouter dans `src/api/health.rs` un handler `create_article_chunks_embeddings` :

```rust
pub async fn create_article_chunks_embeddings(
    State(state): State<Arc<AppState>>,
    Path(article_id): Path<Uuid>,
) -> Result<Json<EmbedProfileResponse>, (StatusCode, String)> {
    let article = article_queries::get_article_by_id(&state.pool, article_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Article {} not found", article_id),
            )
        })?;

    let cleaned_text = match &article.cleaned_text {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Ok(Json(EmbedProfileResponse {
                status: "skipped".to_string(),
                article_id: article_id.to_string(),
                job_id: None,
            }));
        }
    };

    let word_count = cleaned_text.split_whitespace().count();
    if word_count < 500 {
        return Ok(Json(EmbedProfileResponse {
            status: "skipped_too_short".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let input_hash = compute_input_hash(cleaned_text);

    let is_dup = job_queries::is_already_embedded(
        &state.pool,
        "article_chunk",
        article_id,
        &input_hash,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_dup {
        return Ok(Json(EmbedProfileResponse {
            status: "already_embedded".to_string(),
            article_id: article_id.to_string(),
            job_id: None,
        }));
    }

    let job = job_queries::create_embedding_job(
        &state.pool,
        "article_chunk",
        article_id,
        state.provider.model_name(),
        &input_hash,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(EmbedProfileResponse {
        status: "job_created".to_string(),
        article_id: article_id.to_string(),
        job_id: Some(job.id.to_string()),
    }))
}
```

Et ajouter la route dans `src/api/mod.rs` :

```rust
.route(
    "/internal/embeddings/article-chunks/{article_id}",
    axum::routing::post(crate::api::health::create_article_chunks_embeddings),
)
```

- [ ] **Step 4: Verifier compilation**

Run: `cargo check 2>&1`

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add POST /internal/rag/search and chunk embedding endpoints"
```

---

## Task 6: Etendre le worker pour les chunks

**Files:**
- Modify: `src/workers/embedding_worker.rs`

**Principe :** Quand le worker recupere un job `article_chunk`, il decoupe l'article en chunks, embedde chaque chunk, et les stocke via `insert_chunks`.

- [ ] **Step 1: Ajouter le traitement des chunks dans process_job**

Dans `src/workers/embedding_worker.rs`, ajouter un match sur `job.target_type` :

```rust
match job.target_type.as_str() {
    "article_profile" => self.process_profile_job(job).await?,
    "article_chunk" => self.process_chunk_job(job).await?,
    _ => anyhow::bail!("Unknown job target_type: {}", job.target_type),
}
```

Deplacer la logique actuelle dans `process_profile_job` et creer `process_chunk_job` :

```rust
async fn process_chunk_job(
    &self,
    job: &crate::db::schema::EmbeddingJob,
) -> Result<()> {
    use crate::pipeline::chunker::{chunk_text, ChunkConfig};
    use crate::db::chunk_queries;

    let article = article_queries::get_article_by_id(&self.pool, job.target_id)
        .await
        .context("Failed to load article")?
        .context("Article not found")?;

    let cleaned_text = match &article.cleaned_text {
        Some(t) if !t.is_empty() => t,
        _ => {
            info!("Article has no cleaned_text, skipping chunk job");
            return Ok(());
        }
    };

    let config = ChunkConfig::default();
    let text_chunks = chunk_text(cleaned_text, &config);

    if text_chunks.is_empty() {
        return Ok(());
    }

    let texts: Vec<String> = text_chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = self.provider.embed_batch(&texts).await?;

    let chunks_data: Vec<(String, i32, Vec<f32>)> = text_chunks
        .iter()
        .zip(embeddings.iter())
        .map(|(chunk, embedding)| {
            (chunk.text.clone(), chunk.token_count as i32, embedding.clone())
        })
        .collect();

    let dimension = self.provider.dimension() as i32;
    let model_name = self.provider.model_name();

    chunk_queries::insert_chunks(
        &self.pool,
        article.id,
        model_name,
        dimension,
        &chunks_data,
    )
    .await
    .context("Failed to store chunk embeddings")?;

    info!(
        article_id = %article.id,
        chunks = chunks_data.len(),
        "Chunk embeddings stored"
    );

    Ok(())
}
```

- [ ] **Step 2: Verifier compilation**

Run: `cargo check 2>&1`

- [ ] **Step 3: Run tous les tests existants**

Run: `cargo test --lib 2>&1`
Expected: 17+ tests pass (aucune regression)

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: extend worker to process article_chunk jobs"
```

---

## Task 7: Tests de bout en bout + quality gates

**Files:**
- Create: `tests/api_rag_tests.rs`

- [ ] **Step 1: Ecrire les tests API RAG**

```rust
// tests/api_rag_tests.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use vox_rag::api::{self, AppState};
use vox_rag::embedding::MockEmbeddingProvider;
use vox_rag::pipeline::similarity::Thresholds;

fn make_app() -> axum::Router {
    let pool = PgPool::connect_lazy("postgres://unused").unwrap();
    let state = Arc::new(AppState {
        pool,
        provider: Arc::new(MockEmbeddingProvider::new(4096)),
        thresholds: Thresholds::default(),
    });
    api::create_router(state)
}

#[tokio::test]
async fn rag_search_endpoint_exists() {
    let app = make_app();
    let body = serde_json::json!({"query": "test query"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rag_search_requires_json_body() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/rag/search")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run les tests API RAG**

Run: `cargo test --test api_rag_tests 2>&1`

- [ ] **Step 3: Run quality gates completes**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib && cargo test --test chunker_tests && cargo test --test rag_search_tests && cargo test --test octen_provider_tests && cargo test --test api_tests && cargo test --test profile_tests && cargo test --test worker_test`

Expected: 0 erreurs, 0 warnings, tous les tests passent

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add RAG API tests and quality gates for Phase 2"
```

---

## Task 8: Mettre a jour le README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Ajouter dans le README**

Dans la section "API interne", ajouter :

```
### Recherche RAG

POST /internal/rag/search
Body: {"query": "...", "top_k": 20, "language": "fr", "published_after": "2026-06-01"}

Retourne des passages precis avec diversification par source :
- max 2 chunks par article
- max 3 articles par source
- minimum 3 sources distinctes si possible
```

Dans la section "Phases du projet", mettre a jour Phase 2 comme completee.

Ajouter les variables d'environnement nouvelles si besoin.

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "docs: update README with Phase 2 RAG features"
```

---

## Critere d'acceptation Phase 2

La Phase 2 est terminee quand :

1. Les articles peuvent etre decoupes en chunks (700-1200 tokens, overlap 150)
2. Chaque chunk a un embedding stocke dans `article_chunks`
3. `POST /internal/rag/search` retourne des passages pertinents avec score
4. Les resultats sont diversifies par source (pas 10 chunks du meme article)
5. La recherche peut etre filtree par date, langue et source
6. Le worker traite les jobs `article_chunk` en batch
7. Les tests du chunker passent (6+)
8. Les tests de diversification passent (4+)
9. Zero regression sur les tests Phase 1

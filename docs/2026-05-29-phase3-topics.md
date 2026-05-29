# Phase 3 Implementation Plan: Dossiers d'actualité / Topic Tracking

**Date:** 2026-05-29
**Project:** Vox_rag
**Location:** `/Users/nicolasgonthier/Travail/Voxpod/Vox_rag/`

---

## Overview

This plan implements Phase 3: automatic topic grouping, article assignment, markdown generation, and timeline tracking for news articles. The database schema (migrations 007-010) already exists. This plan covers the Rust code to make it operational.

**MVP order:** Topics (create/assign) → Markdown → Timeline → LLM (stub)

---

## Architecture Summary

```
Article arrives
    ↓
EmbeddingWorker (existing) → article_profile with embedding
    ↓
TopicJobWorker (new) picks up process_article_batch job
    ↓
TopicCandidateService: find top 10 similar topics by embedding
    ↓
TopicDecisionService: apply scoring rules, decide relation
    ↓
TopicMutationService: create new topic OR link to existing
    ↓
TopicMarkdownService: rebuild topic markdown (500-1500 words)
    ↓
TopicEmbeddingService: recompute topic embedding (skip if hash unchanged)
    ↓
TopicTimelineService: add/update timeline events
```

---

## Wave 1: Foundation (Schema + Queries + Config)

> **Dependencies:** None — can start immediately.
> **Parallelism:** Tasks 1.1-1.4 can run in parallel. Task 1.5 depends on 1.1.

---

### Task 1.1: Schema structs for topic tables

**Description:** Add `Topic`, `TopicArticle`, `TopicTimelineEvent`, and `TopicCandidateResult` structs to `src/db/schema.rs` matching the existing migrations (007-010).

**Files:**
- `src/db/schema.rs` — add structs

**Key fields to match migrations:**
- `Topic`: id, title, stable_slug, status (String), category, language, first_seen_at, last_seen_at, main_entities (Option<Value>), secondary_entities, keywords, short_summary, long_summary, topic_markdown, topic_markdown_hash, embedding_model, embedding_dimension, topic_embedding (Option<Vector>), embedded_at, importance_score (f64), confidence_score (f64), article_count (i32), source_count (i32), merged_into_topic_id, created_at, updated_at
- `TopicArticle`: id, topic_id, article_id, relation_type, confidence_score (f64), similarity_score (Option<f64>), decision_method, is_primary, is_timeline_source, added_at
- `TopicTimelineEvent`: id, topic_id, event_date, title, summary, importance, event_type, source_article_ids (Vec<Uuid>), confidence_score (f64), created_at, updated_at
- `TopicCandidateResult` (new query result): topic_id, score (f64), title, article_count

**Complexity:** S
**Agent:** glm-5.1

---

### Task 1.2: Topic queries (CRUD + similarity search)

**Description:** Create `src/db/topic_queries.rs` with basic topic operations and vector similarity search against `topic_embedding`.

**Functions needed:**
- `insert_topic(pool, &Topic) -> Result<Topic>`
- `get_topic_by_id(pool, Uuid) -> Result<Option<Topic>>`
- `get_topic_by_slug(pool, &str) -> Result<Option<Topic>>`
- `update_topic(pool, &Topic) -> Result<Topic>`
- `find_similar_topics(pool, embedding: Vec<f32>, top_k: i32, min_score: f64) -> Result<Vec<TopicCandidateResult>>` — uses `1 - (topic_embedding <=> $1)`
- `list_active_topics(pool, limit: i64) -> Result<Vec<Topic>>`
- `update_topic_status(pool, Uuid, &str) -> Result<()>`
- `merge_topic_into(pool, from_id: Uuid, into_id: Uuid) -> Result<()>`

**Conventions:**
- Use `sqlx::query_as::<_, T>()` with `.bind()`
- Use `pgvector::Vector::from(embedding)` for vector binding
- Use `#[instrument(skip(pool))]` on all functions
- Use `anyhow::Context` for error wrapping

**Files:**
- `src/db/topic_queries.rs` — create
- `src/db/mod.rs` — add `pub mod topic_queries;`

**Complexity:** M
**Agent:** mimo-v25-pro

---

### Task 1.3: Topic article queries

**Description:** Create `src/db/topic_article_queries.rs` for linking articles to topics.

**Functions needed:**
- `insert_topic_article(pool, topic_id, article_id, relation_type, confidence_score, similarity_score, decision_method, is_primary) -> Result<TopicArticle>`
- `get_topic_articles(pool, topic_id) -> Result<Vec<TopicArticle>>`
- `get_article_topics(pool, article_id) -> Result<Vec<TopicArticle>>`
- `update_topic_article_relation(pool, id, relation_type, confidence_score) -> Result<()>`
- `set_primary_article(pool, topic_id, article_id) -> Result<()>`
- `count_topic_articles(pool, topic_id) -> Result<i64>`
- `get_primary_article(pool, topic_id) -> Result<Option<TopicArticle>>`

**Files:**
- `src/db/topic_article_queries.rs` — create
- `src/db/mod.rs` — add `pub mod topic_article_queries;`

**Complexity:** M
**Agent:** mimo-v25-pro
**Dependencies:** Task 1.1

---

### Task 1.4: Topic timeline queries

**Description:** Create `src/db/timeline_queries.rs` for timeline event CRUD.

**Functions needed:**
- `insert_timeline_event(pool, topic_id, event_date, title, summary, importance, event_type, source_article_ids, confidence_score) -> Result<TopicTimelineEvent>`
- `get_timeline_events(pool, topic_id) -> Result<Vec<TopicTimelineEvent>>` — ordered by event_date DESC
- `update_timeline_event(pool, id, title, summary, importance, event_type, source_article_ids) -> Result<TopicTimelineEvent>`
- `delete_timeline_event(pool, id) -> Result<()>`
- `get_latest_timeline_event(pool, topic_id) -> Result<Option<TopicTimelineEvent>>`

**Files:**
- `src/db/timeline_queries.rs` — create
- `src/db/mod.rs` — add `pub mod timeline_queries;`

**Complexity:** M
**Agent:** mimo-v25-pro
**Dependencies:** Task 1.1

---

### Task 1.5: Topic config variables

**Description:** Add topic-specific configuration to `src/config.rs`.

**New fields on `Config`:**
```rust
pub topic_strong_match_threshold: f64,      // 0.84
pub topic_ambiguous_match_threshold: f64,   // 0.76
pub topic_weak_match_threshold: f64,        // 0.70
pub topic_reembed_on_markdown_change: bool, // true
pub topic_markdown_max_words: usize,        // 1500
pub topic_active_days: i32,                 // 3
pub topic_archive_days: i32,                // 14
```

**Environment variables:**
- `TOPIC_STRONG_MATCH_THRESHOLD`
- `TOPIC_AMBIGUOUS_MATCH_THRESHOLD`
- `TOPIC_WEAK_MATCH_THRESHOLD`
- `TOPIC_REEMBED_ON_MARKDOWN_CHANGE`
- `TOPIC_MARKDOWN_MAX_WORDS`
- `TOPIC_ACTIVE_DAYS`
- `TOPIC_ARCHIVE_DAYS`

**Files:**
- `src/config.rs` — add fields and env parsing

**Complexity:** S
**Agent:** glm-5.1
**Dependencies:** Task 1.1

---

## Wave 2: Core Services (Decision Pipeline)

> **Dependencies:** Wave 1 complete.
> **Parallelism:** Tasks 2.1-2.3 can run in parallel after Wave 1.

---

### Task 2.1: TopicCandidateService

**Description:** Service to find candidate topics for an article using its embedding. Returns top-N topics with similarity scores.

**Functions:**
- `find_candidates(pool, article_id, top_k: i32, min_score: f64) -> Result<Vec<TopicCandidate>>`
  - Fetch article profile embedding
  - Call `topic_queries::find_similar_topics()`
  - Enrich with topic metadata (article count, last seen)
  - Return scored candidates

**Types:**
```rust
pub struct TopicCandidate {
    pub topic_id: Uuid,
    pub score: f64,           // cosine similarity
    pub title: String,
    pub article_count: i32,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub status: String,
}
```

**Files:**
- `src/topic/candidate.rs` — create
- `src/topic/mod.rs` — create with `pub mod candidate;`

**Complexity:** M
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.2

---

### Task 2.2: TopicDecisionService

**Description:** Core decision engine. Given an article and candidate topics, decide the relation using the scoring rules from the PRD.

**Functions:**
- `decide_article_topic_relation(article, candidates, thresholds) -> Result<DecisionResult>`

**Decision logic (per PRD):**
```
For each candidate topic:
  If topic_score >= 0.92 → near_duplicate, link to same topic
  If topic_score >= 0.86 OR (topic_score >= 0.84) → same_story_update
  If topic_score 0.78-0.86 AND older article AND shared entities → background_context
  If medium score + partial entities + different angle → related_but_different

If NO topic_score >= 0.76 → DecisionResult::CreateNewTopic
If any topic_score 0.76-0.84 → DecisionResult::Ambiguous (candidates) → future LLM stub
```

**Types:**
```rust
pub enum DecisionResult {
    LinkToTopic { topic_id: Uuid, relation_type: String, confidence: f64 },
    CreateNewTopic,
    Ambiguous { candidates: Vec<TopicCandidate> }, // LLM stub for now
}

pub struct TopicThresholds {
    pub strong_match: f64,      // 0.84
    pub ambiguous_match: f64,   // 0.76
    pub weak_match: f64,        // 0.70
}
```

**Helper:** `shared_entities(article_entities: &Value, topic_entities: &Value) -> Vec<String>` — extract common entity names from JSONB arrays.

**Files:**
- `src/topic/decision.rs` — create
- `src/topic/mod.rs` — add `pub mod decision;`

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.5, 2.1

---

### Task 2.3: TopicMutationService

**Description:** Service to create, update, and merge topics. Handles all state changes.

**Functions:**
- `create_topic_from_article(pool, article, &ArticleProfile) -> Result<Topic>`
  - Generate title from article title (truncated)
  - Generate stable_slug (slugify title + short hash)
  - Set first_seen_at = article.published_at or now
  - Set last_seen_at = now
  - Copy entities/keywords from article profile
  - Set status = "new"
  - Insert via topic_queries

- `update_topic_on_new_article(pool, topic_id, article, relation_type) -> Result<Topic>`
  - Update last_seen_at = now
  - Increment article_count
  - Update source_count (distinct source_names)
  - If same_story_update → status = "active"
  - Update entities/keywords (merge, don't overwrite)

- `merge_topics(pool, from_topic_id, into_topic_id) -> Result<()>`
  - Update from_topic status = "merged", merged_into_topic_id = into_topic_id
  - Move all topic_articles to into_topic_id (ON CONFLICT UPDATE)
  - Update into_topic article_count and source_count
  - Rebuild markdown and embedding for into_topic

- `archive_inactive_topics(pool, archive_days: i32) -> Result<usize>`
  - Find topics where last_seen_at < now() - archive_days AND status = 'cooling_down'
  - Update status = 'archived'

- `update_topic_counts(pool, topic_id) -> Result<()>`
  - Recalculate article_count and source_count from topic_articles

**Files:**
- `src/topic/mutation.rs` — create
- `src/topic/mod.rs` — add `pub mod mutation;`

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.2, 1.3

---

## Wave 3: Advanced Services (Markdown + Embedding + Timeline)

> **Dependencies:** Wave 2 complete.
> **Parallelism:** Tasks 3.1-3.3 can run in parallel after Wave 2.

---

### Task 3.1: TopicMarkdownService

**Description:** Build a markdown summary (500-1500 words) for a topic from its linked articles. This becomes the text that gets embedded for the topic.

**Functions:**
- `build_topic_markdown(pool, topic_id, max_words: usize) -> Result<String>`

**Algorithm:**
1. Fetch primary article (or most recent if no primary)
2. Fetch all topic articles ordered by added_at DESC
3. Build sections:
   - `# {topic_title}`
   - `## Overview` — short_summary if exists, else auto-generated from primary article
   - `## Key Developments` — bullet points from article titles + short descriptions (most recent first, deduplicate similar titles)
   - `## Sources` — unique source names
   - `## Entities` — main_entities and secondary_entities
4. Truncate to max_words
5. Return markdown string

- `compute_markdown_hash(markdown: &str) -> String` — SHA-256 hash for deduplication

**Files:**
- `src/topic/markdown.rs` — create
- `src/topic/mod.rs` — add `pub mod markdown;`

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.3, 2.3

---

### Task 3.2: TopicEmbeddingService

**Description:** Compute and store topic embeddings. Skip if markdown hash hasn't changed.

**Functions:**
- `ensure_topic_embedding(pool, provider, topic_id, reembed_on_change: bool) -> Result<bool>`

**Algorithm:**
1. Fetch topic
2. If topic.topic_markdown is None or empty → build via TopicMarkdownService
3. Compute hash of markdown
4. If hash == topic.topic_markdown_hash AND topic.topic_embedding IS NOT NULL → skip, return false
5. Call provider.embed_text(&markdown)
6. Update topic: topic_embedding, topic_markdown_hash, embedded_at, embedding_model, embedding_dimension
7. Return true (was updated)

- `refresh_topic_embeddings_batch(pool, provider, limit: i32) -> Result<usize>`
  - Find topics where topic_embedding IS NULL OR (markdown changed and reembed_on_change=true)
  - Process up to `limit` topics
  - Return count processed

**Files:**
- `src/topic/embedding.rs` — create
- `src/topic/mod.rs` — add `pub mod embedding;`

**Complexity:** M
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.2, 3.1

---

### Task 3.3: TopicTimelineService

**Description:** Create and update timeline events for a topic as new articles are added.

**Functions:**
- `add_article_to_timeline(pool, topic_id, article, relation_type) -> Result<Option<TopicTimelineEvent>>`

**Logic:**
- If relation_type == "same_story_update" or "near_duplicate":
  - Check if an event exists for the same date (±1 day) with similar title (simhash or simple string similarity)
  - If yes → update existing event: append article_id to source_article_ids, update summary
  - If no → create new event:
    - event_date = article.published_at or now
    - title = article.title (or shortened)
    - summary = article.description or first 200 chars of cleaned_text
    - importance = "medium" (default, future: infer from article prominence)
    - event_type = "update" (or "start" if first article)
    - source_article_ids = [article.id]

- `rebuild_timeline(pool, topic_id) -> Result<Vec<TopicTimelineEvent>>`
  - Delete all existing timeline events for topic
  - Re-create from all topic_articles in chronological order
  - Return new events

- `infer_event_type(article, existing_events) -> String`
  - If no existing events → "start"
  - If article title contains escalation words → "escalation"
  - If article title contains correction words → "correction"
  - Default → "update"

**Files:**
- `src/topic/timeline.rs` — create
- `src/topic/mod.rs` — add `pub mod timeline;`

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 1.1, 1.3, 1.4, 2.3

---

## Wave 4: Worker + API

> **Dependencies:** Waves 1-3 complete.
> **Parallelism:** Tasks 4.1 and 4.2 can run in parallel after Wave 3.

---

### Task 4.1: TopicJobWorker

**Description:** Worker that processes topic-related embedding_jobs. Extends the existing job system with new target_types.

**New job types:**
- `process_article_batch` — process a batch of articles for topic assignment
- `refresh_topic_embedding_batch` — refresh topic embeddings
- `archive_inactive_topics` — archive old topics

**Implementation:**
- Add new match arms in `EmbeddingWorker::process_job()` (or create separate `TopicWorker`)
- Recommended: Create `src/workers/topic_worker.rs` as a separate worker for clarity

**TopicWorker::process_article_batch:**
1. Pick batch of articles that have embeddings but no topic assignment
2. For each article:
   - Call TopicCandidateService::find_candidates()
   - Call TopicDecisionService::decide_article_topic_relation()
   - If LinkToTopic → TopicMutationService::update_topic_on_new_article() + TopicArticleQueries::insert_topic_article()
   - If CreateNewTopic → TopicMutationService::create_topic_from_article() + link article
   - If Ambiguous → log, skip (LLM stub for future)
   - Call TopicMarkdownService::build_topic_markdown()
   - Call TopicEmbeddingService::ensure_topic_embedding()
   - Call TopicTimelineService::add_article_to_timeline()

**TopicWorker::refresh_topic_embedding_batch:**
1. Find topics needing embedding refresh
2. Call TopicEmbeddingService::refresh_topic_embeddings_batch()

**TopicWorker::archive_inactive_topics:**
1. Call TopicMutationService::archive_inactive_topics()

**Files:**
- `src/workers/topic_worker.rs` — create
- `src/workers/mod.rs` — add `pub mod topic_worker;`
- `src/workers/embedding_worker.rs` — may need minor changes if sharing job queue

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 2.1, 2.2, 2.3, 3.1, 3.2, 3.3

---

### Task 4.2: API Endpoints

**Description:** Add REST endpoints for topic operations under `/internal/`.

**Endpoints to implement:**

1. `POST /internal/jobs/process-article-batch`
   - Body: `{ "article_ids": [uuid], "force_reprocess": false }`
   - Creates embedding_jobs of type `process_article_batch`
   - Returns `{ "job_ids": [uuid], "count": N }`

2. `GET /internal/articles/:article_id/topic-candidates`
   - Query: `?top_k=10&min_score=0.70`
   - Calls TopicCandidateService
   - Returns `{ "article_id": "uuid", "candidates": [...] }`

3. `POST /internal/articles/:article_id/assign-topic`
   - Body: `{ "topic_id": "uuid", "relation_type": "same_story_update", "confidence": 0.88 }` OR `{ "create_new": true }`
   - Manually assign article to topic or trigger creation
   - Returns the TopicArticle link

4. `POST /internal/topics`
   - Body: `{ "title": "...", "article_ids": ["uuid"], "category": "..." }`
   - Manually create a topic
   - Returns created Topic

5. `GET /internal/topics/:topic_id`
   - Returns Topic with articles and timeline events

6. `POST /internal/topics/:topic_id/rebuild-markdown`
   - Force rebuild markdown and embedding
   - Returns `{ "markdown_hash": "...", "word_count": N, "embedded": true }`

7. `POST /internal/topics/:topic_id/reembed`
   - Force recompute embedding
   - Returns `{ "embedded": true, "dimension": 4096 }`

8. `POST /internal/topics/:topic_id/merge`
   - Body: `{ "target_topic_id": "uuid" }`
   - Calls TopicMutationService::merge_topics()
   - Returns success

**Files:**
- `src/api/topics.rs` — create (all topic endpoints)
- `src/api/mod.rs` — add `pub mod topics;` and register routes in `create_router()`
- `src/api/mod.rs` — add topic routes to Router

**Complexity:** L
**Agent:** mimo-v25-pro
**Dependencies:** Tasks 2.1, 2.2, 2.3, 3.1, 3.2, 3.3

---

## Wave 5: Integration & Testing

> **Dependencies:** Waves 1-4 complete.
> **Parallelism:** Tasks 5.1-5.3 can run in parallel after Wave 4.

---

### Task 5.1: Wire into main.rs and lib.rs

**Description:** Integrate topic services into the application lifecycle.

**Changes needed:**
- `src/lib.rs` — add `pub mod topic;`
- `src/main.rs` —
  - Parse topic config vars
  - Create TopicThresholds from config
  - Add `TopicWorker` to `All` command (spawn alongside EmbeddingWorker)
  - Pass topic thresholds to AppState
- `src/api/mod.rs` — add `topic_thresholds: TopicThresholds` to `AppState`

**Files:**
- `src/lib.rs`
- `src/main.rs`
- `src/api/mod.rs`

**Complexity:** M
**Agent:** glm-5.1
**Dependencies:** Tasks 4.1, 4.2

---

### Task 5.2: Unit tests for topic services

**Description:** Write tests for the pure logic functions (no DB needed).

**Test targets:**
- `topic::decision::classify_relation()` — score → relation_type mapping
- `topic::decision::shared_entities()` — JSONB intersection
- `topic::markdown::compute_markdown_hash()` — deterministic hashing
- `topic::markdown::truncate_to_words()` — word count truncation
- `topic::timeline::infer_event_type()` — event type inference
- `topic::candidate::score_candidates()` — scoring logic

**Conventions:**
- Use `#[test]` (not `#[tokio::test]`) where possible
- Mock data with `serde_json::json!()` for JSONB fields
- Test edge cases: empty entities, missing scores, boundary thresholds

**Files:**
- Tests inside each service file (same pattern as `pipeline/profile.rs` and `workers/embedding_worker.rs`)

**Complexity:** M
**Agent:** deepseek-v4-pro
**Dependencies:** Tasks 2.2, 3.1, 3.3

---

### Task 5.3: Integration tests for API endpoints

**Description:** Test the API endpoints using the existing test patterns.

**Test targets:**
- `POST /internal/topics` → create topic
- `GET /internal/topics/:id` → get topic
- `POST /internal/articles/:id/assign-topic` → assign article
- `GET /internal/articles/:id/topic-candidates` → get candidates

**Conventions:**
- Use `tokio::test` with in-memory DB or mock pool
- Follow existing test patterns from the codebase
- Use `MockEmbeddingProvider` (4096-dim) for embedding tests

**Files:**
- `src/api/topics.rs` — add `#[cfg(test)]` module

**Complexity:** M
**Agent:** deepseek-v4-pro
**Dependencies:** Tasks 4.2, 5.1

---

## Wave 6: Cleanup & Polish

> **Dependencies:** All prior waves complete.
> **Parallelism:** Tasks 6.1-6.3 sequential.

---

### Task 6.1: Add tracing and error handling review

**Description:** Ensure all topic service functions have proper `#[instrument]` attributes and `anyhow::Context` error wrapping. Add structured logging for key decisions.

**Files:** All `src/topic/*.rs` files

**Complexity:** S
**Agent:** glm-5.1

---

### Task 6.2: Documentation comments

**Description:** Add rustdoc comments to all public types and functions in topic modules.

**Files:** All `src/topic/*.rs`, `src/db/topic_queries.rs`, `src/db/topic_article_queries.rs`, `src/db/timeline_queries.rs`

**Complexity:** S
**Agent:** glm-5.1

---

### Task 6.3: Final verification

**Description:** Run full test suite and `cargo check` / `cargo clippy`.

```bash
cd /Users/nicolasgonthier/Travail/Voxpod/Vox_rag
cargo check
cargo clippy -- -D warnings
cargo test
```

**Files:** N/A

**Complexity:** S
**Agent:** deepseek-v4-pro

---

## File Inventory (New + Modified)

### New files (17):
```
src/db/topic_queries.rs
src/db/topic_article_queries.rs
src/db/timeline_queries.rs
src/topic/mod.rs
src/topic/candidate.rs
src/topic/decision.rs
src/topic/mutation.rs
src/topic/markdown.rs
src/topic/embedding.rs
src/topic/timeline.rs
src/workers/topic_worker.rs
src/api/topics.rs
```

### Modified files (7):
```
src/db/schema.rs          — add Topic, TopicArticle, TopicTimelineEvent, TopicCandidateResult
src/db/mod.rs             — add new query modules
src/config.rs             — add topic config vars
src/lib.rs                — add topic module
src/api/mod.rs            — add topics module, register routes, add thresholds to AppState
src/workers/mod.rs        — add topic_worker
src/workers/embedding_worker.rs — optionally extend for new job types
```

---

## Dependency Graph

```
Wave 1 (Foundation)
├── 1.1 Schema structs ──┬──→ 1.2 Topic queries
│                        ├──→ 1.3 Topic article queries
│                        ├──→ 1.4 Topic timeline queries
│                        └──→ 1.5 Config vars

Wave 2 (Core Services)
├── 2.1 TopicCandidateService ──┬──→ 2.2 TopicDecisionService
└── 2.3 TopicMutationService ────┘

Wave 3 (Advanced Services)
├── 3.1 TopicMarkdownService
├── 3.2 TopicEmbeddingService ──→ depends on 3.1
└── 3.3 TopicTimelineService ───→ depends on 2.3

Wave 4 (Worker + API)
├── 4.1 TopicJobWorker ─────────→ depends on all Wave 2 + 3
└── 4.2 API Endpoints ──────────→ depends on all Wave 2 + 3

Wave 5 (Integration)
├── 5.1 Wire into main.rs ──────→ depends on 4.1 + 4.2
├── 5.2 Unit tests ─────────────→ depends on 2.2 + 3.1 + 3.3
└── 5.3 Integration tests ──────→ depends on 4.2 + 5.1

Wave 6 (Cleanup)
├── 6.1 Tracing review
├── 6.2 Documentation
└── 6.3 Final verification
```

---

## Agent Assignment Summary

| Agent | Tasks | Rationale |
|-------|-------|-----------|
| **glm-5.1** | 1.1, 1.5, 5.1, 6.1, 6.2 | Scaffolding, config, wiring, docs, polish. Straightforward structural work. |
| **mimo-v25-pro** | 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 3.1, 3.2, 3.3, 4.1, 4.2 | Complex logic, SQL queries, service orchestration, API handlers. Core business logic. |
| **deepseek-v4-pro** | 5.2, 5.3, 6.3 | Tests, verification, edge cases. Quality assurance. |

---

## Notes for Implementing Agents

### SQL Query Patterns
- Always use `sqlx::query_as::<_, T>(r#"..."#).bind(val)` — NOT the macro version
- Vector binding: `let vector = Vector::from(embedding);` then `.bind(&vector)`
- JSONB binding: `serde_json::to_value(entities)?` then `.bind(json_value)`
- UUID arrays: `.bind(&source_article_ids)` where `Vec<Uuid>`

### Transaction Safety
- Topic creation + article linking should be in a transaction
- Use `pool.begin().await?` and `tx.commit().await?`
- Topic merge must be atomic

### Slug Generation
- Use a simple slugify: lowercase, replace spaces with `-`, strip non-alphanumeric
- Append short hash (first 6 chars of UUID) to ensure uniqueness
- Example: `"trump-tariffs-eu-3a7f2b"`

### LLM Stub
- The `Ambiguous` decision result currently just logs and skips
- Leave a clear TODO comment: `// TODO(Phase 3.5): Call LLM for ambiguous cases`
- The stub should return the candidates so the API can expose them

### Config Defaults
```
TOPIC_STRONG_MATCH_THRESHOLD=0.84
TOPIC_AMBIGUOUS_MATCH_THRESHOLD=0.76
TOPIC_WEAK_MATCH_THRESHOLD=0.70
TOPIC_REEMBED_ON_MARKDOWN_CHANGE=true
TOPIC_MARKDOWN_MAX_WORDS=1500
TOPIC_ACTIVE_DAYS=3
TOPIC_ARCHIVE_DAYS=14
```

### Worker Coexistence
- The existing `EmbeddingWorker` handles `article_profile` and `article_chunk` jobs
- The new `TopicWorker` handles `process_article_batch`, `refresh_topic_embedding_batch`, `archive_inactive_topics`
- Both can run in the same process (see `Commands::All` in main.rs) or separately
- They share the same `embedding_jobs` table but filter by `target_type`

### Performance Notes
- `find_similar_topics` uses HNSW index on `topic_embedding` (migration 010)
- `find_similar_articles` already has index on `article_profiles.embedding`
- Batch article processing in TopicWorker to avoid N+1 queries
- Rebuild markdown only when article count changes (not on every article add)

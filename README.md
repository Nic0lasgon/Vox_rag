# Vox_rag — Module Embedding + RAG pour VoxPod

Module d'embedding et de recherche sémantique pour rapprocher des articles d'actualité entre eux. Premier maillon de la chaîne VoxPod après l'`ingest-pipeline`.

## Vue d'ensemble

Ce projet prend les articles qualifiés par l'`ingest-pipeline`, génère des embeddings vectoriels via l'API Octen, regroupe automatiquement les articles en dossiers d'actualité, et permet de retrouver les articles et passages pertinents.

**Cas d'usage principal** : "Quand un nouvel article arrive, à quelle histoire en cours appartient-il, et quelle nouvelle étape ajoute-t-il ?"

**Stack technique** :
- **Runtime** : Rust + Tokio
- **Web** : Axum
- **Database** : PostgreSQL 16 + pgvector
- **Embedding** : Octen API (modèle `octen-embedding-8b`, dimension 4096)
- **Déploiement** : Docker Compose (local) → VPS/Railway (prod)

## Architecture

### Modes de lancement

```bash
cargo run -- api          # Serveur HTTP Axum uniquement
cargo run -- worker       # Worker d'embedding uniquement
cargo run -- topic-worker # Worker de topics uniquement
cargo run -- all          # API + Embedding Worker + Topic Worker (dev local)
cargo run -- test-embed --text "Un texte à embedder"  # Test rapide avec mock
```

### Flux de données

```
Article qualifié (depuis ingest-pipeline)
  ↓
Nettoyage texte + construction du profile_text
  ↓
Appel Octen API → embedding vectoriel (4096 dimensions)
  ↓
Stockage dans article_profiles (PostgreSQL + pgvector)
  ↓
Recherche similarité cosinus entre articles
  ↓
Articles similaires retournés avec score + relation_hint

  ┌─ En parallèle ─────────────────────────────────┐
  │ Découpage en chunks (700-1200 tokens, overlap 150)│
  │   ↓                                               │
  │ Embedding de chaque chunk via Octen               │
  │   ↓                                               │
  │ Stockage dans article_chunks (pgvector)           │
  │   ↓                                               │
  │ Recherche RAG : query → chunks pertinents         │
  │   ↓                                               │
  │ Diversification par source (max 2/article, 3/source)│
  └──────────────────────────────────────────────────┘

  ┌─ Dossiers d'actualité (Phase 3) ─────────────────┐
  │ Article → embedding → recherche topics similaires  │
  │   ↓                                                │
  │ Décision : lier à un topic existant OU créer nouveau│
  │   ↓                                                │
  │ Mise à jour topic (résumé, timeline, counts)       │
  │   ↓                                                │
  │ Génération topic_markdown (500-1500 mots)          │
  │   ↓                                                │
  │ Embedding du topic (skip si hash inchangé)         │
  └────────────────────────────────────────────────────┘
```

### Structure du projet

```
Vox_rag/
├── src/
│   ├── main.rs                  # Entry point + CLI (clap)
│   ├── config.rs                # Configuration par variables d'environnement
│   ├── lib.rs                   # Exports du crate
│   ├── api/                     # HTTP API (Axum)
│   │   ├── mod.rs               # Router + AppState
│   │   ├── health.rs            # Endpoints /health + /internal/*
│   │   ├── rag.rs               # Endpoint POST /internal/rag/search
│   │   └── topics.rs            # Endpoints CRUD topics + merge + batch
│   ├── embedding/               # Fournisseurs d'embeddings
│   │   ├── provider.rs          # Trait EmbeddingProvider
│   │   ├── octen.rs             # Client Octen API (x-api-key auth)
│   │   └── mock.rs              # Provider mock pour les tests
│   ├── llm/                     # Fournisseur LLM pour l'arbitrage
│   │   ├── provider.rs          # Trait LlmProvider + LlmDecisionResponse
│   │   ├── deepseek.rs          # Client DeepSeek complet (chat, JSON, tools, thinking, FIM, prefix, retry)
│   │   ├── mock.rs              # Provider mock LLM pour les tests
│   │   └── prompt.rs            # Construction du prompt de décision
│   ├── pipeline/                # Logique métier
│   │   ├── profile.rs           # Construction du texte de profil article
│   │   ├── chunker.rs           # Découpage texte en chunks (700-1200 tokens)
│   │   ├── similarity.rs        # Recherche similarité + classification
│   │   └── rag_search.rs        # Recherche RAG avec diversification
│   ├── topic/                   # Services de gestion des dossiers d'actualité
│   │   ├── candidate.rs         # Recherche de topics candidats
│   │   ├── decision.rs          # Logique de décision article → topic
│   │   ├── mutation.rs          # Création / mise à jour / archivage topics
│   │   ├── markdown.rs          # Génération du topic_markdown
│   │   ├── embedding.rs         # Embedding des topics (skip si inchangé)
│   │   └── timeline.rs          # Gestion des événements de timeline
│   ├── db/                      # Couche d'accès aux données
│   │   ├── schema.rs            # Types Rust (Article, ArticleProfile, ArticleChunk, EmbeddingJob)
│   │   ├── article_queries.rs   # CRUD articles
│   │   ├── embedding_queries.rs # Upsert embeddings + recherche pgvector
│   │   ├── chunk_queries.rs     # CRUD chunks + recherche pgvector RAG
│   │   ├── topic_queries.rs     # CRUD topics + recherche pgvector + merge
│   │   └── job_queries.rs       # File de jobs (SKIP LOCKED)
│   └── workers/
│       ├── embedding_worker.rs  # Worker : poll jobs → embed → stocker
│       └── topic_worker.rs      # Worker : assignation articles → topics
├── migrations/                  # Migrations PostgreSQL
│   ├── 001_enable_pgvector.sql
│   ├── 002_articles.sql
│   ├── 003_article_profiles.sql
│   ├── 004_embedding_jobs.sql
│   ├── 005_vector_indexes.sql
│   ├── 006_article_chunks.sql
│   ├── 007_topics.sql
│   ├── 008_topic_articles.sql
│   ├── 009_topic_timeline_events.sql
│   └── 010_topic_indexes.sql
├── Cargo.toml
├── docker-compose.yml
├── Dockerfile
├── .env.example
├── scripts/
│   ├── import_from_ingest.sh   # Import articles depuis ingest-pipeline
│   ├── import_from_ingest.sql  # SQL version (alternative dblink)
│   └── test_with_real_articles.sh # Test complet automatisé
└── docs/
    ├── deepseek-api-reference.md
    ├── benchmark-real-tests.md
    ├── 2026-05-29-phase2-chunks-rag.md
    └── 2026-05-29-phase3-topics.md
```

## Démarrage rapide

### Prérequis

- Rust 1.88+
- PostgreSQL 16+ avec l'extension pgvector (ou Docker)
- Clé API Octen

### Avec Docker Compose (recommandé)

```bash
# 1. Copier la config
cp .env.example .env

# 2. Ajouter la clé API Octen dans .env
# OCTEN_API_KEY=octen-votre-cle-ici

# 3. Lancer le stack complet
docker compose up -d

# 4. Vérifier le health check
curl http://localhost:3001/health
# → {"status":"ok"}

# 5. Voir les logs
docker compose logs -f backend

# 6. Arrêter
docker compose down -v
```

### Sans Docker (dev local)

```bash
# 1. PostgreSQL avec pgvector doit tourner
# (ou utiliser l'image pgvector/pgvector:pg16)

# 2. Configurer les variables d'environnement
export DATABASE_URL="postgres://voxrag:voxrag_dev@localhost:5433/vox_rag"
export OCTEN_API_KEY="octen-votre-cle-ici"

# 3. Lancer (les migrations s'appliquent automatiquement au démarrage)
cargo run -- all
```

### Lancer les tests

```bash
# Tests unitaires (pas de DB nécessaire)
cargo test --lib

# Quality gates
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib
```

### Tester avec de vrais articles (depuis ingest-pipeline)

L'`ingest-pipeline` ingère des flux RSS et qualifie les articles. Vox_rag peut ensuite les embedder et les regrouper en topics.

**Flux recommandé** :
```
RSS → ingest-pipeline (fetch, extract, dedup, qualify)
  → import dans Vox_rag
  → embeddings Octen
  → topic-worker
  → audit qualité
```

**Scripts disponibles** :
- `scripts/import_from_ingest.sh` — Importe les articles qualifiés depuis ingest-pipeline
- `scripts/test_with_real_articles.sh` — Test complet automatisé

**Extraction de contenu** : Les descriptions RSS font 30-60 mots (en dessous du seuil de 80). Pour les sites JavaScript-rendered (Euronews, RFI), utiliser le serveur Scrapling Hetzner :

```bash
curl -X POST "https://search.myswearpod.de/extract" \
  -H "Authorization: Bearer <HETZNER_EXTRACT_SECRET>" \
  -H "Content-Type: application/json" \
  -d '{"url": "...", "stealth": true, "format": "txt"}'
```

**Constats des tests réels** (9 articles, 4 sources) :
- Scrapling a permis de récupérer 2 articles RFI qui échouaient en HTTP direct
- Les scores de similarité sont plus bas quand le texte contient du bruit (cookies, navigation)
- Le LLM DeepSeek n'a pas été déclenché (aucun cas ambigus dans le test)

## Composants clés

### 1. Embedding Providers (`src/embedding/`)

Le système utilise un trait abstrait `EmbeddingProvider` pour découpler la logique métier du fournisseur d'embeddings :

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimension(&self) -> usize;
    async fn embed_text(&self, input: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>>;
}
```

**Implementations** :
- `OctenEmbeddingProvider` : appelle l'API Octen (`POST /embedding`, auth `x-api-key`)
- `MockEmbeddingProvider` : embeddings déterministes pour les tests (pas de réseau)

**Modèle utilisé** : `octen-embedding-8b` ($0.07 / 1M tokens, dimension 4096)

### 2. Article Profile (`src/pipeline/profile.rs`)

Chaque article est transformé en un texte condensé avant embedding :

```
Title: {title}
Source: {source_name}
Published at: {published_at}
Language: {language}
Description: {description}
Article lead:
{premiers 1500 caractères du texte nettoyé}
```

**Guards** : l'embedding est refusé si l'article a moins de 80 mots utiles ou pas de texte nettoyé.

**Normalisation** avant embedding : suppression HTML, normalisation des guillemets typographiques, collapse des espaces.

### 3. Recherche de similarité (`src/pipeline/similarity.rs`)

Utilise la distance cosinus de pgvector (`<=>` operator) pour trouver les articles proches.

**Seuils configurables** (via env vars) :

| Score | Classification | Signification |
|-------|---------------|---------------|
| >= 0.92 | `near_duplicate` | Quasi-doublon ou reprise très proche |
| 0.86 — 0.92 | `same_story` | Même événement / même actualité |
| 0.78 — 0.86 | `context` | Sujet lié ou contexte potentiel |
| 0.70 — 0.78 | `weak` | Lien faible |
| < 0.70 | `unrelated` | Ignoré |

**Fonctions** :
- `find_similar_articles(article_id, top_k)` : articles les plus proches (toutes dates)
- `find_historical_context(article_id, lookback_days, top_k)` : articles plus anciens apportant du contexte

### 4. Job Queue (`src/db/job_queries.rs`)

File de jobs PostgreSQL pour traiter les embeddings de manière asynchrone.

**Mécanisme** :
1. L'API crée un job `pending` quand on demande un embedding
2. Le worker utilise `SELECT ... FOR UPDATE SKIP LOCKED` pour prendre des jobs sans conflit
3. En cas d'échec : retry avec backoff exponentiel (2^n secondes), marqué `failed` après 3 tentatives
4. Déduplication via `input_hash` (SHA-256 du texte) : ne ré-embedde pas si le texte n'a pas changé

**Statuts** : `pending` → `running` → `done` / `failed`

### 5. Stockage vectoriel (`migrations/`)

**Table `article_profiles`** :
- `article_id` (clé étrangère vers articles)
- `profile_text` : texte envoyé à Octen
- `embedding VECTOR(4096)` : vecteur stocké via pgvector
- `embedding_model`, `embedding_dimension` : traçabilité du modèle utilisé
- `embedded_at` : timestamp de l'embedding

**Index vectoriel** : `HNSW` avec `vector_cosine_ops` pour la recherche rapide en similarité cosinus.

**Table `embedding_jobs`** : file de traitement avec hash de déduplication, tentatives, et erreurs.

## API interne

### Health check

```
GET /health
→ {"status": "ok"}
```

### Embedding d'un article

```
POST /internal/embeddings/article-profile/:articleId
```

Crée un job d'embedding pour l'article. Réponses possibles :
- `{"status": "job_created", "job_id": "..."}` : job créé, le worker le traitera
- `{"status": "already_embedded"}` : déjà embeddé avec le même texte
- `{"status": "skipped"}` : article trop court ou vide

### Articles similaires

```
GET /internal/articles/:articleId/similar?top_k=30
```

Retourne les articles les plus proches en similarité cosinus :

```json
{
  "article_id": "...",
  "candidates": [
    {
      "article_id": "...",
      "score": 0.91,
      "relation_hint": "near_duplicate",
      "title": "...",
      "source_name": "...",
      "url": "...",
      "published_at": "..."
    }
  ]
}
```

### Contexte historique

```
GET /internal/articles/:articleId/historical-context?lookback_days=30&top_k=20
```

Retourne les articles plus anciens pouvant apporter du contexte.

### Recherche RAG

```
POST /internal/rag/search
```

Body :
```json
{
  "query": "Pourquoi le cessez-le-feu entre l'Iran et Israël est important ?",
  "top_k": 20,
  "language": "fr",
  "published_after": "2026-06-01",
  "sources": ["Reuters", "AFP"]
}
```

Réponse :
```json
{
  "query": "...",
  "results": [
    {
      "chunk_id": "...",
      "article_id": "...",
      "score": 0.84,
      "chunk_text": "Passage précis de l'article...",
      "title": "...",
      "source_name": "Reuters",
      "url": "...",
      "published_at": "..."
    }
  ]
}
```

**Diversification automatique** : les résultats sont diversifiés par source pour éviter de retourner 10 chunks du même article :
- Max 2 chunks par article
- Max 3 articles par source
- Minimum 3 sources distinctes si possible

### Embedding des chunks d'un article

```
POST /internal/embeddings/article-chunks/:articleId
```

Crée un job de chunking + embedding pour l'article. L'article doit avoir au moins 500 mots de texte nettoyé. Réponses possibles : `job_created`, `already_embedded`, `skipped_too_short`, `skipped`.

### Dossiers d'actualité (Topics)

**Créer un topic manuellement** :
```
POST /internal/topics
```
Body : `{"title": "...", "category": "...", "language": "fr", "article_ids": ["uuid"]}`

**Lire un topic complet** :
```
GET /internal/topics/:topicId
```
Retourne le topic, ses articles liés, sa timeline et son markdown.

**Reconstruire le markdown** :
```
POST /internal/topics/:topicId/rebuild-markdown
```

**Recalculer l'embedding** :
```
POST /internal/topics/:topicId/reembed
```

**Fusionner deux topics** :
```
POST /internal/topics/:topicId/merge
```
Body : `{"target_topic_id": "uuid"}`

**Voir les topics candidats d'un article** :
```
GET /internal/articles/:articleId/topic-candidates
```

**Assigner un article à un topic** :
```
POST /internal/articles/:articleId/assign-topic
```
Body : `{"topic_id": "uuid", "relation_type": "same_story_update"}` ou `{"create_new": true}`

**Lancer un batch de traitement** :
```
POST /internal/jobs/process-article-batch
```
Body : `{"article_ids": ["uuid1", "uuid2"]}`

## Variables d'environnement

| Variable | Obligatoire | Défaut | Description |
|----------|-------------|--------|-------------|
| `DATABASE_URL` | Oui | — | URL PostgreSQL |
| `OCTEN_API_KEY` | Oui | — | Clé API Octen (format: `octen-...`) |
| `PORT` | Non | 3000 | Port API HTTP |
| `LOG_LEVEL` | Non | info | Niveau de log |
| `EMBEDDING_PROVIDER` | Non | octen | Fournisseur (`octen` ou `mock`) |
| `EMBEDDING_MODEL` | Non | octen-embedding-8b | Modèle d'embedding |
| `EMBEDDING_DIMENSION` | Non | 4096 | Dimension des vecteurs |
| `EMBEDDING_BATCH_SIZE` | Non | 32 | Taille de batch |
| `EMBEDDING_MAX_RETRIES` | Non | 3 | Tentatives max par job |
| `EMBEDDING_TIMEOUT_MS` | Non | 60000 | Timeout API (ms) |
| `OCTEN_API_BASE_URL` | Non | https://api.octen.ai | URL de base Octen |
| `SIMILARITY_NEAR_DUPLICATE_THRESHOLD` | Non | 0.92 | Seuil quasi-doublon |
| `SIMILARITY_SAME_STORY_THRESHOLD` | Non | 0.86 | Seuil même sujet |
| `SIMILARITY_CONTEXT_THRESHOLD` | Non | 0.78 | Seuil contexte |
| `SIMILARITY_WEAK_THRESHOLD` | Non | 0.70 | Seuil lien faible |
| `TOPIC_STRONG_MATCH_THRESHOLD` | Non | 0.84 | Seuil topic fort |
| `TOPIC_AMBIGUOUS_MATCH_THRESHOLD` | Non | 0.76 | Seuil topic ambigu |
| `TOPIC_WEAK_MATCH_THRESHOLD` | Non | 0.70 | Seuil topic faible |
| `TOPIC_MARKDOWN_MAX_WORDS` | Non | 1500 | Max mots topic_markdown |
| `TOPIC_ACTIVE_DAYS` | Non | 3 | Jours avant passage cooling_down |
| `TOPIC_ARCHIVE_DAYS` | Non | 14 | Jours avant archivage |
| `USE_LLM_FOR_TOPICS` | Non | false | Activer l'arbitrage LLM (voir .env.example pour exemple) |
| `LLM_API_KEY` | Non | — | Clé API DeepSeek |
| `LLM_MODEL` | Non | deepseek-v4-flash | Modèle LLM |
| `LLM_BASE_URL` | Non | https://api.deepseek.com | URL API LLM (compatible OpenAI) |
| `LLM_TIMEOUT_MS` | Non | 30000 | Timeout LLM (ms) |

## Coûts estimés

### Embedding Octen

Prix Octen 8B : **$0.07 / 1M tokens**

| Volume | Coût estimé (article profile uniquement) |
|--------|------------------------------------------|
| 100 000 articles | ~$7 |
| 1 000 000 articles | ~$70 |

La stratégie MVP : embedder systématiquement le `article_profile`, et les chunks RAG uniquement pour les articles retenus (Phase 2).

### LLM DeepSeek (arbitrage)

Prix DeepSeek V4 Flash : **$0.14/1M input**, **$0.28/1M output**

Coût par décision LLM (thinking désactivé) : **~$0.00007**

| Volume (décisions LLM) | Coût estimé |
|------------------------|-------------|
| 10 000 décisions | ~$0.70 |
| 100 000 décisions | ~$7 |

Les appels LLM ne se déclenchent que pour les cas ambigus (score 0.76-0.84), pas pour chaque article.

## Phases du projet

### Phase 1 — Fondations (terminée)

- Embedding `article_profile` via Octen
- Stockage PostgreSQL + pgvector
- Recherche similarité cosinus article ↔ article
- API interne (embedding, similarité, contexte historique)
- Worker asynchrone avec job queue
- Tests unitaires

### Phase 2 — Chunks RAG (terminée)

- Découpage des articles en chunks (700-1200 tokens, overlap 150)
- Embedding des chunks via Octen (batch)
- Endpoint `POST /internal/rag/search` avec diversification par source
- Endpoint `POST /internal/embeddings/article-chunks/:articleId`
- Worker étendu pour traiter les jobs `article_chunk`
- Table `article_chunks` avec index HNSW

### Phase 3 — Dossiers d'actualité (terminée)

- Tables `topics`, `topic_articles`, `topic_timeline_events` avec index HNSW
- Regroupement automatique : article → recherche topics similaires → décision
- Décision par seuils : near_duplicate, same_story_update, background_context, related_but_different, ambiguous
- Création automatique de nouveaux topics si aucun candidat solide
- Génération de `topic_markdown` (500-1500 mots) à partir des articles liés
- Embedding des topics via Octen (skip si hash SHA-256 inchangé)
- Timeline automatique avec inférence du type d'événement (escalade, désescalade, annonce...)
- Fusion de topics via API
- 8 endpoints API : CRUD topics, merge, reembed, candidates, assign, batch
- Worker `topic-worker` séparé de l'embedding worker

### Phase 4 — LLM d'arbitrage (terminée)

  - Client LLM complet via DeepSeek (`deepseek-v4-flash`, compatible OpenAI)
  - Chat completion standard + JSON Output (`response_format: json_object`)
  - Tool Calls, Thinking Mode, FIM, Chat Prefix (capacités disponibles, non utilisées par défaut)
  - Thinking désactivé pour l'arbitrage (économie de ~65% de tokens, inutile pour les décisions JSON)
  - Retry automatique avec backoff exponentiel sur erreurs 429/500/503
  - Logging des coûts par appel (tokens, cache hit/miss, USD)
  - Context caching supporté (DeepSeek cache les préfixes de messages) — non exploité car le prompt user change à chaque appel
- Arbitrage LLM pour les cas ambigus (score 0.76-0.84) : le LLM décide si l'article appartient à un topic existant ou si un nouveau topic doit être créé
- Prompt structuré en français avec article, candidats, timeline
- Réponse JSON parsée automatiquement (decision, confidence, reason)
- Fallback conservateur si LLM indisponible : création d'un nouveau topic
- Provider mock pour les tests
- Activation par `USE_LLM_FOR_TOPICS=true`

### Phase 5 — Fusion et archivage avancés (à venir)

## Conventions de code

- `anyhow::Result` pour la gestion d'erreurs
- `sqlx::query_as::<_, T>()` avec `.bind()` (pas de macro compile-time)
- `tracing` pour les logs structurés
- Pas de `unwrap()` hors des tests
- `#[derive(sqlx::FromRow)]` pour les types DB
- `async_trait` pour les traits asynchrones

## Points d'attention

### Idempotence
- Les embeddings ne sont pas recalculés si le `input_hash` (SHA-256) n'a pas changé
- L'`upsert` sur `article_profiles` utilise `ON CONFLICT (article_id) DO UPDATE`
- Le worker vérifie `is_already_embedded` avant d'appeler Octen

### Performance
- Le worker utilise `SKIP LOCKED` pour ne jamais bloquer la table
- L'index HNSW sur `embedding` permet la recherche en temps constant
- Le client HTTP est réutilisé (connection pooling + HTTP/2)
- Les regex sont compilées une seule fois

### Erreurs
- API Octen indisponible → retry avec backoff, ne bloque pas l'ingestion
- Dimension incorrecte → insertion refusée, erreur critique loguée
- Article vide → statut `skipped`, pas d'embedding

## Déploiement

### Local (Docker Compose)

```bash
docker compose up -d
```

### Production

```bash
docker build -t vox-rag .
# Configurer les variables d'environnement
# Lancer avec : vox-rag api / vox-rag worker / vox-rag all
```

**Important production** :
- Utiliser l'image `pgvector/pgvector:pg16` pour PostgreSQL
- Le worker peut tourner sur plusieurs instances (grâce au `SKIP LOCKED`)
- Surveiller les jobs `failed` (indiquent des échecs répétés d'API)
- Les migrations s'appliquent automatiquement au démarrage

## Liens

- **PRD Embedding + RAG** : `Documentation/Step Rag/PRD - Système Embedding + RAG.pdf`
- **PRD Dossiers d'actualité** : `Documentation/Step Rag/PRD - Système de dossiers d'actualité vivants pour VoxPod.pdf`
- **Ingest Pipeline** (module amont) : `../ingest-pipeline/`
- **Octen API docs** : https://docs.octen.ai/api-reference/embedding
- **DeepSeek API docs** : https://api-docs.deepseek.com
- **Référence DeepSeek** : `docs/deepseek-api-reference.md`
- **Benchmarks réels** : `docs/benchmark-real-tests.md`
- **Plan Phase 3** : `docs/2026-05-29-phase3-topics.md`
- **Scripts d'import** : `scripts/`

## Changelog

### v0.4.0 (Phase 4 — LLM d'arbitrage)

**Fonctionnalités** :
- Client LLM via DeepSeek (`src/llm/deepseek.rs`)
- Trait `LlmProvider` avec implémentation mock pour les tests
- Arbitrage LLM pour les cas ambigus (score 0.76-0.84) : décision en JSON
- Prompt structuré en français avec article, candidats, timeline du dossier
- Réponse parsée : decision, confidence, should_update_summary, should_create_timeline_event, reason
- Intégration dans TopicWorker : remplace le fallback "create new topic" par un appel LLM
- Fallback conservateur si LLM indisponible ou non configuré
- Activation via `USE_LLM_FOR_TOPICS=true`
- Client DeepSeek complet avec retry, logging coûts, cache optimization
- Refactoré en `DeepSeekClient` (réutilisable) + `DeepSeekProvider` (implémente LlmProvider)
- Capacités : chat, JSON, tool calls, thinking mode, FIM, chat prefix
- Retry automatique avec backoff exponentiel sur erreurs 429/500/503
- Référence API sauvegardée dans `docs/deepseek-api-reference.md`

**Tests** : 147 tests (53 lib + 2 chunker + 5 rag_search + 9 api + 17 api_rag + 14 api_chunk + 14 api_topic + 18 octen + 9 profile + 6 worker)

### v0.3.0 (Phase 3 — Dossiers d'actualité)

**Fonctionnalités** :
- Tables `topics`, `topic_articles`, `topic_timeline_events` (migrations 007-010)
- TopicCandidateService : recherche de topics similaires via pgvector
- TopicDecisionService : logique de décision par seuils (0.92 → near_duplicate, 0.84 → same_story_update, 0.78 → background_context, 0.76 → ambiguous)
- TopicMutationService : création, mise à jour, archivage de topics
- TopicMarkdownService : génération de topic_markdown condensé (500-1500 mots)
- TopicEmbeddingService : embedding des topics avec déduplication par hash SHA-256
- TopicTimelineService : timeline automatique avec inférence du type d'événement
- 8 endpoints API : CRUD topics, merge, reembed, candidates, assign, batch
- TopicWorker séparé (process_article_batch, refresh_topic_embedding, archive_inactive_topics)
- CLI : `cargo run -- topic-worker` + `cargo run -- all` lance les 3 workers

**Tests** : 138 tests (44 lib + 17 api_topic + 9 api + 5 api_rag + 2 api_chunk + 14 chunker + 9 rag_search + 14 octen + 18 profile + 6 worker + 6 timeline)

### v0.2.0 (Phase 2 — Chunks RAG)

**Fonctionnalités** :
- Chunker : découpage des articles en passages de 700-1200 tokens (overlap 150)
- Table `article_chunks` avec index HNSW pgvector
- Embedding des chunks via Octen API (batch)
- Recherche RAG sémantique avec diversification par source
- Endpoint `POST /internal/rag/search` (top_k, filtres langue/date/sources)
- Endpoint `POST /internal/embeddings/article-chunks/:articleId`
- Worker étendu : traite les jobs `article_profile` et `article_chunk`
- Guard : articles < 500 mots ignorés pour le chunking

**Tests** : 56 tests (17 lib + 14 chunker + 9 rag_search + 9 api + 5 api_rag + 2 api_chunk)

### v0.1.0 (Phase 1 — Fondations)

**Fonctionnalités** :
- Embedding `article_profile` via Octen API (`octen-embedding-8b`, dimension 4096)
- Stockage vectoriel PostgreSQL + pgvector avec index HNSW
- Recherche similarité cosinus article ↔ article
- Recherche de contexte historique
- API interne : health, embedding, similarité, historique
- Worker asynchrone avec job queue PostgreSQL (SKIP LOCKED)
- Déduplication par hash SHA-256 du texte
- Provider mock pour les tests (pas de réseau)
- Configuration complète par variables d'environnement
- Docker Compose avec pgvector

**Tests** : 56 tests passent (embedding mock, profile builder, classification similarité, chunker, RAG search, API endpoints)

**Quality gates** : `cargo fmt --check && cargo clippy -- -D warnings && cargo test` → 0 erreurs, 0 warnings

---

**Dernière mise à jour** : 2026-05-29
**Version** : v0.4.0
**Statut** : Phase 4 terminée

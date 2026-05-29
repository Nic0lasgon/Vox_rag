# Vox_rag — Module Embedding, Similarité et Topics pour VoxPod

Vox_rag est le module d'analyse sémantique de VoxPod. Il reçoit les articles qualifiés par l'ingest-pipeline, génère des embeddings vectoriels, détecte les similarités entre articles, et les regroupe automatiquement en dossiers d'actualité (topics).

## Vue d'ensemble

**Mission** : "Quand un nouvel article arrive, à quelle histoire en cours appartient-il, et quelle nouvelle étape ajoute-t-il ?"

**Pipeline** :
```
Articles qualifiés (ingest-pipeline)
  → Import dans Vox_rag
  → Construction du profile_text
  → Embedding via Octen API (4096 dims)
  → Stockage pgvector
  → Recherche similarité cosinus
  → Décision : lier à un topic existant ou créer un nouveau
  → Mise à jour du topic (résumé, timeline, markdown)
```

**Stack technique** :
- **Langage** : Rust (edition 2024)
- **Runtime** : Tokio + Axum
- **Base de données** : PostgreSQL 16 + pgvector
- **Embedding** : Octen API (`octen-embedding-8b`, 4096 dimensions)
- **Arbitrage ambigu** : DeepSeek API (`deepseek-v4-flash`)

## Architecture

### Modes de lancement

```bash
cargo run -- api          # Serveur HTTP Axum uniquement
cargo run -- worker       # Worker d'embedding uniquement
cargo run -- topic-worker # Worker de topics uniquement
cargo run -- all          # API + workers (dev local)
```

**Important** : en raison de [BUG-004](#bugs-connus), les workers `embedding` et `topic` partagent la même file de jobs sans filtrer par `target_type`. Lancez-les **séquentiellement** : d'abord le worker d'embedding, puis le topic-worker.

### Flux de données détaillé

```
┌─ ingest-pipeline (port 5432) ─────────────────────────┐
│  RSS → fetch → extraction HTML → dedup → qualification │
│  → raw_articles (quality_status = 'qualified')         │
└────────────────────────────────────────────────────────┘
                           ↓
┌─ Import (scripts/import_from_ingest.sh) ──────────────┐
│  Export CSV → insertion table articles (port 5433)     │
└────────────────────────────────────────────────────────┘
                           ↓
┌─ Vox_rag (port 5433) ─────────────────────────────────┐
│  1. Profile : construction du texte à embedder         │
│  2. Job queue : création d'un job pending              │
│  3. Embedding worker : appel Octen API → vecteur 4096d │
│  4. Stockage : article_profiles (VECTOR(4096))         │
│  5. Topic worker : recherche similarité → décision     │
│     - Score ≥ 0.92 : near_duplicate                    │
│     - Score 0.86-0.92 : same_story_update              │
│     - Score 0.78-0.86 : background_context             │
│     - Score 0.76-0.84 : ambiguous (LLM si activé)      │
│     - Score < 0.76 : nouveau topic                     │
│  6. Mise à jour : topic_markdown, timeline, counts     │
└────────────────────────────────────────────────────────┘
```

### Structure du projet

```
Vox_rag/
├── src/
│   ├── main.rs                  # Entry point + CLI (clap)
│   ├── config.rs                # Configuration env
│   ├── api/                     # HTTP API (Axum)
│   │   ├── health.rs
│   │   ├── rag.rs               # Recherche RAG
│   │   └── topics.rs            # CRUD topics
│   ├── embedding/               # Fournisseurs d'embeddings
│   │   ├── provider.rs          # Trait EmbeddingProvider
│   │   ├── octen.rs             # Client Octen API
│   │   └── mock.rs              # Provider mock (tests)
│   ├── llm/                     # Fournisseur LLM
│   │   ├── provider.rs
│   │   ├── deepseek.rs          # Client DeepSeek
│   │   ├── mock.rs
│   │   └── prompt.rs
│   ├── pipeline/                # Logique métier
│   │   ├── profile.rs           # Construction profile_text
│   │   ├── chunker.rs           # Découpage en chunks
│   │   ├── similarity.rs        # Classification similarité
│   │   └── rag_search.rs        # Recherche RAG
│   ├── topic/                   # Gestion des dossiers d'actualité
│   │   ├── candidate.rs         # Recherche topics candidats
│   │   ├── decision.rs          # Décision article → topic
│   │   ├── mutation.rs          # CRUD topics
│   │   ├── markdown.rs          # Génération topic_markdown
│   │   ├── embedding.rs         # Embedding topics
│   │   └── timeline.rs          # Événements timeline
│   ├── db/                      # Accès données
│   │   ├── schema.rs
│   │   ├── article_queries.rs
│   │   ├── embedding_queries.rs
│   │   ├── chunk_queries.rs
│   │   ├── topic_queries.rs
│   │   └── job_queries.rs       # File de jobs (SKIP LOCKED)
│   └── workers/
│       ├── embedding_worker.rs
│       └── topic_worker.rs
├── migrations/                  # 10 migrations PostgreSQL
├── scripts/
│   ├── import_from_ingest.sh   # Import articles qualifiés
│   └── test_with_real_articles.sh
├── Cargo.toml
├── docker-compose.yml
└── .env.example
```

## Seuils de similarité

Les seuils sont configurables via variables d'environnement. Valeurs par défaut :

| Score | Classification | Signification |
|-------|---------------|---------------|
| ≥ 0.92 | `near_duplicate` | Quasi-doublon ou reprise très proche |
| 0.86 — 0.92 | `same_story` | Même événement / même actualité |
| 0.78 — 0.86 | `context` | Sujet lié ou contexte potentiel |
| 0.70 — 0.78 | `weak` | Lien faible |
| < 0.70 | `unrelated` | Ignoré |

**Seuils de décision topic** :

| Seuil | Valeur | Action |
|-------|--------|--------|
| `TOPIC_STRONG_MATCH_THRESHOLD` | 0.84 | Lien fort → rattachement direct |
| `TOPIC_AMBIGUOUS_MATCH_THRESHOLD` | 0.76 | Zone ambigu → arbitrage LLM (si activé) |
| `TOPIC_WEAK_MATCH_THRESHOLD` | 0.70 | En dessous → nouveau topic |

## Démarrage rapide

### Prérequis

- Rust 1.88+
- PostgreSQL 16+ avec extension pgvector (ou Docker)
- Clé API Octen
- Module `ingest-pipeline` opérationnel (port 5432)

### Avec Docker Compose

```bash
# 1. Configurer les variables d'environnement
cp .env.example .env
# Éditer .env avec vos clés API

# 2. Lancer le stack
docker compose up -d

# 3. Vérifier le health check
curl http://localhost:3000/health

# 4. Voir les logs
docker compose logs -f backend
```

### Sans Docker (dev local)

```bash
# PostgreSQL avec pgvector doit tourner sur le port 5433
export DATABASE_URL="postgres://voxrag:voxrag_dev@localhost:5433/vox_rag"
export OCTEN_API_KEY="octen-votre-cle-ici"
export LLM_API_KEY="sk-votre-cle-ici"  # Optionnel

# Lancer (migrations automatiques au démarrage)
cargo run -- all
```

### Lancer les tests

```bash
# Quality gates (obligatoire avant chaque commit)
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Variables d'environnement

Toutes les variables sensibles (clés API) sont dans `.env` (fichier gitignoré). Jamais de secrets en dur dans le code.

| Variable | Obligatoire | Défaut | Description |
|----------|-------------|--------|-------------|
| `DATABASE_URL` | Oui | — | URL PostgreSQL (port 5433) |
| `PORT` | Non | 3000 | Port API HTTP |
| `LOG_LEVEL` | Non | info | Niveau de log |
| `OCTEN_API_KEY` | Oui | — | Clé API Octen |
| `OCTEN_API_BASE_URL` | Non | https://api.octen.ai | URL base Octen |
| `EMBEDDING_MODEL` | Non | octen-embedding-8b | Modèle d'embedding |
| `EMBEDDING_DIMENSION` | Non | 4096 | Dimension des vecteurs (voir [BUG-002](#bugs-connus)) |
| `EMBEDDING_BATCH_SIZE` | Non | 32 | Taille de batch |
| `EMBEDDING_MAX_RETRIES` | Non | 3 | Tentatives max par job |
| `EMBEDDING_TIMEOUT_MS` | Non | 60000 | Timeout API (ms) |
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
| `USE_LLM_FOR_TOPICS` | Non | false | Activer l'arbitrage LLM |
| `LLM_API_KEY` | Non | — | Clé API DeepSeek |
| `LLM_MODEL` | Non | deepseek-v4-flash | Modèle LLM |
| `LLM_BASE_URL` | Non | https://api.deepseek.com | URL API LLM |
| `LLM_TIMEOUT_MS` | Non | 30000 | Timeout LLM (ms) |

## Schéma de base de données

### Tables principales

- **`articles`** — Articles importés depuis ingest-pipeline (texte nettoyé, métadonnées)
- **`article_profiles`** — Texte de profil + embedding vectoriel (VECTOR(4096))
- **`article_chunks`** — Chunks d'articles pour la recherche RAG (VECTOR(4096))
- **`topics`** — Dossiers d'actualité (titre, résumé, markdown, embedding)
- **`topic_articles`** — Liaison articles ↔ topics avec type de relation
- **`topic_timeline_events`** — Événements chronologiques du topic
- **`embedding_jobs`** — File de jobs de traitement (pending → running → done/failed)

### Index vectoriels

- `article_profiles` : index cosinus sur `embedding`
- `article_chunks` : index cosinus sur `embedding`
- `topics` : index cosinus sur `embedding`

**Note** : pgvector ne supporte pas HNSW au-delà de 2000 dimensions. Les index utilisent un scan par brute-force, acceptable pour < 10 000 articles. Au-delà, envisager IVFFlat ou une réduction de dimension.

## Résultats de validation qualité

Validation réalisée le 2026-05-29 sur **131 articles** provenant de **7 sources** internationales (BBC, The Guardian, Euronews, Al Jazeera, L'Obs, France24).

### Métriques clés

| Métrique | Valeur |
|----------|--------|
| Articles embeddés | 124 (7 skippés, trop courts) |
| Topics créés | 112 |
| Topics multi-articles | 9 (2-4 articles) |
| Topics multi-sources | 7 (couverture croisée 2-3 médias) |
| Topics singletons | 103 (92%) |
| Appels LLM | 0 |
| Faux merges détectés | 0 |

### Méthodes de décision

| Méthode | Count | Description |
|---------|-------|-------------|
| `auto_create` | 110 | Premier article d'un topic |
| `rules` | 12 | Décision par seuils |
| `auto_create_ambiguous` | 2 | Fallback sans LLM (zone ambigu) |

### Relations détectées

| Relation | Count |
|----------|-------|
| `same_story_update` | 116 |
| `related_but_different` | 6 |
| `near_duplicate` | 2 |

### Constats

- Le pipeline fonctionne correctement sur les cas évidents : les 7 topics multi-sources sont pertinents et bien fusionnés.
- **Problème principal** : taux de singletons très élevé (92%). Les seuils de topic sont probablement trop hauts pour le texte actuel, qui contient encore du bruit (cookies, navigation, pubs) issu de l'extraction HTML.
- Le LLM DeepSeek n'a jamais été déclenché : aucun score n'est tombé dans la zone ambiguë (0.76-0.84).
- 1 split probable : deux topics sur l'incendie d'internat au Kenya couvrent le même événement mais n'ont pas été fusionnés (score probablement < 0.76).

Le détail audit topic-by-topic est disponible dans `docs/validation-report-2026-05-29.md`. Le fichier `docs/validation-audit.csv` contient les 124 lignes d'audit détaillé (topic_id, article_id, relation_type, score, etc.).

## Bugs connus

### BUG-002 — Dimension vectorielle 1536 vs 4096 [CORRIGÉ]

- **Symptôme** : `expected 1536 dimensions, not 4096` à l'insertion des embeddings
- **Cause** : Migrations en `VECTOR(1536)` mais le modèle `octen-embedding-8b` retourne 4096 dimensions
- **Fix** : Passage à `VECTOR(4096)` dans les migrations 003, 005, 006, 007, 010. Suppression des index HNSW (limite pgvector = 2000 dims).
- **Statut** : Corrigé dans le code

### BUG-004 — Workers partagent la même file de jobs [WORKAROUND]

- **Fichier** : `src/db/job_queries.rs:46` (`pick_pending_jobs`)
- **Symptôme** : Le topic worker prend les jobs `article_profile`, les marque `failed`. L'embedding worker ne peut plus les traiter. Le mode `all` ne fonctionne pas.
- **Cause** : `pick_pending_jobs` ne filtre pas par `target_type`
- **Workaround** : Lancer les workers séquentiellement (embedding d'abord, puis topic)
- **Fix nécessaire** : Ajouter un filtre `target_type` à `pick_pending_jobs`

### BUG-005 — Script import DB URL incorrect [WORKAROUND]

- **Fichier** : `scripts/import_from_ingest.sh:12`
- **Symptôme** : Authentification échouée
- **Cause** : URL hardcodée `postgres://postgres:postgres@localhost:5432` au lieu des credentials réels
- **Workaround** : Exécution manuelle des commandes SQL avec les bons credentials
- **Fix nécessaire** : Utiliser une variable d'environnement ou `.env` pour l'URL ingest-pipeline

## Points d'attention

1. **Dimension vectorielle : toujours 4096** — Le modèle `octen-embedding-8b` produit des vecteurs de 4096 dimensions. Ne jamais remettre 1536 dans les migrations.

2. **Pas d'index HNSW au-delà de 2000 dims** — pgvector ne supporte pas HNSW au-delà de 2000 dimensions. Le brute-force est acceptable pour < 10 000 articles. Au-delà, envisager IVFFlat ou réduire la dimension.

3. **Seuils de similarité** — Les seuils actuels (0.84 strong, 0.76 ambiguous) ont été calibrés sur du texte bruité. Après l'intégration de rs-trafilatura (nettoyage HTML) dans ingest-pipeline, il faudra probablement les ajuster à la baisse (0.78 / 0.70) car le texte sera plus propre et les scores de similarité plus élevés.

4. **LLM jamais validé en conditions réelles** — 0 appel DeepSeek sur 124 articles. La zone ambiguë n'est jamais atteinte. Après nettoyage du texte, les scores augmenteront et le LLM sera plus souvent déclenché — il faudra valider qu'il fonctionne correctement.

5. **Singletons (92%)** — Le principal levier d'amélioration. Causes probables : texte bruité, seuils trop hauts, manque de sources couvrant les mêmes sujets.

## Lien avec ingest-pipeline

Vox_rag ne fonctionne pas en autonomie : il dépend de l'`ingest-pipeline` (port 5432) pour son approvisionnement en articles.

### Flux d'intégration

```
Flux RSS → ingest-pipeline
  → fetch RSS
  → extraction HTML (trafilatura / Scrapling Hetzner pour JS)
  → déduplication
  → qualification (quality_status = 'qualified')
  → raw_articles (PostgreSQL, port 5432)
    → import Vox_rag (scripts/import_from_ingest.sh)
    → articles (PostgreSQL, port 5433)
      → embedding → topics
```

### Import des articles

Le script `scripts/import_from_ingest.sh` exporte les articles qualifiés depuis ingest-pipeline vers Vox_rag :

```bash
# Prérequis : ingest-pipeline et Vox_rag doivent être démarrés
cd Vox_rag
./scripts/import_from_ingest.sh
```

**Conditions d'import** :
- `quality_status = 'qualified'` (obligatoire)
- `content IS NOT NULL`
- `length(content) > 300`
- Limite : 500 articles par import

**Note** : Le script contient [BUG-005](#bugs-connus) (URL DB hardcodée). Vérifiez les credentials avant exécution.

### Extraction de contenu

Pour les sites JavaScript-rendered (Euronews, RFI, etc.), ingest-pipeline utilise le serveur Scrapling Hetzner :

```bash
curl -X POST "https://search.myswearpod.de/extract" \
  -H "Authorization: Bearer $HETZNER_EXTRACT_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"url": "...", "stealth": true, "format": "txt"}'
```

### Sources testées (validation 2026-05-29)

| Source | Articles importés | Statut |
|--------|-------------------|--------|
| BBC World | 28 | OK |
| The Guardian World | 26 | OK |
| Euronews English | 25 | OK |
| Al Jazeera | 19 | OK |
| The Guardian Politics | 15 | OK |
| L'Obs | 14 | OK |
| France24 English | 4 | OK |

**Sources bloquées** : Le Monde (paywall), NYT (paywall), Reuters (DNS), CNN (bloquage), Deutsche Welle (bloquage).

## Conventions de code

- `anyhow::Result` pour la gestion d'erreurs
- `tracing` pour les logs structurés
- Pas de `unwrap()` hors des tests
- `sqlx::query_as::<_, T>()` avec `.bind()` (pas de macro compile-time)
- `async_trait` pour les traits asynchrones
- `SKIP LOCKED` pour la job queue (jamais un simple `UPDATE`)

### Quality gates (obligatoire avant chaque commit)

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## API interne (aperçu)

### Health check
```
GET /health
→ {"status": "ok"}
```

### Embedding d'un article
```
POST /internal/embeddings/article-profile/:articleId
→ {"status": "job_created", "job_id": "..."}
```

### Articles similaires
```
GET /internal/articles/:articleId/similar?top_k=30
```

### Contexte historique
```
GET /internal/articles/:articleId/historical-context?lookback_days=30&top_k=20
```

### Recherche RAG
```
POST /internal/rag/search
Body : {"query": "...", "top_k": 20, "language": "fr"}
```

### Dossiers d'actualité (Topics)
```
POST   /internal/topics                          # Créer un topic
GET    /internal/topics/:topicId                 # Lire un topic
POST   /internal/topics/:topicId/rebuild-markdown
POST   /internal/topics/:topicId/reembed
POST   /internal/topics/:topicId/merge           # Body: {"target_topic_id": "..."}
GET    /internal/articles/:articleId/topic-candidates
POST   /internal/articles/:articleId/assign-topic
POST   /internal/jobs/process-article-batch      # Body: {"article_ids": [...]}
```

## Déploiement

### Local (Docker Compose)

```bash
docker compose up -d
```

### Production

```bash
docker build -t vox-rag .
# Configurer les variables d'environnement
# Lancer : vox-rag api / vox-rag worker / vox-rag all
```

**Points de vigilance production** :
- Utiliser l'image `pgvector/pgvector:pg16`
- Le worker peut tourner sur plusieurs instances (grâce au `SKIP LOCKED`)
- Surveiller les jobs `failed` (échecs répétés d'API)
- Les migrations s'appliquent automatiquement au démarrage
- Lancer les workers séquentiellement jusqu'à correction de [BUG-004](#bugs-connus)

## Ressources

- **Module amont** : `../ingest-pipeline/`
- **Rapport de validation** : `docs/validation-report-2026-05-29.md`
- **Audit détaillé (CSV)** : `docs/validation-audit.csv`
- **Référence DeepSeek API** : `docs/deepseek-api-reference.md`
- **PRD Embedding + RAG** : `Documentation/Step Rag/PRD - Système Embedding + RAG.pdf`
- **PRD Dossiers d'actualité** : `Documentation/Step Rag/PRD - Système de dossiers d'actualité vivants pour VoxPod.pdf`

---

**Version** : v0.4.0  
**Dernière mise à jour** : 2026-05-29  
**Statut** : Phases 1-4 terminées. Validation qualité en cours (131 articles testés).

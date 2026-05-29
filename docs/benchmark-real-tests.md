# Vox_rag — Tests et Benchmarks Réels

Tests réalisés le 2026-05-29 avec de vrais articles RSS et l'API Octen/DeepSeek.

## Environnement de test

| Composant | Version | Détail |
|---|---|---|
| **OS** | macOS (darwin) | MacBook Pro |
| **Rust** | edition 2024 | Cargo 1.88+ |
| **PostgreSQL** | 16 (pgvector/pgvector:pg16) | Docker, port 5433 |
| **pgvector** | 0.4.2 | Extension vector, HNSW désactivé (limite 2000 dim) |
| **Octen API** | octen-embedding-8b | 1536 dimensions, $0.07/1M tokens |
| **DeepSeek API** | deepseek-v4-flash | Thinking désactivé, $0.14/1M input, $0.28/1M output |

## Sources RSS testées

| Source | URL | Articles récupérés | Articles embeddés |
|---|---|---|---|
| **The Guardian** | feeds.guardian.co.uk/theguardian/world/rss | 2 | 2 |
| **L'Obs** | nouvelobs.com/a-la-une/rss.xml | 2 | 1 |
| **Euronews** | feeds.feedburner.com/euronews/en/home/ | 2 | 0 (trop court) |
| **RFI** | english.rfi.fr/asia-pacific/rss | 2 | 0 (trop court) |

**Note** : Les descriptions RSS sont souvent trop courtes (< 80 mots). Pour un vrai pipeline, il faut récupérer le contenu complet des articles via leur URL.

## Benchmark 1 : Embedding Octen

### Métriques mesurées

| Métrique | Valeur |
|---|---|
| **Latence par article** | ~900ms |
| **Tokens par article** | ~282 tokens |
| **Dimension** | 1536 |
| **Coût par embedding** | ~$0.00002 |
| **Modèle** | octen-embedding-8b |

### Coûts projetés

| Volume | Coût embedding |
|---|---|
| 1 000 articles | ~$0.02 |
| 10 000 articles | ~$0.20 |
| 100 000 articles | ~$2 |
| 1 000 000 articles | ~$20 |

### Profil envoyé à Octen

```
Title: WHO puts Ebola outbreak death rate at 'huge' 30-50% as chief arrives in DRC
Source: The Guardian
Published at: 2026-05-29T16:08:22.345337+00:00
Language: en
Description: Tedros Adhanom Ghebreyesus calls for ceasefire among armed groups...
Article lead:
{texte nettoyé, max 1500 caractères}
```

## Benchmark 2 : Recherche de similarité pgvector

### Résultats mesurés

| Article A | Article B | Similarité | Classification |
|---|---|---|---|
| WHO Ebola (Guardian) | Friday briefing Ebola (Guardian) | **0.971** | `near_duplicate` |
| WHO Ebola (Guardian) | New Ebola BBC (test) | **0.694** | `unrelated` |
| WHO Ebola (Guardian) | Climatologues cyberharcèlement (L'Obs) | **0.142** | `unrelated` |
| Friday briefing Ebola (Guardian) | Climatologues cyberharcèlement (L'Obs) | **0.089** | `unrelated` |

### Seuils de classification

| Score | Classification | Comportement |
|---|---|---|
| ≥ 0.92 | `near_duplicate` | Lier au même topic, pas de timeline |
| 0.86 — 0.92 | `same_story_update` | Lier au topic, mettre à jour résumé |
| 0.78 — 0.86 | `background_context` | Contexte historique |
| 0.70 — 0.78 | `weak` | Lien faible |
| < 0.70 | `unrelated` | Ignoré |

### Performance

La recherche de similarité est **instantanée** (< 10ms) même sans index HNSW (4 articles). Avec l'index HNSW et des milliers d'articles, la recherche reste en temps constant.

## Benchmark 3 : Topic Processing

### Résultats mesurés

| Article | Topic créé | Relation | Score | Méthode |
|---|---|---|---|---|
| WHO Ebola (Guardian) | WHO puts Ebola... | `same_story_update` | 1.0 | `auto_create` |
| Friday briefing Ebola (Guardian) | Friday briefing... | `same_story_update` | 1.0 | `auto_create` |
| Climatologues cyberharcèlement (L'Obs) | Le but est de nous faire taire... | `same_story_update` | 1.0 | `auto_create` |
| New Ebola BBC (test) | New Ebola cases... | `same_story_update` | 1.0 | `auto_create` |

### Topic Markdown généré

```markdown
# WHO puts Ebola outbreak death rate at 'huge' 30-50% as chief arrives in DRC

## Statut
new

## Résumé
Tedros Adhanom Ghebreyesus calls for ceasefire among armed groups to help avoid deaths from preventable disease in DRC.

## Entités principales
- WHO
- DRC
- Ebola

## Chronologie
- 2026-05-29 : WHO puts Ebola outbreak death rate at 'huge' 30-50% as chief arrives in DRC
```

## Benchmark 4 : LLM DeepSeek (arbitrage)

### Test API réel

| Métrique | Avec thinking | Sans thinking | Économie |
|---|---|---|---|
| **completion_tokens** | 256-288 | **91-100** | **-65%** |
| **Coût par appel** | ~$0.000118 | **~$0.000069** | **-43%** |
| **Langue** | Anglais (par défaut) | **Français** | — |
| **reasoning_content** | 162 tokens | **None** | — |

### Exemple de réponse LLM

```json
{
  "decision": "same_story_update",
  "confidence": 0.95,
  "should_update_topic_summary": true,
  "should_create_timeline_event": true,
  "timeline_event_type": "cessez-le-feu",
  "reason": "L'article annonce un cessez-le-feu, ce qui est une évolution directe et significative de l'escalade militaire en cours."
}
```

### Coûts projetés (LLM)

| Volume (décisions ambiguës) | Coût DeepSeek |
|---|---|
| 100 décisions/jour | ~$0.007/jour |
| 1 000 décisions/jour | ~$0.07/jour |
| 10 000 décisions/jour | ~$0.70/jour |

**Note** : Le LLM n'est appelé QUE pour les cas ambigus (score 0.76-0.84). Dans notre test, aucun article n'est tombé dans cette zone.

## Benchmark 5 : Pipeline complet

### Latence end-to-end

| Étape | Latence | Tokens |
|---|---|---|
| Fetch RSS | ~2s | — |
| Extraction HTML | ~100ms | — |
| Embedding Octen | ~900ms | ~282 |
| Recherche similarité | < 10ms | — |
| Topic processing | ~50ms | — |
| Topic embedding | ~900ms | ~200 |
| **Total** | **~4s** | **~500** |

### Coût end-to-end par article

| Composant | Coût |
|---|---|
| Embedding article | $0.00002 |
| Embedding topic | $0.000014 |
| LLM (si ambigu) | $0.00007 |
| **Total (sans LLM)** | **~$0.00003** |
| **Total (avec LLM)** | **~$0.0001** |

## Problèmes rencontrés et solutions

### 1. pgvector HNSW : limite 2000 dimensions

**Problème** : `CREATE INDEX ... USING hnsw` échoue avec 4096 dimensions.
**Solution** : Réduit à 1536 dimensions (comme OpenAI). L'index HNSW n'est pas critique pour les tests.

### 2. Type NUMERIC vs FLOAT8

**Problème** : sqlx ne convertit pas automatiquement `NUMERIC` → `f64`.
**Solution** : Changé les migrations pour utiliser `FLOAT8` au lieu de `NUMERIC`.

### 3. Descriptions RSS trop courtes

**Problème** : Les descriptions RSS font 30-60 mots, en dessous du seuil de 80 mots.
**Solution** : Pour un vrai pipeline, récupérer le contenu complet via l'URL de l'article.

### 4. Docker credential helper

**Problème** : `docker-credential-desktop` introuvable dans le PATH.
**Solution** : Modifier `~/.docker/config.json` pour désactiver le credential store.

### 5. Workers concurrents

**Problème** : Le topic worker traite les jobs `article_profile` par erreur.
**Solution** : Lancer l'embedding worker AVANT le topic worker, ou utiliser `cargo run -- all`.

## Recommandations

### Pour la production

1. **Récupérer le contenu complet** des articles (pas seulement les descriptions RSS)
2. **Activer l'index HNSW** avec 1536 dimensions (ou moins)
3. **Lancer les deux workers** ensemble : `cargo run -- all`
4. **Surveiller les coûts** via les logs DeepSeek (chaque appel log `cost_usd`)
5. **Ajuster les seuils** selon les résultats (0.76 peut être trop bas pour le topic matching)

### Pour le développement

1. **Utiliser `EMBEDDING_PROVIDER=mock`** pour les tests rapides (pas de réseau)
2. **Utiliser `USE_LLM_FOR_TOPICS=false`** pour désactiver le LLM
3. **Vérifier les logs** : `docker compose logs -f` pour voir les décisions
4. **Tester avec peu d'articles** d'abord (3-5) avant de lancer des batches

## Scripts de test

```bash
# Test complet automatisé
./scripts/test_with_real_articles.sh

# Import depuis ingest-pipeline
./scripts/import_from_ingest.sh

# Test API DeepSeek seul
python3 -c "
import json, urllib.request
data = json.dumps({...}).encode()
req = urllib.request.Request('https://api.deepseek.com/chat/completions', data=data, headers={...})
resp = urllib.request.urlopen(req)
print(json.loads(resp.read()))
"
```

---

**Date des tests** : 2026-05-29
**Version Vox_rag** : v0.4.0
**Articles testés** : 9 (Euronews, The Guardian, L'Obs, RFI, BBC)
**Embeddings créés** : 9 (Octen octen-embedding-8b)
**Topics créés** : 9
**Appels LLM** : 0 (pas de cas ambigus)

---

## Mises à jour — Test complet avec Scrapling (2026-05-29)

### Scrapling (extraction contenu)

Le serveur Scrapling Hetzner (`search.myswearpod.de/extract`) a été testé pour récupérer le contenu complet des articles dont les descriptions RSS étaient trop courtes.

| Article | Source | Méthode | Mots |
|---|---|---|---|
| Colombia elections | Euronews | Direct | 3 225 |
| Ukraine drone barrage | Euronews | Direct | 1 385 |
| WHO Ebola | The Guardian | Direct | 1 623 |
| Friday briefing Ebola | The Guardian | Direct | 3 167 |
| Climatologues cyberharcèlement | L'Obs | Direct | 1 210 |
| Drone russe Roumanie | L'Obs | Direct | 1 990 |
| EU Hungary | RFI | **Scrapling** | 1 229 |
| Budapest Pride | RFI | **Scrapling** | 1 046 |

**Scrapling a permis de récupérer 2 articles RFI qui échouaient en HTTP direct** (sites JavaScript-rendered).

### API Scrapling

```
POST https://search.myswearpod.de/extract
Authorization: Bearer <HETZNER_EXTRACT_SECRET>
Content-Type: application/json

{
  "url": "https://example.com/article",
  "stealth": true,
  "format": "txt"
}
```

Timeout : 90 secondes. Gère Cloudflare Turnstile, cookie walls, AMP.

### Similarité avec vrais articles

| Article A | Article B | Score |
|---|---|---|
| Friday briefing Ebola | WHO Ebola | 0.514 |
| Drone Roumanie | Ukraine drone | 0.476 |
| EU Hungary | Hungary Pride | 0.450 |
| Colombia | Ebola | 0.262 |

**Constat** : Les scores sont plus bas que prévu car les embeddings sont basés sur les 5000 premiers caractères (bruit de cookies/navigation inclus). Pour de meilleurs résultats, il faut mieux nettoyer le texte extrait.

### Problèmes identifiés

1. **Texte extrait non nettoyé** : Les contenus scrapés contiennent des messages de cookies, navigation, pub. Il faut un nettoyage avant embedding.
2. **Seuil de similarité** : Avec 9 articles de sujets différents, aucun n'atteint le seuil de 0.76 pour le topic matching. Les articles du même sujet (Ebola) atteignent 0.51 — en dessous du seuil.
3. **LLM non déclenché** : Aucun cas ambigus dans ce test (scores tous < 0.76). Le LLM n'a pas été testé en conditions réelles.

### Recommandations pour la validation

1. **Nettoyer le texte** avant embedding (supprimer cookies, navigation, pub)
2. **Utiliser le contenu complet** (pas tronqué à 5000 chars)
3. **Tester sur 200-500 articles** pour observer les vrais patterns de similarité
4. **Créer un goldset manuel** (100-200 articles annotés) pour mesurer la qualité
5. **Ajuster les seuils** après observation des résultats réels

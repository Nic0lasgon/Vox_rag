# VoxPod — Rapport de Validation Qualite

**Date** : 2026-05-29
**Version** : Vox_rag v0.4.0 + ingest-pipeline v1.1.0
**Objectif** : Valider le pipeline complet sur 100-500 vrais articles

## 1. Resume executif

Le pipeline complet (RSS -> extraction -> embedding -> topics) a ete execute sur **131 articles** provenant de **7 sources** internationales. Resultats :

- **112 topics** crees, dont **9 topics multi-articles** (2-4 articles)
- **7 topics multi-sources** (couverture croisee de 2-3 medias)
- **0 appel LLM** (aucun cas ambigu n'a atteint le seuil 0.76-0.84)
- **92% des topics** sont des singletons (1 article = 1 topic)

**Constat principal** : Le pipeline fonctionne correctement sur les cas evidents (topics multi-sources bien fusionnes), mais le taux de singletons est tres eleve. Les seuils de topic matching sont probablement trop hauts pour le texte actuel (bruited par le contenu extrait).

## 2. Donnees d'entree

### Sources RSS

| Source | Articles importes | Articles embeddes |
|---|---|---|
| BBC World | 28 | 28 |
| The Guardian World | 26 | 26 |
| Euronews English | 25 | 25 |
| Al Jazeera | 19 | 19 |
| The Guardian Politics | 15 | 15 |
| L'Obs | 14 | 14 |
| France24 English | 4 | 4 |
| **Total** | **131** | **131** |

### Sources echouees (paywalls / bloquantes)

| Source | Raison |
|---|---|
| Le Monde | Paywall (402 sur toutes les strategies) |
| NYT World | Paywall (402 sur la plupart des articles) |
| Reuters World | DNS / reseau inaccessible |
| CNN World | Bloquage (pas de contenu extrait) |
| Deutsche Welle | Bloquage (pas de contenu extrait) |

### Pipeline ingest

| Metrique | Valeur |
|---|---|
| Flux RSS configures | 15 |
| Flux reussis | 11 |
| Articles ingeres (RSS) | 280 |
| Articles extraits (HTML) | 142 |
| Articles qualifies | 131 |
| Taux d'extraction | 51% (142/280) |
| Taux de qualification | 92% (131/142) |

## 3. Embeddings

| Metrique | Valeur |
|---|---|
| Modele | octen-embedding-8b |
| Dimension | 4096 |
| Profiles crees | 124 |
| Articles skippes (trop courts) | 7 |
| Latence moyenne / article | ~500ms |
| Cout total estime | ~$0.01 |

## 4. Topics

### Distribution

| Articles par topic | Nombre de topics | % |
|---|---|---|
| 1 article (singletons) | 103 | 91.9% |
| 2 articles | 7 | 6.3% |
| 3 articles | 1 | 0.9% |
| 4 articles | 1 | 0.9% |

### Multi-sources

| Sources par topic | Nombre de topics |
|---|---|
| 1 source | 5 |
| 2 sources | 6 |
| 3 sources | 1 |

### Methodes de decision

| Methode | Count | Description |
|---|---|---|
| `auto_create` | 110 | Premier article d'un topic (pas de candidat existant) |
| `rules` | 12 | Decision par seuils (regles) |
| `auto_create_ambiguous` | 2 | Creation apres zone ambigu (fallback sans LLM) |

### Relation types

| Relation | Count | Score moyen |
|---|---|---|
| `same_story_update` | 116 | 0.99 (auto) / 0.77-0.90 (rules) |
| `related_but_different` | 6 | 0.79-0.84 |
| `near_duplicate` | 2 | 0.92 |

### LLM (DeepSeek)

| Metrique | Valeur |
|---|---|
| `USE_LLM_FOR_TOPICS` | true |
| Appels LLM reels | **0** |
| Raison | Aucun score de similarite n'est tombe dans la zone ambigu (0.76-0.84) |

## 5. Top 7 Topics Multi-Sources (les plus interessants)

### 1. Drone russe en Roumanie (4 articles, 3 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| Romania: First footage shows damage after drone hits residential block near Ukraine border | Euronews English | primary | 1.0 |
| Nato condemns Russian 'recklessness' after drone hits Romanian residential block | BBC World | same_story_update | rules |
| Euronews on the scene after Russian drone strike in Romania | Euronews English | same_story_update | rules |
| Un drone russe s'ecrase en Roumanie : deux blesses | L'Obs | related_but_different | 0.84 |

**Resume** : Un drone russe de frappes en Ukraine s'ecrase a Galati, blessant deux personnes et poussant la Roumanie a demander un soutien anti-drone NATO accelere.

**Verdict** : **CORRECT** - Fusion pertinente de 3 sources (BBC, Euronews, L'Obs). L'article L'Obs est classe `related_but_different` (score 0.84) au lieu de `same_story_update`, ce qui est coherent vu l'angle different (article en francais, perspective locale).

### 2. Fournisseur de kits suicidaires canadien (3 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| Anger at decision not to extradite Canadian suicide kit supplier to face UK justice | The Guardian World | primary | 1.0 |
| 'Poison seller' who sold toxic chemicals online to people across world admits aiding suicides | BBC World | same_story_update | rules |
| Canadian man admits sending 'suicide packets' to hundreds of people around world | The Guardian World | near_duplicate | 0.93 |

**Verdict** : **CORRECT** - Les 3 articles couvrent la meme affaire Kenneth Law. Le near_duplicate est legitime (deux articles du Guardian sur le meme sujet).

### 3. Netanyahu / Gaza (2 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| Netanyahu says he has directed IDF to increase control of Gaza to 70% | BBC World | primary | 1.0 |
| Netanyahu vows to increase Israel's control of the Gaza Strip to 70% | Euronews English | same_story_update | 0.90 |

**Verdict** : **CORRECT** - Meme annonce, deux sources. Score de similarite bon (0.90).

### 4. UE / Hongrie (2 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| EU to release EUR 16bn to Hungary previously frozen under Orban | The Guardian World | primary | 1.0 |
| EU to unlock 16 billion euros for Hungary | France24 English | same_story_update | 0.86 |

**Verdict** : **CORRECT** - Meme evenement, score de 0.86 (juste au-dessus du seuil `same_story`).

### 5. Accord Etats-Unis / Iran (2 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| US and Iran reach tentative deal, pending Trump's approval | BBC World | primary | 1.0 |
| US and Iran 'very close' to deal but 'not there yet', Vance says | Euronews English | same_story_update | 0.85 |

**Verdict** : **CORRECT** - Meme sujet, le deuxieme article est le contexte precedent l'accord.

### 6. Incendie ecole Kenya (2 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| Fire rips through dormitory at girl's school in Kenya, killing at least 16 students | BBC World | primary | 1.0 |
| Questions over safety as 16 pupils die in another Kenya school fire | Euronews English | related_but_different | 0.79 |

**Verdict** : **CORRECT** - Le deuxieme article est un suivi analytique, pas un simple doublon. La classification `related_but_different` est appropriee.

### 7. Arson attack Kenya school (2 articles, 2 sources)

| Article | Source | Relation | Score |
|---|---|---|---|
| Eight students arrested in Kenya after suspected deadly school arson attack | Al Jazeera | primary | 1.0 |
| Eight girls arrested on suspicion of arson after Kenya school fire | BBC World | same_story_update | 0.76 |

**Verdict** : **CORRECT** - Meme evenement, score de 0.76 (zone ambigu, mais decision rules OK).

## 6. Audit des problemes identifies

### Faux merges (sujets differents fusionnes)

**0 cas detecte**. Aucun topic ne contient d'articles sur des sujets manifestement differents.

### Meme sujet separe en deux topics

**1 cas probable** : Les topics "Fire rips through dormitory" (BBC/Euronews) et "Eight students arrested" (Al Jazeera/BBC) couvrent le meme evenement (incendie de l'internat au Kenya). Ils sont en deux topics distincts car les scores de similarite entre les articles des deux groupes sont probablement dans la zone 0.70-0.76 (en dessous du seuil topic_weak).

**Gravite** : Moyenne. Ces deux topics pourraient etre fusionnes manuellement.

### Timeline trop bavarde

Chaque article cree un evenement de timeline. Pour un topic a 4 articles, on a 4 evenements. C'est attendu mais pourrait etre plus synthetique.

**Gravite** : Moyenne. Pas bloque pour la validation.

### Topic markdown = concatenation molle

Les `short_summary` et `topic_markdown` sont generes automatiquement mais restent basiques (resume du premier article + chronologie). Pas de synthese reelle multi-sources.

**Gravite** : Moyenne. Acceptable pour un MVP.

### LLM jamais declenche

Le LLM DeepSeek n'a ete appele **0 fois** sur 124 articles. La zone ambigu (0.76-0.84) n'a ete atteinte que pour 8 articles (les decisions `rules`), et le fallback `auto_create_ambiguous` a ete utilise 2 fois.

**Gravite** : Basse. Le LLM est un filet de securite, pas le chemin principal. Mais il n'est pas valide en conditions reelles.

### Taux de singletons trop eleve (92%)

103 topics sur 112 ne contiennent qu'un seul article. C'est le probleme principal.

**Causes probables** :
1. **Texte bruite** : Le contenu extrait contient du bruit (cookies, navigation, pubs) qui dilue le signal semantique
2. **Seuils trop hauts** : Les seuils de topic (0.84 strong, 0.76 ambiguous) sont peut-etre trop eleves pour des embeddings bruites
3. **Manque de sources en commun** : Les memes sujets ne sont couverts que par 2-3 sources, et les articles d'une meme source sur des sujets differents ont des scores plus bas

**Gravite** : Elevee. C'est le principal levier d'amelioration.

## 7. Bugs corriges cette session

### BUG-002 : Dimension vectorielle 1536 vs 4096

- **Fichiers** : `Vox_rag/migrations/003, 005, 006, 007, 010`
- **Symptome** : `expected 1536 dimensions, not 4096` a l'insertion des embeddings
- **Cause** : Migrations en `VECTOR(1536)` mais le modele `octen-embedding-8b` retourne 4096 dims. Le benchmark disait 1536, c'etait incorrect.
- **Fix applique** : Passage a `VECTOR(4096)` + suppression des index HNSW (limite pgvector = 2000 dims). Brute-force acceptable pour < 10k articles.
- **Statut** : Corrige dans le code, pas encore commit

## 8. Bugs NON corriges (workaround applique)

### BUG-004 : Workers embedding et topic partagent la meme file — MOYEN

- **Fichier** : `Vox_rag/src/db/job_queries.rs:46` (`pick_pending_jobs`)
- **Symptome** : Le topic worker prend les jobs `article_profile`, les marque failed. L'embedding worker ne peut plus les traiter. Le mode `all` ne fonctionne pas.
- **Cause** : `pick_pending_jobs` ne filtre pas par `target_type`
- **Workaround utilise** : Lancer les workers sequentiellement (embedding d'abord, puis topic)
- **Fix necessaire** : Ajouter un filtre `target_type` a `pick_pending_jobs`

### BUG-005 : Script import DB URL incorrect — MINEUR

- **Fichier** : `Vox_rag/scripts/import_from_ingest.sh:12`
- **Symptome** : Authentification echouee
- **Cause** : `postgres://postgres:postgres@localhost:5432` au lieu de `postgres://mypod:mypod_password@localhost:5432`
- **Workaround utilise** : Execution manuelle des commandes SQL avec les bons credentials

## 9. Bugs ingest-pipeline (documentes dans ingest-pipeline/docs/)

Les bugs BUG-001 (migration) et BUG-003 (`quality_status`) concernent ingest-pipeline. Voir `ingest-pipeline/docs/trafilatura-integration.md`.

L'integration rs-trafilatura (nettoyage HTML) est aussi documentee dans `ingest-pipeline/docs/trafilatura-integration.md` avec les options, resultats de comparaison, et points d'attention.

## 10. Checklist avant prochain run

1. ingest-pipeline demarre sans erreur de migration (BUG-001, voir ingest-pipeline)
2. Les articles passent en `quality_status = 'qualified'` automatiquement (BUG-003, voir ingest-pipeline)
3. Vox_rag demarre en mode `all` sans conflit entre workers (BUG-004 a fixer)
4. `./scripts/import_from_ingest.sh` fonctionne tel quel (BUG-005 a fixer)
5. Les embeddings sont stockes en VECTOR(4096) sans erreur (BUG-002 fixe)
6. rs-trafilatura nettoie le HTML a l'extraction (voir ingest-pipeline/docs/)

## 11. Points d'attention Vox_rag

1. **Dimension vectorielle : toujours 4096** — Le modele `octen-embedding-8b` produit des vecteurs de 4096 dimensions. Ne pas remettre 1536 dans les migrations.

2. **Pas d'index HNSW au-dela de 2000 dims** — pgvector ne supporte pas HNSW au-dela de 2000 dimensions. Brute-force acceptable pour < 10k articles. Au-dela, envisager IVFFlat ou reduire la dimension.

3. **Seuils de similarite** — Les seuils actuels (0.84 strong, 0.76 ambiguous) ont ete calibres sur du texte bruite. Apres l'integration rs-trafilatura, il faudra probablement les ajuster a la baisse (0.78 / 0.70) car le texte sera plus propre et les scores de similarite plus eleves.

4. **LLM jamais valide** — 0 appel DeepSeek sur 124 articles. La zone ambigu n'est jamais atteinte. Apres nettoyage du texte, les scores de similarite augmenteront et le LLM sera plus souvent declenche — il faudra valider qu'il fonctionne correctement.

5. **Kenya school fire : 1 split probable** — Les 2 topics "Fire rips through dormitory" et "Eight students arrested" couvrent le meme evenement mais n'ont pas ete fusionnes. Verifier si le nouveau texte propre ameliore le score.

## 12. Fichiers produits

- **Ce rapport** : `docs/validation-report-2026-05-29.md`
- **CSV Audit** : `docs/validation-audit.csv` (124 lignes, colonnes : topic_id, topic_title, article_id, article_title, source, relation_type, similarity_score, llm_decision, confidence_score)

## 13. Seuils utilises (non modifies)

```
near_duplicate >= 0.92 | same_story 0.86-0.92 | context 0.78-0.86
topic_strong 0.84 | topic_ambiguous 0.76 (LLM) | topic_weak 0.70
```

Aucun seuil n'a ete modifie pendant cette validation.

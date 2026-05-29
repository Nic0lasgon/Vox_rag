-- Import des articles qualifiés depuis ingest-pipeline vers Vox_rag
--
-- Usage:
--   1. Lancer ingest-pipeline: cd ../ingest-pipeline && docker compose up -d
--   2. Lancer Vox_rag: cd ../Vox_rag && docker compose up -d
--   3. Attendre que ingest-pipeline ait ingéré des articles
--   4. Exécuter: psql -h localhost -p 5433 -U voxrag -d vox_rag -f scripts/import_from_ingest.sql
--
-- Ce script se connecte à la DB ingest-pipeline (port 5432) via dblink
-- et copie les articles qualifiés dans Vox_rag (port 5433).

-- Installer l'extension dblink si pas déjà fait
CREATE EXTENSION IF NOT EXISTS dblink;

-- Insérer les articles qualifiés depuis ingest-pipeline
INSERT INTO articles (
    id, source_name, source_url, canonical_url, title, description,
    author, published_at, cleaned_text, content_hash, status,
    created_at, updated_at
)
SELECT
    ra.id,
    fs.name AS source_name,
    ra.url AS source_url,
    ra.canonical_url,
    ra.title,
    ra.description,
    ra.author,
    ra.pub_date AS published_at,
    ra.content AS cleaned_text,
    ra.content_hash,
    'active' AS status,
    ra.created_at,
    ra.updated_at
FROM dblink(
    'host=localhost port=5432 dbname=mypod_pipeline user=postgres password=postgres',
    'SELECT id, source_id, url, canonical_url, title, description, author, pub_date, content, content_hash, created_at, updated_at
     FROM raw_articles
     WHERE quality_status = ''qualified''
       AND processing_status = ''pending_qualification''
       AND content IS NOT NULL
       AND length(content) > 300'
) AS ra(
    id UUID,
    source_id TEXT,
    url TEXT,
    canonical_url TEXT,
    title TEXT,
    description TEXT,
    author TEXT,
    pub_date TIMESTAMPTZ,
    content TEXT,
    content_hash TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)
JOIN dblink(
    'host=localhost port=5432 dbname=mypod_pipeline user=postgres password=postgres',
    'SELECT id, name FROM feed_sources'
) AS fs(id TEXT, name TEXT)
ON ra.source_id = fs.id
ON CONFLICT (id) DO UPDATE SET
    cleaned_text = EXCLUDED.cleaned_text,
    content_hash = EXCLUDED.content_hash,
    updated_at = NOW();

-- Afficher le résultat
SELECT
    'Import terminé' AS status,
    COUNT(*) AS articles_importés
FROM articles
WHERE status = 'active';

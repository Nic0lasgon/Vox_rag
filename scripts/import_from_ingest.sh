#!/bin/bash
# Import des articles qualifiés depuis ingest-pipeline vers Vox_rag
#
# Usage:
#   1. Lancer ingest-pipeline: cd ../ingest-pipeline && docker compose up -d
#   2. Lancer Vox_rag: cd ../Vox_rag && docker compose up -d
#   3. Attendre que ingest-pipeline ait ingéré des articles
#   4. Exécuter: ./scripts/import_from_ingest.sh

set -e

INGEST_DB="${INGEST_DB:-postgres://postgres:postgres@localhost:5432/mypod_pipeline}"
VOXRAG_DB="postgres://voxrag:voxrag_dev@localhost:5433/vox_rag"

echo "=== Import des articles qualifiés depuis ingest-pipeline ==="

# Vérifier que les deux bases sont accessibles
echo "1. Vérification des connexions..."
psql "$INGEST_DB" -c "SELECT 1" > /dev/null 2>&1 || { echo "ERREUR: ingest-pipeline DB inaccessible (port 5432)"; exit 1; }
psql "$VOXRAG_DB" -c "SELECT 1" > /dev/null 2>&1 || { echo "ERREUR: Vox_rag DB inaccessible (port 5433)"; exit 1; }

# Compter les articles qualifiés dans ingest-pipeline
echo "2. Articles qualifiés dans ingest-pipeline:"
psql "$INGEST_DB" -t -c "
    SELECT COUNT(*)
    FROM raw_articles
    WHERE quality_status = 'qualified'
      AND content IS NOT NULL
      AND length(content) > 300
"

# Exporter les articles qualifiés vers un CSV temporaire
echo "3. Export des articles qualifiés..."
psql "$INGEST_DB" -t -A -F',' -c "
    SELECT
        ra.id,
        fs.name,
        ra.url,
        COALESCE(ra.canonical_url, ra.url),
        ra.title,
        COALESCE(ra.description, ''),
        COALESCE(ra.author, ''),
        ra.pub_date,
        ra.content,
        COALESCE(ra.content_hash, '')
    FROM raw_articles ra
    JOIN feed_sources fs ON ra.source_id = fs.id
    WHERE ra.quality_status = 'qualified'
      AND ra.content IS NOT NULL
      AND length(ra.content) > 300
    ORDER BY ra.pub_date DESC
    LIMIT 500
" > /tmp/voxrag_import.csv

echo "   $(wc -l < /tmp/voxrag_import.csv) articles exportés"

# Insérer dans Vox_rag
echo "4. Import dans Vox_rag..."
psql "$VOXRAG_DB" -c "
    CREATE TEMP TABLE import_staging (
        id UUID,
        source_name TEXT,
        source_url TEXT,
        canonical_url TEXT,
        title TEXT,
        description TEXT,
        author TEXT,
        published_at TIMESTAMPTZ,
        cleaned_text TEXT,
        content_hash TEXT
    );
"

psql "$VOXRAG_DB" -c "\COPY import_staging FROM '/tmp/voxrag_import.csv' WITH (FORMAT csv)"

psql "$VOXRAG_DB" -c "
    INSERT INTO articles (id, source_name, source_url, canonical_url, title, description, author, published_at, cleaned_text, content_hash, status, created_at, updated_at)
    SELECT id, source_name, source_url, canonical_url, title, description, author, published_at, cleaned_text, content_hash, 'active', NOW(), NOW()
    FROM import_staging
    ON CONFLICT (id) DO UPDATE SET
        cleaned_text = EXCLUDED.cleaned_text,
        content_hash = EXCLUDED.content_hash,
        updated_at = NOW();
"

# Résultat
echo "5. Résultat:"
psql "$VOXRAG_DB" -t -c "
    SELECT
        source_name,
        COUNT(*) AS articles
    FROM articles
    WHERE status = 'active'
    GROUP BY source_name
    ORDER BY articles DESC
"

TOTAL=$(psql "$VOXRAG_DB" -t -c "SELECT COUNT(*) FROM articles WHERE status = 'active'")
echo ""
echo "=== Import terminé: $TOTAL articles dans Vox_rag ==="
echo ""
echo "Prochaines étapes:"
echo "  - Embedding: curl -X POST http://localhost:3001/internal/embeddings/article-profile/{id}"
echo "  - Topic batch: curl -X POST http://localhost:3001/internal/jobs/process-article-batch -H 'Content-Type: application/json' -d '{\"article_ids\":[\"{id}\"]}'"

# Nettoyer
rm -f /tmp/voxrag_import.csv

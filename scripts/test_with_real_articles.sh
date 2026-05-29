#!/bin/bash
# Test complet du pipeline Vox_rag avec de vrais articles
#
# Ce script:
# 1. Lance les deux bases de données
# 2. Lance ingest-pipeline pour ingérer des articles RSS
# 3. Importe les articles qualifiés dans Vox_rag
# 4. Lance Vox_rag (API + workers)
# 5. Crée des jobs d'embedding et de topic processing

set -e

echo "=== Test complet Vox_rag avec vrais articles ==="
echo ""

# 1. Lancer les bases de données
echo "1. Lancement des bases de données..."
cd /Users/nicolasgonthier/Travail/Voxpod/ingest-pipeline
docker compose up -d postgres
cd /Users/nicolasgonthier/Travail/Voxpod/Vox_rag
docker compose up -d postgres

echo "   Attente que PostgreSQL soit prêt..."
sleep 3

# 2. Lancer ingest-pipeline pour ingérer des articles
echo "2. Lancement de ingest-pipeline (ingestion RSS)..."
cd /Users/nicolasgonthier/Travail/Voxpod/ingest-pipeline
docker compose up -d

echo "   Attente de l'ingestion (30 secondes)..."
sleep 30

# Vérifier combien d'articles ont été ingérés
INGEST_DB="postgres://postgres:postgres@localhost:5432/mypod_pipeline"
ARTICLE_COUNT=$(psql "$INGEST_DB" -t -c "
    SELECT COUNT(*)
    FROM raw_articles
    WHERE quality_status = 'qualified'
      AND content IS NOT NULL
      AND length(content) > 300
" | tr -d ' ')

echo "   $ARTICLE_COUNT articles qualifiés dans ingest-pipeline"

if [ "$ARTICLE_COUNT" -eq 0 ]; then
    echo "   Aucun article qualifié. Attente supplémentaire..."
    sleep 30
    ARTICLE_COUNT=$(psql "$INGEST_DB" -t -c "
        SELECT COUNT(*)
        FROM raw_articles
        WHERE quality_status = 'qualified'
          AND content IS NOT NULL
          AND length(content) > 300
    " | tr -d ' ')
    echo "   $ARTICLE_COUNT articles qualifiés"
fi

# 3. Importer les articles dans Vox_rag
echo "3. Import des articles dans Vox_rag..."
cd /Users/nicolasgonthier/Travail/Voxpod/Vox_rag
./scripts/import_from_ingest.sh

# 4. Lancer Vox_rag
echo ""
echo "4. Lancement de Vox_rag..."
docker compose up -d

echo "   Attente que Vox_rag soit prêt..."
sleep 5

# Vérifier le health check
HEALTH=$(curl -s http://localhost:3001/health 2>/dev/null || echo "not ready")
if [ "$HEALTH" = '{"status":"ok"}' ]; then
    echo "   Vox_rag API: OK"
else
    echo "   Vox_rag API: $HEALTH"
fi

# 5. Créer des jobs d'embedding pour tous les articles
echo "5. Création des jobs d'embedding..."
VOXRAG_DB="postgres://voxrag:voxrag_dev@localhost:5433/vox_rag"

# Récupérer les IDs des articles sans embedding
ARTICLE_IDS=$(psql "$VOXRAG_DB" -t -A -c "
    SELECT id
    FROM articles
    WHERE status = 'active'
      AND id NOT IN (SELECT article_id FROM article_profiles)
    LIMIT 10
")

if [ -n "$ARTICLE_IDS" ]; then
    # Construire le JSON array
    IDS_JSON=$(echo "$ARTICLE_IDS" | jq -R . | jq -s .)

    echo "   Création de jobs pour $(echo "$ARTICLE_IDS" | wc -l | tr -d ' ') articles..."

    # Créer les jobs via l'API
    curl -s -X POST http://localhost:3001/internal/jobs/process-article-batch \
        -H "Content-Type: application/json" \
        -d "{\"article_ids\": $IDS_JSON}" | jq .

    echo "   Jobs créés. Les workers vont les traiter..."
else
    echo "   Tous les articles ont déjà des embeddings."
fi

echo ""
echo "=== Test terminé ==="
echo ""
echo "Commandes utiles:"
echo "  - Voir les logs: docker compose logs -f"
echo "  - Health check: curl http://localhost:3001/health"
echo "  - Articles: psql $VOXRAG_DB -c 'SELECT id, source_name, title FROM articles LIMIT 10'"
echo "  - Embeddings: psql $VOXRAG_DB -c 'SELECT article_id, embedding_model FROM article_profiles'"
echo "  - Topics: psql $VOXRAG_DB -c 'SELECT id, title, status FROM topics'"
echo "  - Similar articles: curl http://localhost:3001/internal/articles/{id}/similar"

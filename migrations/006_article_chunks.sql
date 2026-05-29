CREATE TABLE article_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    token_count INTEGER,
    section_title TEXT,
    embedding_model TEXT,
    embedding_dimension INTEGER,
    embedding VECTOR(1536),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(article_id, chunk_index)
);

CREATE INDEX idx_article_chunks_article_id ON article_chunks (article_id);
CREATE INDEX idx_article_chunks_embedding_hnsw
    ON article_chunks USING hnsw (embedding vector_cosine_ops);

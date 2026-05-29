CREATE INDEX idx_article_profiles_embedding_hnsw
ON article_profiles
USING hnsw (embedding vector_cosine_ops);

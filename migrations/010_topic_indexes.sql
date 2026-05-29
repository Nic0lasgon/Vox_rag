-- Standard indexes
CREATE INDEX idx_topics_status ON topics(status);
CREATE INDEX idx_topics_last_seen_at ON topics(last_seen_at DESC);
CREATE INDEX idx_topics_category ON topics(category);
CREATE INDEX idx_topics_stable_slug ON topics(stable_slug);
CREATE INDEX idx_topic_articles_topic_id ON topic_articles(topic_id);
CREATE INDEX idx_topic_articles_article_id ON topic_articles(article_id);
CREATE INDEX idx_topic_articles_relation_type ON topic_articles(relation_type);
CREATE INDEX idx_topic_timeline_topic_id ON topic_timeline_events(topic_id);
CREATE INDEX idx_topic_timeline_event_date ON topic_timeline_events(event_date DESC);

-- Vector index for topic embeddings
CREATE INDEX idx_topics_embedding_hnsw
ON topics
USING hnsw (topic_embedding vector_cosine_ops);

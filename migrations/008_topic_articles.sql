CREATE TABLE topic_articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    topic_id UUID NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    confidence_score FLOAT8 NOT NULL DEFAULT 0,
    similarity_score FLOAT8,
    decision_method TEXT,
    is_primary BOOLEAN DEFAULT false,
    is_timeline_source BOOLEAN DEFAULT false,
    added_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(topic_id, article_id)
);

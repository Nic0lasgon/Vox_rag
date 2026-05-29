CREATE TABLE topic_timeline_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    topic_id UUID NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    event_date TIMESTAMPTZ NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    importance TEXT DEFAULT 'medium',
    event_type TEXT,
    source_article_ids UUID[] DEFAULT '{}',
    confidence_score FLOAT8 DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

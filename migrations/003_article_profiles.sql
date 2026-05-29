CREATE TABLE article_profiles (
    article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    profile_text TEXT NOT NULL,
    short_summary TEXT,
    main_event TEXT,
    entities JSONB DEFAULT '[]'::jsonb,
    keywords JSONB DEFAULT '[]'::jsonb,
    embedding_model TEXT,
    embedding_dimension INTEGER,
    embedding VECTOR(4096),
    embedded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

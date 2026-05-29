CREATE TABLE articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_name TEXT NOT NULL,
    source_url TEXT,
    canonical_url TEXT,
    title TEXT NOT NULL,
    description TEXT,
    author TEXT,
    language TEXT,
    country TEXT,
    published_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ DEFAULT NOW(),
    raw_text TEXT,
    cleaned_text TEXT,
    title_hash TEXT,
    content_hash TEXT,
    simhash TEXT,
    status TEXT DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_articles_published_at ON articles (published_at DESC);
CREATE INDEX idx_articles_source_name ON articles (source_name);
CREATE INDEX idx_articles_language ON articles (language);
CREATE INDEX idx_articles_title_hash ON articles (title_hash);
CREATE INDEX idx_articles_content_hash ON articles (content_hash);
CREATE INDEX idx_articles_canonical_url ON articles (canonical_url);

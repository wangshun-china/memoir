-- Non-linear collection: extract traceable story cards from short interviews.

CREATE TABLE stories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memoir_id UUID NOT NULL REFERENCES memoirs(id) ON DELETE CASCADE,
    session_id UUID UNIQUE REFERENCES interview_sessions(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    narrative TEXT NOT NULL,
    life_stage TEXT,
    time_text TEXT,
    year_start INT,
    year_end INT,
    time_precision TEXT NOT NULL DEFAULT 'unknown',
    location_text TEXT,
    people JSONB NOT NULL DEFAULT '[]'::jsonb,
    themes JSONB NOT NULL DEFAULT '[]'::jsonb,
    emotions JSONB NOT NULL DEFAULT '[]'::jsonb,
    missing_details JSONB NOT NULL DEFAULT '[]'::jsonb,
    primary_chapter_id UUID REFERENCES chapters(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'confirmed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (year_end IS NULL OR year_start IS NULL OR year_end >= year_start)
);

CREATE INDEX idx_stories_memoir_created ON stories (memoir_id, created_at DESC);
CREATE INDEX idx_stories_memoir_time ON stories (memoir_id, year_start, created_at);
CREATE INDEX idx_stories_primary_chapter ON stories (primary_chapter_id)
    WHERE primary_chapter_id IS NOT NULL;

CREATE TABLE story_sources (
    story_id UUID NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    message_id UUID NOT NULL REFERENCES interview_messages(id) ON DELETE CASCADE,
    PRIMARY KEY (story_id, message_id)
);

CREATE INDEX idx_story_sources_message ON story_sources (message_id);

CREATE TABLE story_chapter_relations (
    story_id UUID NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL DEFAULT 'primary'
        CHECK (relation_type IN ('primary', 'related')),
    relevance_score REAL NOT NULL DEFAULT 1.0,
    confirmed_by_user BOOLEAN NOT NULL DEFAULT FALSE,
    classification_reason TEXT,
    PRIMARY KEY (story_id, chapter_id)
);

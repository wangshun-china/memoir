-- Session generation lock / status for async + idempotent chapter generate.
ALTER TABLE interview_sessions
    ADD COLUMN IF NOT EXISTS generation_status TEXT NOT NULL DEFAULT 'idle';

-- idle | generating | ready | failed
COMMENT ON COLUMN interview_sessions.generation_status IS
  'idle=no job; generating=in progress; ready=last gen ok; failed=last gen failed';

-- Speed up list progress / resume queries.
CREATE INDEX IF NOT EXISTS idx_sessions_memoir_status_started
    ON interview_sessions (memoir_id, status, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_sessions_memoir_topic
    ON interview_sessions (memoir_id, topic);

CREATE INDEX IF NOT EXISTS idx_sessions_chapter
    ON interview_sessions (chapter_id)
    WHERE chapter_id IS NOT NULL;

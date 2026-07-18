-- Chapter draft body generated from interview transcripts.
ALTER TABLE chapters
    ADD COLUMN IF NOT EXISTS content TEXT;

-- Track whether auto-generation already ran for a session (avoid repeated auto-gen).
ALTER TABLE interview_sessions
    ADD COLUMN IF NOT EXISTS auto_generated_at TIMESTAMPTZ;

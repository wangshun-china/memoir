-- Password login for miniprogram (username + password; optional WeChat openid still unique).

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS username TEXT,
    ADD COLUMN IF NOT EXISTS password_hash TEXT,
    ADD COLUMN IF NOT EXISTS is_admin BOOLEAN NOT NULL DEFAULT FALSE;

-- Unique username when set (multiple NULLs allowed for pure WeChat users).
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_unique
    ON users (username)
    WHERE username IS NOT NULL AND username <> '';

CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users (is_admin) WHERE is_admin = TRUE;

-- User profile fields for WeChat miniprogram "我的" page

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS avatar_url TEXT;

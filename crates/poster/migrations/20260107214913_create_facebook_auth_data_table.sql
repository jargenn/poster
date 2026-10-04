CREATE TABLE facebook_auth_data (
    user_id TEXT PRIMARY KEY
        REFERENCES users(user_id) ON DELETE CASCADE,
    fb_user_access_token TEXT NOT NULL,
    fb_app_id TEXT NOT NULL,
    fb_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    expires_at TEXT,
    last_verified_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    
    CONSTRAINT expires_after_created 
        CHECK (expires_at IS NULL OR CAST(strftime('%s', expires_at) AS INTEGER) > CAST(strftime('%s', created_at) AS INTEGER)),
    CONSTRAINT last_verified_reasonable 
        CHECK (CAST(strftime('%s', last_verified_at) AS INTEGER) >= CAST(strftime('%s', created_at) AS INTEGER))
);

CREATE INDEX IF NOT EXISTS idx_facebook_auth_data_user_id 
    ON facebook_auth_data(fb_user_id);

CREATE INDEX IF NOT EXISTS idx_facebook_auth_data_expires_at 
    ON facebook_auth_data(expires_at) 
    WHERE expires_at IS NOT NULL;

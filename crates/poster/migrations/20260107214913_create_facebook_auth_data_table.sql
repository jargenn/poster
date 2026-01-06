CREATE TABLE facebook_auth_data (
    user_id uuid PRIMARY KEY
        REFERENCES users(user_id) ON DELETE CASCADE,
    fb_user_access_token TEXT NOT NULL,
    fb_app_id TEXT NOT NULL,
    fb_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    last_verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT expires_after_created 
        CHECK (expires_at IS NULL OR expires_at > created_at),
    CONSTRAINT last_verified_reasonable 
        CHECK (last_verified_at >= created_at)
);

CREATE INDEX IF NOT EXISTS idx_facebook_auth_data_user_id 
    ON facebook_auth_data(fb_user_id);

CREATE INDEX IF NOT EXISTS idx_facebook_auth_data_expires_at 
    ON facebook_auth_data(expires_at) 
    WHERE expires_at IS NOT NULL;

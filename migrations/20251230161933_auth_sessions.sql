CREATE TABLE IF NOT EXISTS auth_sessions (
    session_id TEXT PRIMARY KEY,
    user_access_token TEXT NOT NULL,
    app_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    last_verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT expires_after_created 
        CHECK (expires_at IS NULL OR expires_at > created_at),
    CONSTRAINT last_verified_reasonable 
        CHECK (last_verified_at >= created_at),
    CONSTRAINT session_id_format 
        CHECK (length(session_id) > 0)
);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id 
    ON auth_sessions(user_id);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at 
    ON auth_sessions(expires_at) 
    WHERE expires_at IS NOT NULL;

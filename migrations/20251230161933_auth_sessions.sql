-- Add migration script here

CREATE TABLE if not exists auth_sessions (
    session_id TEXT PRIMARY KEY,
    user_access_token TEXT NOT NULL,
    app_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER,         -- unix timestamp (nullable)
    last_verified_at INTEGER NOT NULL   -- unix timestamp
);

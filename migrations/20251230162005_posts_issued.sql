-- Add migration script here
CREATE TABLE if not exists posts_issued (
    post_id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    checked_at TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'published', 'failed')),
    page_id TEXT NOT NULL,
    check_attempts INTEGER NOT NULL DEFAULT 0
);


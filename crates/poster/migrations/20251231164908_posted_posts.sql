CREATE TABLE published_posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    post_data_id INT NOT NULL REFERENCES post_data(id) ON DELETE CASCADE,
    scheduled_post_id INT NOT NULL REFERENCES scheduled_posts(id) ON DELETE CASCADE,
    facebook_post_id TEXT NOT NULL UNIQUE,
    posted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    
    CONSTRAINT posted_at_reasonable
        CHECK (CAST(strftime('%s', posted_at) AS INTEGER) >= CAST(strftime('%s', created_at) AS INTEGER))
);

CREATE UNIQUE INDEX idx_posted_post_data ON published_posts(post_data_id);
CREATE UNIQUE INDEX idx_posted_scheduled ON published_posts(scheduled_post_id);
CREATE INDEX idx_posted_at ON published_posts(posted_at DESC);

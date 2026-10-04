CREATE TABLE scheduled_posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    post_data_id INT NOT NULL REFERENCES post_data(id) ON DELETE CASCADE,
    scheduled_for TEXT NOT NULL,
    schedule_mode TEXT NOT NULL CHECK (schedule_mode IN ('immediate', 'scheduled')),
    status TEXT NOT NULL DEFAULT 'processing' CHECK (status IN ('pending', 'processing', 'failed', 'cancelled')),
    attempts INT NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    
    CONSTRAINT attempts_reasonable 
        CHECK (attempts >= 0 AND attempts <= 10),
    
    CONSTRAINT facebook_scheduling_window
    CHECK (
        schedule_mode != 'scheduled'
        OR (
            CAST(strftime('%s', scheduled_for) AS INTEGER) >= CAST(strftime('%s', created_at, '+10 minutes') AS INTEGER)
            AND CAST(strftime('%s', scheduled_for) AS INTEGER) <= CAST(strftime('%s', created_at, '+30 days') AS INTEGER)
        )
    ),

    CONSTRAINT failed_requires_error 
        CHECK (status != 'failed' OR last_error IS NOT NULL),
    
    CONSTRAINT cancelled_finality
        CHECK (status != 'cancelled' OR attempts >= 0),
        
    CONSTRAINT scheduled_after_creation
        CHECK (CAST(strftime('%s', scheduled_for) AS INTEGER) >= CAST(strftime('%s', created_at) AS INTEGER))
);

CREATE INDEX idx_scheduled_post_data ON scheduled_posts(post_data_id);

CREATE INDEX idx_scheduled_status_time ON scheduled_posts(status, scheduled_for) 
    WHERE status IN ('pending', 'processing');

CREATE INDEX idx_scheduled_created ON scheduled_posts(created_at DESC);

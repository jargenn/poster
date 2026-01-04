CREATE TYPE scheduled_post_status AS ENUM (
    'pending',  
    'processing',
    'failed',      
    'cancelled'     
);

CREATE TYPE post_schedule_mode AS ENUM (
    'immediate',
    'scheduled'
);

CREATE TABLE scheduled_posts (
    id SERIAL PRIMARY KEY,
    post_data_id INT NOT NULL REFERENCES post_data(id) ON DELETE CASCADE,
    scheduled_for TIMESTAMPTZ NOT NULL,
    schedule_mode post_schedule_mode NOT NULL,
    status scheduled_post_status NOT NULL DEFAULT 'pending',
    attempts INT NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT attempts_reasonable 
        CHECK (attempts >= 0 AND attempts <= 10),
    
    CONSTRAINT facebook_scheduling_window
    CHECK (
        schedule_mode != 'scheduled'
        OR (
            scheduled_for >= created_at + INTERVAL '10 minutes'
            AND scheduled_for <= created_at + INTERVAL '30 days'
        )
    ),

    CONSTRAINT failed_requires_error 
        CHECK (status != 'failed' OR last_error IS NOT NULL),
    
    CONSTRAINT processing_has_attempt 
        CHECK (status != 'processing' OR last_attempted_at IS NOT NULL),
    
    CONSTRAINT cancelled_finality
        CHECK (status != 'cancelled' OR attempts >= 0),
        
    CONSTRAINT scheduled_after_creation
        CHECK (scheduled_for >= created_at)
);

CREATE INDEX idx_scheduled_post_data ON scheduled_posts(post_data_id);

CREATE INDEX idx_scheduled_status_time ON scheduled_posts(status, scheduled_for) 
    WHERE status IN ('pending', 'processing');

CREATE INDEX idx_scheduled_created ON scheduled_posts(created_at DESC);

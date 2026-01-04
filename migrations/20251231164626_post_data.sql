CREATE TABLE post_data (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    page_id TEXT NOT NULL,
    content TEXT NOT NULL,
    link TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    has_media BOOLEAN NOT NULL DEFAULT FALSE,
    
    CONSTRAINT content_not_empty 
        CHECK (LENGTH(TRIM(content)) > 0)
);

CREATE INDEX idx_post_data_user ON post_data(user_id, created_at DESC);
CREATE INDEX idx_post_data_page ON post_data(page_id, created_at DESC);
CREATE INDEX idx_post_data_user_page ON post_data(user_id, page_id, created_at DESC);

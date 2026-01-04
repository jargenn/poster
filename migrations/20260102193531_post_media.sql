CREATE TABLE post_media (
    id SERIAL PRIMARY KEY,
    post_data_id INT NOT NULL
        REFERENCES post_data(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL
        CHECK (media_type IN ('image', 'video')),
    storage_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INT NOT NULL,
    width INT,
    height INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT storage_key_not_empty
        CHECK (LENGTH(storage_key) > 0),

    CONSTRAINT allowed_image_types
        CHECK (
            media_type != 'image'
            OR content_type IN (
                'image/jpeg',
                'image/png',
                'image/webp'
        )),

    CONSTRAINT image_size_reasonable
        CHECK (
            media_type != 'image'
            OR size_bytes <= 10 * 1024 * 1024 
        ),
        
    CONSTRAINT image_dimensions_reasonable
        CHECK (
            media_type != 'image'
            OR (
                width IS NOT NULL
                AND height IS NOT NULL
                AND width <= 4096
                AND height <= 4096
        )
    )
);

CREATE INDEX idx_post_media_post
    ON post_media(post_data_id);

CREATE INDEX idx_post_media_type
    ON post_media(media_type);

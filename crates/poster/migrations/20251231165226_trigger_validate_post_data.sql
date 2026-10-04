CREATE TRIGGER trg_validate_post_data
    BEFORE INSERT ON published_posts
    FOR EACH ROW
    WHEN NOT EXISTS (
        SELECT 1 FROM scheduled_posts
        WHERE id = NEW.scheduled_post_id AND post_data_id = NEW.post_data_id
    )
    BEGIN
        SELECT RAISE(ABORT, 'post_data_id mismatch or scheduled post does not exist');
    END;

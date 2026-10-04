CREATE TRIGGER trg_delete_scheduled_post_after_publish
    AFTER INSERT ON published_posts
    FOR EACH ROW
    BEGIN
        DELETE FROM scheduled_posts WHERE id = NEW.scheduled_post_id;
    END;

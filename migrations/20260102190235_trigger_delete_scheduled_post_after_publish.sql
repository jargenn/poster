CREATE OR REPLACE FUNCTION delete_scheduled_post_after_publish()
RETURNS TRIGGER AS $$
BEGIN
    DELETE FROM scheduled_posts
    WHERE id = NEW.scheduled_post_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'scheduled_post % not found for cleanup', NEW.scheduled_post_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_delete_scheduled_post_after_publish
    AFTER INSERT ON published_posts
    FOR EACH ROW
    EXECUTE FUNCTION delete_scheduled_post_after_publish();


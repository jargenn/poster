CREATE OR REPLACE FUNCTION update_scheduled_status_on_post()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE scheduled_posts 
    SET status = 'published'
    WHERE id = NEW.scheduled_post_id 
        AND status != 'published';
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'scheduled_post % not found or already published', NEW.scheduled_post_id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_update_scheduled_status
    AFTER INSERT ON published_posts
    FOR EACH ROW
    EXECUTE FUNCTION update_scheduled_status_on_post();

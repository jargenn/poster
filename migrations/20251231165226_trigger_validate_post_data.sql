CREATE OR REPLACE FUNCTION validate_post_data_consistency()
RETURNS TRIGGER AS $$
DECLARE
    v_scheduled_post_data_id INT;
BEGIN
    SELECT post_data_id INTO v_scheduled_post_data_id
    FROM scheduled_posts
    WHERE id = NEW.scheduled_post_id;
    
    IF v_scheduled_post_data_id IS NULL THEN
        RAISE EXCEPTION 'scheduled_post % does not exist', NEW.scheduled_post_id;
    END IF;
    
    IF v_scheduled_post_data_id != NEW.post_data_id THEN
        RAISE EXCEPTION 'post_data_id mismatch: scheduled_post references %, but posted_post references %',
            v_scheduled_post_data_id, NEW.post_data_id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_validate_post_data
    BEFORE INSERT ON published_posts
    FOR EACH ROW
    EXECUTE FUNCTION validate_post_data_consistency();

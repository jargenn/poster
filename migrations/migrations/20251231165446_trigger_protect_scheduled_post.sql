CREATE OR REPLACE FUNCTION protect_scheduled_post_data()
RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM scheduled_posts WHERE post_data_id = OLD.id) THEN
        RAISE EXCEPTION 'Cannot modify post_data % that has been scheduled', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_protect_post_data
    BEFORE UPDATE OR DELETE ON post_data
    FOR EACH ROW
    EXECUTE FUNCTION protect_scheduled_post_data();

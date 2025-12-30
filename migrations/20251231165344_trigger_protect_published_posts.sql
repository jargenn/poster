CREATE OR REPLACE FUNCTION protect_published_posts()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status = 'published' THEN
        IF TG_OP = 'DELETE' THEN
            IF EXISTS (SELECT 1 FROM published_posts WHERE scheduled_post_id = OLD.id) THEN
                RAISE EXCEPTION 'Cannot delete published scheduled_post % with posted record', OLD.id;
            END IF;
        ELSIF TG_OP = 'UPDATE' THEN
            IF NEW.status != OLD.status OR 
               NEW.post_data_id != OLD.post_data_id OR
               NEW.scheduled_for != OLD.scheduled_for THEN
                RAISE EXCEPTION 'Cannot modify core fields of published scheduled_post %', OLD.id;
            END IF;
        END IF;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_protect_published
    BEFORE UPDATE OR DELETE ON scheduled_posts
    FOR EACH ROW
    EXECUTE FUNCTION protect_published_posts();

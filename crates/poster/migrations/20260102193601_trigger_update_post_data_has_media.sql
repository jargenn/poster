CREATE OR REPLACE FUNCTION update_post_data_has_media()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE post_data
    SET has_media = TRUE
    WHERE id = NEW.post_data_id;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_set_has_media
    AFTER INSERT ON post_media
    FOR EACH ROW
    EXECUTE FUNCTION update_post_data_has_media();

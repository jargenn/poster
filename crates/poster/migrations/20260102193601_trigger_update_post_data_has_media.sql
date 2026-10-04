CREATE TRIGGER trg_set_has_media
    AFTER INSERT ON post_media
    FOR EACH ROW
    BEGIN
        UPDATE post_data SET has_media = TRUE WHERE id = NEW.post_data_id;
    END;

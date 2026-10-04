CREATE TABLE user_configs (
    id UUID PRIMARY KEY
        REFERENCES users(user_id) ON DELETE CASCADE,
    app_id TEXT NOT NULL,
    app_secret TEXT NOT NULL,
    app_config_id TEXT NOT NULL,
    redirect_url TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TRIGGER update_user_configs_updated_at 
    AFTER UPDATE ON user_configs
    FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at
    BEGIN
        UPDATE user_configs SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = NEW.id;
    END;

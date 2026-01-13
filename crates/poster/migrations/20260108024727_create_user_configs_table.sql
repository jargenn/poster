CREATE TABLE user_configs (
    id UUID PRIMARY KEY
        REFERENCES users(user_id) ON DELETE CASCADE,
    app_id TEXT NOT NULL,
    app_secret TEXT NOT NULL,
    app_config_id TEXT NOT NULL,
    redirect_url TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_user_configs_updated_at 
    BEFORE UPDATE ON user_configs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

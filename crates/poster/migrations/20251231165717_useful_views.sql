CREATE VIEW v_scheduled_posts_complete AS
SELECT 
    sp.id AS scheduled_post_id,
    sp.status,
    sp.scheduled_for,
    sp.attempts,
    sp.last_error,
    sp.last_attempted_at,
    sp.created_at AS scheduled_at,
    pd.id AS post_data_id,
    pd.user_id,
    pd.page_id,
    pd.content,
    pd.created_at AS content_created_at,
    pp.facebook_post_id,
    pp.posted_at
FROM scheduled_posts sp
JOIN post_data pd ON sp.post_data_id = pd.id
LEFT JOIN published_posts pp ON pp.scheduled_post_id = sp.id;

CREATE VIEW v_posts_ready_to_process AS
SELECT 
    sp.id AS scheduled_post_id,
    sp.post_data_id,
    sp.scheduled_for,
    sp.attempts,
    pd.user_id,
    pd.page_id,
    pd.content
FROM scheduled_posts sp
JOIN post_data pd ON sp.post_data_id = pd.id
WHERE sp.status = 'pending'
    AND CAST(strftime('%s', sp.scheduled_for) AS INTEGER) <= CAST(strftime('%s', 'now', '+10 minutes') AS INTEGER)
ORDER BY sp.scheduled_for ASC;

CREATE VIEW v_user_posting_history AS
SELECT 
    pd.user_id,
    pd.page_id,
    pd.content,
    sp.scheduled_for,
    pp.posted_at,
    pp.facebook_post_id,
    sp.status
FROM post_data pd
JOIN scheduled_posts sp ON sp.post_data_id = pd.id
LEFT JOIN published_posts pp ON pp.post_data_id = pd.id
ORDER BY pd.user_id, COALESCE(pp.posted_at, sp.scheduled_for) DESC;

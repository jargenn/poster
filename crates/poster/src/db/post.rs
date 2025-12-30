use color_eyre::owo_colors::OwoColorize as _;
use eyre::Result;
use facebook_graph_api::FacebookPost;
use sqlx::{Acquire as _, PgConnection, Row};
use std::fmt::Display;
use time::OffsetDateTime;
use tracing::{info, instrument};

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "post_schedule_mode", rename_all = "lowercase")]
pub enum ScheduleMode {
    Immediate,
    Scheduled,
}

impl Display for ScheduleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let out = match self {
            ScheduleMode::Immediate => "immediate",
            ScheduleMode::Scheduled => "scheduled",
        };
        write!(f, "{out}")
    }
}

#[instrument("Scheduling post", skip(conn, fb_post))]
pub async fn schedule_post(
    conn: &mut PgConnection,
    user_id: &str,
    page_id: &str,
    fb_post: FacebookPost,
) -> Result<()> {
    tracing::info!(page_id=%page_id.bold(),"Scheduling post");

    let mut tx = conn.begin().await?;

    let post_data_id = sqlx::query_scalar!(
        "INSERT INTO post_data (
                user_id,
                page_id,
                content,
                link
            )
            VALUES ($1, $2, $3, $4)
            RETURNING id
        ",
        user_id,
        page_id,
        fb_post.message,
        fb_post.link,
    )
    .fetch_one(&mut *tx)
    .await?;

    let (schedule_mode, scheduled_time) = match fb_post.scheduled_publish_time {
        None => (ScheduleMode::Immediate, OffsetDateTime::now_utc()),
        Some(st) => (ScheduleMode::Scheduled, st.try_into()?),
    };

    sqlx::query!(
        "INSERT INTO scheduled_posts (
                post_data_id, 
                scheduled_for,
                schedule_mode
            )
            VALUES ($1, $2, $3)
        ",
        post_data_id,
        scheduled_time,
        schedule_mode as ScheduleMode,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    info!(%user_id,%page_id,%post_data_id,"scheduled post stored in the database");

    Ok(())
}

#[instrument(
    "Scheduling multiple posts",
    skip(conn, fb_posts),
    fields(user_id, page_id, num_posts)
)]
pub async fn schedule_multiple_posts(
    conn: &mut PgConnection,
    user_id: &str,
    page_id: &str,
    fb_posts: Vec<FacebookPost>,
) -> Result<()> {
    let num_posts = fb_posts.len();

    let mut contents = Vec::with_capacity(num_posts);
    let mut links = Vec::with_capacity(num_posts);
    let mut scheduled_for: Vec<Option<OffsetDateTime>> = Vec::with_capacity(num_posts);
    let mut schedule_modes = Vec::with_capacity(num_posts);

    for fb_post in fb_posts {
        let FacebookPost {
            message,
            link,
            scheduled_publish_time,
            ..
        } = fb_post;

        contents.push(message);
        links.push(link);

        match scheduled_publish_time {
            None => {
                schedule_modes.push(ScheduleMode::Immediate);
                scheduled_for.push(None);
            }
            Some(st) => {
                schedule_modes.push(ScheduleMode::Scheduled);
                scheduled_for.push(Some(st.try_into()?));
            }
        }
    }

    let mut tx = conn.begin().await?;

    let post_data_ids: Vec<i32> = sqlx::query(
        r#"
    INSERT INTO post_data (user_id, page_id, content, link)
    SELECT
        $1,
        $2,
        content,
        link
    FROM UNNEST($3::text[], $4::text[])
        AS t(content, link)
    RETURNING id
    "#,
    )
    .bind(user_id)
    .bind(page_id)
    .bind(&contents)
    .bind(&links)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| row.get::<i32, _>(0))
    .collect();

    sqlx::query(
        r#"
        INSERT INTO scheduled_posts (
            post_data_id,
            scheduled_for,
            schedule_mode
        )
        SELECT
            post_data_id,
            COALESCE(scheduled_for, NOW()),
            schedule_mode
        FROM UNNEST(
            $1::int4[],
            $2::timestamptz[],
            $3::post_schedule_mode[]
        )
        AS t(post_data_id, scheduled_for, schedule_mode)
        "#,
    )
    .bind(&post_data_ids)
    .bind(&scheduled_for)
    .bind(&schedule_modes)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        %user_id,
        %page_id,
        num_posts,
        "batched posts scheduled successfully"
    );

    Ok(())
}

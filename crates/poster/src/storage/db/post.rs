use color_eyre::owo_colors::OwoColorize as _;
use facebook_graph_api::{FacebookPost, page_api::post_to_page};
use reqwest::{Client, Url};
use sqlx::{Acquire as _, PgConnection, Row};
use std::{fmt::Display, iter::zip};
use time::OffsetDateTime;
use tracing::{Instrument, info, info_span, instrument};

use crate::{
    configuration::MediaSettings,
    error::{Error, PostSchedulingError},
    media::Pipeline,
    storage,
};

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

#[instrument("Scheduling post", skip(conn, fb_post, media_settings))]
pub async fn schedule_post(
    conn: &mut PgConnection,
    user_id: &str,
    user_access_token: &str,
    page_id: &str,
    fb_post: FacebookPost,
    media_settings: MediaSettings,
) -> Result<(), PostSchedulingError> {
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

    let http_client = Client::new();
    let process_settings = media_settings.process_settings;
    let storage = media_settings.storage_settings;

    for input in fb_post.media_url.unwrap_or_default() {
        let media = async {
            Pipeline::from_input(input, &http_client)
                .await?
                .decode()
                .await?
                .plan(&process_settings)?
                .process(post_data_id, user_id, page_id)
                .await
        }
        .instrument(tracing::info_span!("Media processing pipeline"))
        .await?;

        storage::save_post_media(&mut tx, media, &storage).await?
    }

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
    skip(conn, fb_posts, media_settings, user_id),
    fields(page_id, num_posts)
)]
pub async fn submit_posts(
    conn: &mut PgConnection,
    user_id: &str,
    page_id: &str,
    user_access_token: &str,
    fb_posts: Vec<FacebookPost>,
    media_settings: MediaSettings,
    facebook_uri: Url,
) -> Result<(), Error> {
    let num_posts = fb_posts.len();

    let mut contents = Vec::with_capacity(num_posts);
    let mut links = Vec::with_capacity(num_posts);
    let mut media_content = Vec::with_capacity(num_posts);
    let mut scheduled_for: Vec<Option<OffsetDateTime>> = Vec::with_capacity(num_posts);
    let mut schedule_modes = Vec::with_capacity(num_posts);

    for fb_post in &fb_posts {
        let FacebookPost {
            message,
            link,
            scheduled_publish_time,
            media_url,
            ..
        } = fb_post;

        contents.push(message);
        links.push(link);
        media_content.push(media_url);

        match scheduled_publish_time {
            None => {
                schedule_modes.push(ScheduleMode::Immediate);
                scheduled_for.push(None);
            }
            Some(st) => {
                schedule_modes.push(ScheduleMode::Scheduled);
                scheduled_for.push(Some(st.try_into().map_err(PostSchedulingError::from)?));
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

    assert_eq!(post_data_ids.len(), media_content.len());

    let http_client = Client::new();
    let process_settings = media_settings.process_settings;
    let storage = media_settings.storage_settings;

    for (post_data_id, found_inputs) in zip(post_data_ids, media_content) {
        // FIX: Avoid cloning
        let inputs = found_inputs.clone().unwrap_or_default();
        for input in inputs {
            let media = async {
                Pipeline::from_input(input, &http_client)
                    .await?
                    .decode()
                    .await?
                    .plan(&process_settings)?
                    .process(post_data_id, user_id, page_id)
                    .await
            }
            .instrument(tracing::info_span!("Media processing pipeline"))
            .await
            .map_err(PostSchedulingError::from)?;

            storage::save_post_media(&mut tx, media, &storage).await?
        }
    }

    tx.commit().await?;

    tracing::info!(
        %user_id,
        %page_id,
        num_posts,
        "posts data saved in the database"
    );

    tracing::info!("Starting the post workflow");
    // FIX: Delete this
    let client = reqwest::Client::new();
    for post in fb_posts {
        let post_id = post_to_page(
            &client,
            "24.0",
            post,
            page_id,
            user_access_token,
            user_id,
            facebook_uri.clone(),
        )
        .instrument(info_span!("Posting"))
        .await?;

        tracing::info!(post_id, "Post succesful!");
    }

    Ok(())
}

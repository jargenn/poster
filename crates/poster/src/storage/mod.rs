use sqlx::PgConnection;

use crate::{
    configuration::StorageBackend,
    error::PostSchedulingError,
    media::{Media, MediaType},
    storage::media_storage::store_image,
};

pub mod db;
pub mod media_storage;

pub async fn save_post_media(
    conn: &mut PgConnection,
    media: Media,
    storage: &StorageBackend,
) -> Result<(), PostSchedulingError> {
    let storage_key = media.generate_storage_key();
    let content_type = media.asset.metadata.format.to_mime_type();

    store_image(&media.asset.data, &storage, &storage_key, content_type)
        .await
        .map_err(PostSchedulingError::from)?;

    sqlx::query!(
        r#"
            INSERT INTO post_media
                (post_data_id, media_type, storage_key, content_type, size_bytes, width, height)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        media.post_id,
        String::from(MediaType::Image),
        storage_key,
        content_type,
        media.asset.metadata.size_bytes as i32,
        media.asset.metadata.width as i32,
        media.asset.metadata.height.cast_signed(),
    )
    .execute(conn)
    .await
    .map_err(PostSchedulingError::from)?;

    Ok(())
}

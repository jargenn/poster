use sqlx::SqliteConnection;
use tracing::{Instrument, instrument};

use crate::{
    configuration::StorageBackend,
    error::PostSchedulingError,
    media::{Media, MediaType},
    storage::media_storage::store_image,
};

pub mod db;
pub mod media_storage;

#[instrument("Saving the post media", skip(conn, media), fields(storage))]
pub async fn save_post_media(
    conn: &mut SqliteConnection,
    media: Media,
    storage: &StorageBackend,
) -> Result<(), PostSchedulingError> {
    let storage_key = media.generate_storage_key();
    tracing::info!(storage_key = %storage_key, "Generated storage key");
    let content_type = media.asset.metadata.format.to_mime_type();

    store_image(&media.asset.data, storage, &storage_key, content_type)
        .instrument(tracing::info_span!(
            "Storing the image according to the StorageBackend defined"
        ))
        .await
        .map_err(PostSchedulingError::from)?;

    sqlx::query(
        r#"
            INSERT INTO post_media
                (post_data_id, media_type, storage_key, content_type, size_bytes, width, height)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
    )
    .bind(media.post_id)
    .bind(String::from(MediaType::Image))
    .bind(storage_key)
    .bind(content_type)
    .bind(i32::try_from(media.asset.metadata.size_bytes).expect("Failed to cast usize to i32"))
    .bind(media.asset.metadata.width.cast_signed())
    .bind(media.asset.metadata.height.cast_signed())
    .execute(conn)
    .instrument(tracing::info_span!("Saving the post_media into the DB"))
    .await
    .map_err(PostSchedulingError::from)?;

    Ok(())
}

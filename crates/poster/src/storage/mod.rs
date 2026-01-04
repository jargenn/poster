use sqlx::PgConnection;

use crate::{
    configuration::StorageBackend,
    error::PostSchedulingError,
    media::{Media, MediaError, MediaType, StorageLocation},
};

pub mod db;

pub async fn save_post_media(
    conn: &mut PgConnection,
    media: Media,
    storage: &StorageBackend,
) -> Result<(), PostSchedulingError> {
    // 1. First save image to object storage or local as given
    let key = media.generate_storage_key();
    let location = store_image(&media.asset.data, &storage, &key)
        .await
        .map_err(PostSchedulingError::from)?;

    assert_eq!(location.storage_key(), key);

    // 2. Now save it in the database
    sqlx::query!(
        r#"
            INSERT INTO post_media
                (post_data_id, media_type, storage_key, content_type, size_bytes, width, height)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        media.post_id,
        String::from(MediaType::Image),
        key,
        media.asset.metadata.format.to_mime_type(),
        media.asset.metadata.size_bytes as i32,
        media.asset.metadata.width as i32,
        media.asset.metadata.height.cast_signed(),
    )
    .execute(conn)
    .await
    .map_err(PostSchedulingError::from)?;

    Ok(())
}

async fn store_image(
    data: &[u8],
    storage: &StorageBackend,
    key: &str,
) -> Result<StorageLocation, MediaError> {
    match storage {
        StorageBackend::Local {
            base_path,
            base_url,
        } => {
            let full_path = base_path.join(&key);

            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(MediaError::from)?;
            }

            tokio::fs::write(&full_path, data)
                .await
                .map_err(MediaError::from)?;

            let url = base_url.as_ref().map(|base| format!("{}/{}", base, key));

            Ok(StorageLocation::Local {
                path: key.to_owned(),
                full_path: full_path.to_string_lossy().to_string(),
                url,
            })
        }
        StorageBackend::R2 {
            bucket, public_url, ..
        } => {
            // TODO: Implement R2 upload using aws-sdk-s3
            let url = format!("{}/{}", public_url, key);
            Ok(StorageLocation::R2 {
                bucket: bucket.clone(),
                key: key.to_owned(),
                url,
            })
        }
    }
}

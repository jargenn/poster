use aws_config::BehaviorVersion;
use secrecy::ExposeSecret;

use crate::{configuration::StorageBackend, media::MediaError};

pub async fn store_image(
    data: &[u8],
    storage: &StorageBackend,
    key: &str,
    content_type: &str,
) -> Result<(), MediaError> {
    match storage {
        StorageBackend::Local { base_path, .. } => {
            let full_path = base_path.join(key);

            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    tracing::error!("Failure creating the dirs: {parent:?}");
                    MediaError::from(e)
                })?;
            }

            tokio::fs::write(&full_path, data).await.map_err(|e| {
                tracing::error!("Failure to save the image to {full_path:?}");
                MediaError::from(e)
            })?;
        }
        StorageBackend::R2 {
            account_id,
            access_key_id,
            secret_access_key,
            bucket,
            ..
        } => {
            use aws_sdk_s3::config::{Credentials, Region};
            use aws_sdk_s3::primitives::ByteStream;

            let credentials = Credentials::new(
                access_key_id.expose_secret(),
                secret_access_key.expose_secret(),
                None,
                None,
                "r2-credentials",
            );
            let r2_config = aws_sdk_s3::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .credentials_provider(credentials)
                .region(Region::new("auto"))
                .endpoint_url(format!("https://{account_id}.r2.cloudflarestorage.com"))
                .force_path_style(true)
                .build();

            let client = aws_sdk_s3::Client::from_conf(r2_config);
            let body = ByteStream::from(data.to_vec());

            client
                .put_object()
                .bucket(bucket)
                .key(key)
                .body(body)
                .content_type(content_type)
                .send()
                .await
                .map_err(|e| tracing::error!("Failure to save the image to bucket: {e}"))
                .expect("Failed to send request to the bucket");
        }
    };
    Ok(())
}

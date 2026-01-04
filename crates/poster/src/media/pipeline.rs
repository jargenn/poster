use base64::Engine;
use facebook_graph_api::Input;
use image::{DynamicImage, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::path::PathBuf;
use time::OffsetDateTime;
use tracing::{debug, error, instrument};

type ImagePipelineError = crate::media::error::Error;
use crate::{
    configuration::{MediaSettings, ProcessSettings, StorageBackend},
    error::Error,
};

/// Media type enum matching your schema constraint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
}

impl Into<String> for MediaType {
    fn into(self) -> String {
        match self {
            MediaType::Image => String::from("image"),
            MediaType::Video => String::from("video"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StorageLocation {
    Local {
        path: String,
        full_path: String,
        url: Option<String>,
    },
    R2 {
        bucket: String,
        key: String,
        url: String,
    },
}

impl StorageLocation {
    pub fn storage_key(&self) -> String {
        match self {
            StorageLocation::Local { full_path, .. } => full_path.to_owned(),
            StorageLocation::R2 { url, .. } => url.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Source {
    /// Local filesystem path
    LocalPath(PathBuf),
    /// Remote URL (could be R2, or any HTTP endpoint)
    Url(String),
    /// Raw bytes with optional filename hint
    Bytes {
        data: Vec<u8>,
        filename: Option<String>,
    },
}

impl TryFrom<Input> for Source {
    type Error = Error;
    fn try_from(value: Input) -> Result<Self, Self::Error> {
        match value {
            Input::Url(url) => {
                if url.starts_with("http://") || url.starts_with("https://") {
                    Ok(Source::Url(url))
                } else {
                    Ok(Source::LocalPath(PathBuf::from(url)))
                }
            }
            Input::Base64 { data, filename, .. } => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data.trim_start_matches("data:image/"))
                    .map_err(ImagePipelineError::from)?;

                Ok(Source::Bytes {
                    data: bytes,
                    filename,
                })
            }
        }
    }
}

pub struct Raw {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Decoded {
    /// TODO: Work on support video. Maybe using generics <A: impl Asset> or an enum?
    pub asset: DynamicImage,
    pub metadata: AssetMetadata,
}

#[derive(Debug)]
pub struct Plan {
    pub resize: bool,
    pub target_width: u32,
    pub target_format: ImageFormat,
    pub target_height: u32,
    pub jpeg_quality: u8,
    pub filter_type: FilterType,
    pub max_size_in_bytes: usize,
}

pub struct Planned {
    pub image: DynamicImage,
    pub metadata: AssetMetadata,
    pub plan: Plan,
}

#[derive(Debug, Clone)]
pub struct Processed {
    pub data: Vec<u8>,
    pub metadata: AssetMetadata,
}

pub struct Stored {
    pub location: StorageLocation,
    pub metadata: AssetMetadata,
}

#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub size_bytes: usize,
    pub has_alpha: bool,
}

/// Configuration for the media pipeline
#[derive(Debug, Clone)]
pub struct PipelineSettings {
    /// Storage backend to use
    pub storage: StorageBackend,
    /// Image processing rules
    pub processing: ProcessSettings,
    /// Optional temp directory for downloads
    pub temp_dir: Option<PathBuf>,
}

impl PipelineSettings {
    pub fn new(settings: MediaSettings) -> Self {
        Self {
            storage: settings.storage_settings,
            processing: settings.process_settings,
            temp_dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewPostMedia {
    pub post_data_id: u32,
    pub media_type: String,
    pub storage_key: String,
    pub content_type: String,
    pub size_bytes: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct MediaPipeline {
    config: PipelineSettings,
    http_client: reqwest::Client,
}

impl MediaPipeline {
    pub fn new(config: PipelineSettings) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    #[instrument("Processing media",skip(self, conn), fields(post_data_id = %post_data_id))]
    pub async fn process_media(
        &self,
        conn: &mut PgConnection,
        media_input: Input,
        post_data_id: u32,
        user_id: &str,
        page_id: &str,
    ) -> Result<(), Error> {
        let source = Source::try_from(media_input)?;
        let result = Asset::from_source(source, &self.http_client)
            .await?
            .decode()?
            .plan(&self.config.processing)?
            .process()
            .await?
            .store(page_id, user_id, self.config.storage.clone())
            .await?;

        let metadata = result.state.metadata;
        let new_post = NewPostMedia {
            post_data_id: post_data_id,
            media_type: MediaType::Image.into(),
            storage_key: result.state.location.storage_key(),
            content_type: metadata.format.to_mime_type().to_string(),
            size_bytes: metadata.size_bytes as u32,
            width: metadata.width,
            height: metadata.height,
        };

        self.save_to_database(conn, new_post).await?;

        Ok(())
    }

    pub async fn process_media_batch(
        &self,
        conn: &mut PgConnection,
        media_inputs: Vec<Input>,
        post_data_id: i32,
        user_id: &str,
        page_id: &str,
    ) -> Result<(), Error> {
        for media in media_inputs {
            if let Err(e) = self
                .process_media(conn, media, post_data_id as u32, user_id, page_id)
                .await
            {
                error!("Failed to process media item: {}", e);
                // TODO: Continue but show exactly what image failed
                // return Err(e);
            }
        }

        Ok(())
    }

    async fn save_to_database(
        &self,
        conn: &mut PgConnection,
        new_media: NewPostMedia,
    ) -> Result<i32, Error> {
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO post_media 
                (post_data_id, media_type, storage_key, content_type, size_bytes, width, height)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
            new_media.post_data_id.cast_signed(),
            new_media.media_type,
            new_media.storage_key,
            new_media.content_type,
            new_media.size_bytes.cast_signed(),
            new_media.width.cast_signed(),
            new_media.height.cast_signed()
        )
        .fetch_one(conn)
        .await
        .map_err(|e| Error::Database(e))?;

        Ok(id)
    }
}

#[derive(Debug)]
pub struct Asset<S> {
    state: S,
}

impl Asset<Raw> {
    pub async fn from_source(source: Source, client: &reqwest::Client) -> Result<Self, Error> {
        let bytes = Asset::load_bytes(&source, client).await?;

        Ok(Asset {
            state: Raw { bytes },
        })
    }

    pub fn decode(self) -> Result<Asset<Decoded>, Error> {
        let decoded = Self::decode_and_inspect(&self.state.bytes)?;

        Ok(Asset { state: decoded })
    }

    async fn load_bytes(source: &Source, client: &reqwest::Client) -> Result<Vec<u8>, Error> {
        debug!("Reading the bytes of the media");
        match source {
            Source::LocalPath(path) => Ok(tokio::fs::read(path)
                .await
                .map_err(ImagePipelineError::from)?),
            Source::Url(url) => {
                let response = client.get(url).send().await.map_err(Error::from)?;

                response
                    .bytes()
                    .await
                    .map(|b| b.to_vec())
                    .map_err(Error::from)
            }
            // FIX: Avoid cloning
            Source::Bytes { data, .. } => Ok(data.clone()),
        }
    }

    /// Takes the bytes of the media as input, derives some metadata from it, mainly: size in bytes, format,
    /// width and height and checks if they valid against the config given. Returns the metadata
    /// read and the decoded image.
    fn decode_and_inspect(bytes: &[u8]) -> Result<Decoded, Error> {
        use image::ImageReader;
        use std::io::Cursor;

        debug!("Reading image metadata");
        let (image, format) = {
            let reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(ImagePipelineError::from)?;

            let Some(format) = reader.format() else {
                return Err(ImagePipelineError::ValidationError(
                    "Couldn't define the image format".to_owned(),
                ))?;
            };

            (reader.decode().map_err(ImagePipelineError::from)?, format)
        };

        debug!(?format, "guessed format");

        let (width, height) = (image.width(), image.height());
        let size_bytes = bytes.len();
        let has_alpha = image.has_alpha();
        debug!(?format, %size_bytes, %width,%height,%has_alpha, "Metadata of the decoded image");

        Ok(Decoded {
            asset: image,
            metadata: AssetMetadata {
                width,
                height,
                format,
                size_bytes,
                has_alpha,
            },
        })
    }
}

impl Asset<Decoded> {
    pub fn plan(self, settings: &ProcessSettings) -> Result<Asset<Planned>, Error> {
        let plan = Self::decide_transform_plan(&self.state.metadata, settings)?;

        Ok(Asset {
            state: Planned {
                image: self.state.asset,
                metadata: self.state.metadata,
                plan,
            },
        })
    }

    /// Derives a transform plan given the image metadata.
    fn decide_transform_plan(
        metadata: &AssetMetadata,
        settings: &ProcessSettings,
    ) -> Result<Plan, Error> {
        let resize = metadata.width > settings.max_width || metadata.height > settings.max_height;

        let content_type = metadata.format.to_mime_type();
        if !settings
            .allowed_content_types
            .contains(&content_type.to_string())
        {
            return Err(ImagePipelineError::UnsupportedFormat {
                allowed: settings.allowed_content_types.clone(),
                got: content_type.to_string(),
            })?;
        }

        Ok(Plan {
            resize,
            target_width: settings.target_width.unwrap_or(settings.max_width),
            target_height: settings.target_height.unwrap_or(settings.max_height),
            filter_type: FilterType::Lanczos3,
            max_size_in_bytes: settings.max_size_bytes,
            jpeg_quality: settings.jpeg_quality,
            // FIX: This is not okay
            target_format: ImageFormat::Jpeg,
        })
    }
}

impl Asset<Planned> {
    pub async fn process(self) -> Result<Asset<Processed>, Error> {
        let processed = Self::apply_transform_plan(self.state.image, self.state.plan)?;

        Ok(Asset { state: processed })
    }

    /// Inspect the image passed to it, determines if it needs resizing
    fn apply_transform_plan(mut image: DynamicImage, plan: Plan) -> Result<Processed, Error> {
        if plan.resize {
            image = image.resize(
                plan.target_width,
                plan.target_height,
                image::imageops::FilterType::Lanczos3,
            );
        }

        let mut output = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut output, plan.jpeg_quality);
        encoder
            .encode_image(&image)
            .map_err(ImagePipelineError::from)?;

        let size = output.len();
        if size > plan.max_size_in_bytes {
            return Err(ImagePipelineError::FileSizeTooLarge {
                size: output.len(),
                max_size: plan.max_size_in_bytes,
            })?;
        }

        Ok(Processed {
            data: output,
            metadata: AssetMetadata {
                width: image.width(),
                height: image.height(),
                // FIX: Decide the format
                format: ImageFormat::Jpeg,
                size_bytes: size,
                has_alpha: image.has_alpha(),
            },
        })
    }
}

impl Asset<Processed> {
    pub async fn store(
        self,
        user_id: &str,
        page_id: &str,
        storage: StorageBackend,
    ) -> Result<Asset<Stored>, Error> {
        let key = Self::generate_storage_key(user_id, page_id);
        let location = Self::store_image(&self.state.data, storage, &key).await?;

        Ok(Asset {
            state: Stored {
                location,
                metadata: self.state.metadata,
            },
        })
    }

    async fn store_image(
        data: &[u8],
        storage: StorageBackend,
        key: &str,
    ) -> Result<StorageLocation, Error> {
        match storage {
            StorageBackend::Local {
                base_path,
                base_url,
            } => {
                let full_path = base_path.join(&key);

                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(ImagePipelineError::from)?;
                }

                tokio::fs::write(&full_path, data)
                    .await
                    .map_err(ImagePipelineError::from)?;

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

    fn generate_storage_key(user_id: &str, page_id: &str) -> String {
        let now = OffsetDateTime::now_utc();
        let uuid = uuid::Uuid::new_v4();

        format!(
            "{user_id}/{page_id}/{year}/{month:02}/{uuid}",
            // "{user_id}/{page_id}/{year}/{month:02}/{uuid}.{ext}",
            year = now.year(),
            month = now.month(),
            // ext = metadata.format.extensions_str()
        )
    }
}

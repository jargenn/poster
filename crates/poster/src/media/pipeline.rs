use base64::Engine;
use facebook_graph_api::Input;
use image::{DynamicImage, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use time::OffsetDateTime;
use tracing::debug;

use crate::{configuration::ProcessSettings, media::MediaError};

/// Media type enum matching your schema constraint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
}

impl From<MediaType> for String {
    fn from(value: MediaType) -> Self {
        match value {
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
    type Error = MediaError;
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
                    .map_err(MediaError::from)?;

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

/// Media properly validated and with the post_id from where it belongs
#[derive(Debug, Clone)]
pub struct Media {
    pub post_id: i32,
    pub user_id: String,
    pub page_id: String,
    pub asset: Processed,
}
impl Media {
    pub fn generate_storage_key(&self) -> String {
        let now = OffsetDateTime::now_utc();
        let uuid = uuid::Uuid::new_v4();

        let ext = self
            .asset
            .metadata
            .format
            .extensions_str()
            .first()
            .copied()
            .unwrap_or("unknown");

        format!(
            "{user_id}/{page_id}/{year}/{month:02}/{uuid}.{ext}",
            year = now.year(),
            month = now.month(),
            user_id = &self.user_id,
            page_id = &self.page_id,
        )
    }
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

#[derive(Debug)]
pub struct Pipeline<S> {
    state: S,
}

impl Pipeline<Raw> {
    pub async fn from_input(input: Input, client: &reqwest::Client) -> Result<Self, MediaError> {
        let source = Source::try_from(input)?;
        let bytes = Pipeline::load_bytes(&source, client).await?;

        Ok(Pipeline {
            state: Raw { bytes },
        })
    }

    pub fn decode(self) -> Result<Pipeline<Decoded>, MediaError> {
        let decoded = Self::decode_and_inspect(&self.state.bytes)?;

        Ok(Pipeline { state: decoded })
    }

    async fn load_bytes(source: &Source, client: &reqwest::Client) -> Result<Vec<u8>, MediaError> {
        debug!("Reading the bytes of the media");
        match source {
            Source::LocalPath(path) => Ok(tokio::fs::read(path).await.map_err(MediaError::from)?),
            Source::Url(url) => {
                let response = client.get(url).send().await.map_err(MediaError::from)?;

                response
                    .bytes()
                    .await
                    .map(|b| b.to_vec())
                    .map_err(MediaError::from)
            }
            // FIX: Avoid cloning
            Source::Bytes { data, .. } => Ok(data.clone()),
        }
    }

    /// Takes the bytes of the media as input, derives some metadata from it, mainly: size in bytes, format,
    /// width and height and checks if they valid against the config given. Returns the metadata
    /// read and the decoded image.
    fn decode_and_inspect(bytes: &[u8]) -> Result<Decoded, MediaError> {
        use image::ImageReader;
        use std::io::Cursor;

        debug!("Reading image metadata");
        let (image, format) = {
            let reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(MediaError::from)?;

            let Some(format) = reader.format() else {
                return Err(MediaError::ValidationError(
                    "Couldn't define the image format".to_owned(),
                ))?;
            };

            (reader.decode().map_err(MediaError::from)?, format)
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

impl Pipeline<Decoded> {
    pub fn plan(self, settings: &ProcessSettings) -> Result<Pipeline<Planned>, MediaError> {
        let plan = Self::decide_transform_plan(&self.state.metadata, settings)?;

        Ok(Pipeline {
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
    ) -> Result<Plan, MediaError> {
        let resize = metadata.width > settings.max_width || metadata.height > settings.max_height;

        let content_type = metadata.format.to_mime_type();
        if !settings
            .allowed_content_types
            .contains(&content_type.to_string())
        {
            return Err(MediaError::UnsupportedFormat {
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

impl Pipeline<Planned> {
    pub async fn process(
        self,
        post_id: i32,
        user_id: &str,
        page_id: &str,
    ) -> Result<Media, MediaError> {
        let data = Self::apply_transform_plan(self.state.image, self.state.plan)?;

        Ok(Media {
            post_id,
            user_id: user_id.to_owned(),
            page_id: page_id.to_owned(),
            asset: data,
        })
    }

    /// Inspect the image passed to it, determines if it needs resizing
    fn apply_transform_plan(mut image: DynamicImage, plan: Plan) -> Result<Processed, MediaError> {
        if plan.resize {
            image = image.resize(
                plan.target_width,
                plan.target_height,
                image::imageops::FilterType::Lanczos3,
            );
        }

        let mut output = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut output, plan.jpeg_quality);
        encoder.encode_image(&image).map_err(MediaError::from)?;

        let size = output.len();
        if size > plan.max_size_in_bytes {
            return Err(MediaError::FileSizeTooLarge {
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

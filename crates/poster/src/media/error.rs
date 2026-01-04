use base64::DecodeError;
use image::ImageError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to load image from source: {0}")]
    LoadError(String),

    #[error("Image validation failed: {0}")]
    ValidationError(String),

    #[error("Invalid base64: {0}")]
    InvalidMedia(#[from] DecodeError),

    #[error("Image dimensions {width}x{height} exceed maximum {max_width}x{max_height}")]
    DimensionsTooLarge {
        width: u32,
        height: u32,
        max_width: u32,
        max_height: u32,
    },

    #[error("Image size {size} bytes exceeds maximum {max_size} bytes")]
    FileSizeTooLarge { size: usize, max_size: usize },

    #[error("Unsupported image format. Allowed: {allowed:?}, got: {got}")]
    UnsupportedFormat { allowed: Vec<String>, got: String },

    #[error("Image processing/optimization failed: {0}")]
    ProcessingError(#[from] ImageError),

    #[error("Storage operation failed: {0}")]
    StorageError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

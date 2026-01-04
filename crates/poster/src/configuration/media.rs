use std::path::PathBuf;

use secrecy::SecretString;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MediaSettings {
    #[serde(rename = "process")]
    pub process_settings: ProcessSettings,
    #[serde(rename = "storage")]
    pub storage_settings: StorageBackend,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessSettings {
    /// Maximum width allowed (default: 4096 per your schema)
    pub max_width: u32,
    /// Maximum height allowed (default: 4096 per your schema)
    pub max_height: u32,
    /// Maximum file size in bytes (default: 10MB per your schema)
    pub max_size_bytes: usize,
    /// Allowed MIME types
    pub allowed_content_types: Vec<String>,
    /// Quality for JPEG compression (1-100)
    pub jpeg_quality: u8,
    /// Target width for resizing (if auto_resize is true)
    pub target_width: Option<u32>,
    /// Target height for resizing (if auto_resize is true)
    pub target_height: Option<u32>,
}

impl Default for ProcessSettings {
    fn default() -> Self {
        Self {
            max_width: 4096,
            max_height: 4096,
            max_size_bytes: 10 * 1024 * 1024, // 10MB
            allowed_content_types: vec![
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/webp".to_string(),
            ],
            jpeg_quality: 85,
            target_width: Some(2000),
            target_height: Some(2000),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum StorageBackend {
    R2 {
        account_id: String,
        access_key_id: SecretString,
        secret_access_key: SecretString,
        auth_token: SecretString,
        bucket: String,
        public_url: Option<String>,
    },
    Local {
        base_path: PathBuf,
        base_url: Option<String>,
    },
}

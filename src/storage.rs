use s3::Region;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use std::fmt;

use crate::app_settings::AppSettings;

#[derive(Debug)]
pub enum StorageError {
    NotFound,
    AccessDenied,
    Unavailable(String),
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::NotFound => write!(f, "Object not found"),
            StorageError::AccessDenied => write!(f, "Access denied"),
            StorageError::Unavailable(msg) => write!(f, "Storage unavailable: {}", msg),
            StorageError::Other(msg) => write!(f, "Storage error: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

pub struct Storage {
    bucket: Box<Bucket>,
    prefix: String,
}

impl Storage {
    pub fn new(settings: &AppSettings) -> Self {
        let region = match &settings.s3_endpoint {
            Some(endpoint) => Region::Custom {
                region: settings.s3_region.clone(),
                endpoint: endpoint.clone(),
            },
            None => settings
                .s3_region
                .parse::<Region>()
                .expect("Invalid S3 region"),
        };

        let credentials = match (&settings.s3_access_key, &settings.s3_secret_key) {
            (Some(access_key), Some(secret_key)) => {
                Credentials::new(Some(access_key), Some(secret_key), None, None, None)
                    .expect("Failed to create S3 credentials")
            }
            _ => Credentials::default()
                .expect("Failed to resolve AWS credentials. Set S3_ACCESS_KEY and S3_SECRET_KEY or configure IAM role"),
        };

        let mut bucket = Bucket::new(&settings.s3_bucket, region, credentials)
            .expect("Failed to create S3 bucket handle");

        if settings.s3_use_path_style {
            bucket = bucket.with_path_style();
        }

        Self {
            bucket,
            prefix: settings.s3_prefix.clone(),
        }
    }

    /// Construct the full S3 object key for a given cache ID.
    fn object_key(&self, cache_id: &str) -> String {
        format!("{}/{}", self.prefix, cache_id)
    }

    /// Retrieve a file from S3. Returns Ok(Some(bytes)) on hit, Ok(None) on miss.
    pub async fn get_file(&self, cache_id: &str) -> Result<Option<bytes::Bytes>, StorageError> {
        let key = self.object_key(cache_id);
        let response = self.bucket.get_object(&key).await;

        match response {
            Ok(resp) => {
                if resp.status_code() == 404 {
                    Ok(None)
                } else if resp.status_code() == 200 {
                    Ok(Some(bytes::Bytes::from(resp.to_vec())))
                } else {
                    Err(map_s3_status(resp.status_code()))
                }
            }
            Err(e) => Err(map_s3_error(e)),
        }
    }

    /// Store a file in S3.
    pub async fn put_file(&self, cache_id: &str, data: bytes::Bytes) -> Result<(), StorageError> {
        let key = self.object_key(cache_id);
        let response = self.bucket.put_object(&key, &data).await;

        match response {
            Ok(resp) => {
                if resp.status_code() == 200 || resp.status_code() == 201 {
                    Ok(())
                } else {
                    Err(map_s3_status(resp.status_code()))
                }
            }
            Err(e) => Err(map_s3_error(e)),
        }
    }
}

fn map_s3_status(status: u16) -> StorageError {
    match status {
        404 => StorageError::NotFound,
        403 => StorageError::AccessDenied,
        502..=504 => StorageError::Unavailable(format!("S3 returned status {}", status)),
        _ => StorageError::Other(format!("S3 returned unexpected status {}", status)),
    }
}

fn map_s3_error(err: s3::error::S3Error) -> StorageError {
    let msg = err.to_string();
    if msg.contains("NoSuchKey") {
        StorageError::NotFound
    } else if msg.contains("AccessDenied") || msg.contains("Forbidden") {
        StorageError::AccessDenied
    } else if msg.contains("timeout") || msg.contains("Timeout") || msg.contains("connection") {
        StorageError::Unavailable(msg)
    } else {
        StorageError::Other(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_key_construction() {
        let storage = Storage {
            bucket: Bucket::new(
                "test-bucket",
                "us-east-1".parse().unwrap(),
                Credentials::anonymous().unwrap(),
            )
            .unwrap(),
            prefix: "rush-cache".to_string(),
        };
        assert_eq!(storage.object_key("abc123"), "rush-cache/abc123");
    }

    #[test]
    fn test_object_key_with_custom_prefix() {
        let storage = Storage {
            bucket: Bucket::new(
                "test-bucket",
                "us-east-1".parse().unwrap(),
                Credentials::anonymous().unwrap(),
            )
            .unwrap(),
            prefix: "custom-prefix".to_string(),
        };
        assert_eq!(storage.object_key("def456"), "custom-prefix/def456");
    }

    #[test]
    fn test_map_s3_status_codes() {
        assert!(matches!(map_s3_status(404), StorageError::NotFound));
        assert!(matches!(map_s3_status(403), StorageError::AccessDenied));
        assert!(matches!(map_s3_status(503), StorageError::Unavailable(_)));
        assert!(matches!(map_s3_status(502), StorageError::Unavailable(_)));
        assert!(matches!(map_s3_status(500), StorageError::Other(_)));
    }
}

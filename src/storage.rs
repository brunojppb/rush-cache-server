use std::fmt;

use s3::Region;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::request::ResponseDataStream;
use tokio::io::AsyncRead;

use secrecy::ExposeSecret;

use crate::app_settings::AppSettings;

#[derive(Debug)]
pub enum StorageError {
    /// The bucket answered, but holds no object under that key.
    NotFound,
    /// The bucket could not be reached, or rejected the request.
    Unreachable(S3Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "no such object in the bucket"),
            Self::Unreachable(error) => write!(f, "S3 request failed: {error}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotFound => None,
            Self::Unreachable(error) => Some(error),
        }
    }
}

impl From<S3Error> for StorageError {
    fn from(error: S3Error) -> Self {
        match error {
            S3Error::HttpFailWithBody(404, _) => Self::NotFound,
            other => Self::Unreachable(other),
        }
    }
}

pub struct Storage {
    bucket: Box<Bucket>,
    prefix: String,
}

/// Hand-written so credentials never reach a log or tracing span: `s3::Bucket`
/// derives `Debug` and its `Credentials` print the access and secret keys in
/// plaintext.
impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Storage")
            .field("bucket_name", &self.bucket.name)
            .field("region", &self.bucket.region)
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
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
                Credentials::new(
                    Some(access_key.expose_secret()),
                    Some(secret_key.expose_secret()),
                    None,
                    None,
                    None,
                )
                .expect("Failed to create S3 credentials")
            }
            _ => Credentials::default().expect(
                "Failed to resolve AWS credentials. Set S3_ACCESS_KEY and S3_SECRET_KEY or configure IAM role",
            ),
        };

        let mut bucket = Bucket::new(&settings.s3_bucket, region, credentials)
            .expect("Failed to create S3 bucket handle");

        if settings.s3_use_path_style {
            bucket.set_path_style();
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

    /// Stream a file from S3.
    #[tracing::instrument(name = "get S3 file", skip(self))]
    pub async fn get_file(&self, cache_id: &str) -> Result<ResponseDataStream, StorageError> {
        let key = self.object_key(cache_id);
        let file = self.bucket.get_object_stream(&key).await?;
        Ok(file)
    }

    /// Stream data from the reader to S3.
    #[tracing::instrument(name = "put S3 file stream", skip(self, reader))]
    pub async fn put_file_stream<R>(
        &self,
        cache_id: &str,
        reader: &mut R,
    ) -> Result<(), StorageError>
    where
        R: AsyncRead + Unpin,
    {
        let key = self.object_key(cache_id);
        let builder = self.bucket.put_object_stream_builder(&key);

        builder.execute_stream(reader).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_settings::TokenStore;
    use secrecy::SecretString;
    use std::collections::HashSet;

    fn settings_with_credentials() -> AppSettings {
        AppSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            s3_region: "us-east-1".to_string(),
            s3_bucket: "test-bucket".to_string(),
            s3_prefix: "rush-cache".to_string(),
            s3_endpoint: Some("http://localhost:9000".to_string()),
            s3_access_key: Some(SecretString::new("super-secret-access-key".into())),
            s3_secret_key: Some(SecretString::new("super-secret-secret-key".into())),
            s3_use_path_style: true,
            log_level: "info".to_string(),
            logs_directory: None,
            token_store: TokenStore::new(HashSet::new(), HashSet::new()),
        }
    }

    /// Storage reaches tracing spans, which record it through `Debug`.
    #[test]
    fn test_debug_output_hides_s3_credentials() {
        let storage = Storage::new(&settings_with_credentials());

        let debug_output = format!("{storage:?}");

        assert!(!debug_output.contains("super-secret-access-key"));
        assert!(!debug_output.contains("super-secret-secret-key"));
        assert!(debug_output.contains("test-bucket"));
    }

    #[test]
    fn test_storage_error_maps_404_to_not_found() {
        let error = StorageError::from(S3Error::HttpFailWithBody(404, "NoSuchKey".to_string()));
        assert!(matches!(error, StorageError::NotFound));
    }

    #[test]
    fn test_storage_error_maps_other_statuses_to_unreachable() {
        let error = StorageError::from(S3Error::HttpFailWithBody(403, "AccessDenied".to_string()));
        assert!(matches!(error, StorageError::Unreachable(_)));
    }

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
}

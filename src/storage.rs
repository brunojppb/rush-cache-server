use s3::Region;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::request::ResponseDataStream;
use tokio::io::AsyncRead;

use crate::app_settings::AppSettings;

#[derive(Debug)]
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

    /// Stream a file from S3. Returns Some(stream) on hit, None on miss.
    #[tracing::instrument(name = "get S3 file", skip(self))]
    pub async fn get_file(&self, cache_id: &str) -> Option<ResponseDataStream> {
        let key = self.object_key(cache_id);
        self.bucket.get_object_stream(&key).await.ok()
    }

    /// Stream data from the reader to S3.
    #[tracing::instrument(name = "put S3 file stream", skip(self, reader))]
    pub async fn put_file_stream<R>(&self, cache_id: &str, reader: &mut R) -> Result<(), String>
    where
        R: AsyncRead + Unpin,
    {
        let key = self.object_key(cache_id);
        let builder = self.bucket.put_object_stream_builder(&key);

        match builder.execute_stream(reader).await {
            Ok(_response) => Ok(()),
            Err(e) => Err(format!("Could not upload file: {e}")),
        }
    }

    /// Check whether an object exists in S3.
    #[tracing::instrument(name = "check S3 file exists", skip(self))]
    pub async fn file_exists(&self, cache_id: &str) -> bool {
        let key = self.object_key(cache_id);
        self.bucket.head_object(&key).await.is_ok()
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
}

use aws_sdk_s3 as s3;
use s3::error::ProvideErrorMetadata;
use s3::primitives::ByteStream;
use ssri::Integrity;
use utils::errors::{ServalError, ServalResult};

#[derive(Debug, Clone)]
pub struct S3Storage {
    client: s3::Client,
    bucket: String,
}

impl S3Storage {
    pub fn new(bucket_name: &str, config: aws_config::SdkConfig) -> ServalResult<Self> {
        let client = s3::Client::new(&config);

        Ok(S3Storage {
            client,
            bucket: bucket_name.to_string(),
        })
    }

    /// Check if the given data blob is present in our data store, by integrity hash.
    pub async fn data_by_sri(&self, integrity: Integrity) -> ServalResult<ByteStream> {
        let key = integrity.to_string();
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(object.body)
    }

    /// Check if the given data blob is present in our data store, using its human key.
    pub async fn data_exists_by_key(&self, key: &str) -> ServalResult<bool> {
        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        Ok(result.is_ok())
    }

    /// Fetch data from the store by key.
    pub async fn data_by_key(&self, key: &str) -> ServalResult<Vec<u8>> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        let chunks = object.body.collect().await?;
        Ok(chunks.into_bytes().to_vec())
    }

    /// Fetch data by key as a readable byte stream.
    pub async fn stream_by_key(&self, key: &str) -> ServalResult<ByteStream> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(object.body)
    }

    /// Store data by key.
    pub async fn store_by_key(&self, key: &str, bytes: &[u8]) -> ServalResult<Integrity> {
        let sri = Integrity::from(bytes);
        let body = ByteStream::from(bytes.to_vec());
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_type("application/octet-stream")
            .send()
            .await;

        match result {
            Ok(_resp) => Ok(sri),
            Err(e) => {
                log::info!("Error storing data in s3: {e:?}");

                Err(ServalError::StorageError(format!(
                    "unable to store executable! key={}; error={}",
                    key,
                    e.message().unwrap_or("cannot get error message from AWS")
                )))
            }
        }
    }
}

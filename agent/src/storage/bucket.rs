use aws_sdk_s3 as s3;
use s3::error::ProvideErrorMetadata;
use s3::primitives::ByteStream;
use ssri::Integrity;
use urlencoding::encode;
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

    /// Check if the given data blob is present in our data store, by integrity hash. Returns a stream.
    pub async fn stream_by_integrity(&self, integrity: &Integrity) -> ServalResult<ByteStream> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(encode(&integrity.to_string()))
            .send()
            .await?;

        Ok(object.body)
    }

    pub async fn store_by_integrity(
        &self,
        integrity: &Integrity,
        bytes: &[u8],
    ) -> ServalResult<Integrity> {
        let body = ByteStream::from(bytes.to_vec());
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(encode(&integrity.to_string())) // integrity string is not url-safe!
            .body(body)
            .content_type("application/octet-stream")
            .send()
            .await;

        match result {
            Ok(_resp) => Ok(integrity.clone()),
            Err(e) => {
                log::info!("Error storing data in s3: {e:?}");

                Err(ServalError::StorageError(format!(
                    "unable to store data! integrity={integrity}; error={}",
                    e.message().unwrap_or("cannot get error message from AWS")
                )))
            }
        }
    }

    /// Check if the given data blob is present in our data store, using its human key.
    pub async fn data_exists_by_key(&self, key: &str) -> ServalResult<bool> {
        let integrity = self.lookup_integrity(key).await?;
        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&integrity)
            .send()
            .await;
        Ok(result.is_ok())
    }

    /// Fetch data from the store by key. Returns a vec of u8.
    pub async fn data_by_key(&self, key: &str) -> ServalResult<Vec<u8>> {
        let integrity = self.lookup_integrity(key).await?;
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&integrity)
            .send()
            .await?;
        let chunks = object.body.collect().await?;
        Ok(chunks.into_bytes().to_vec())
    }

    /// Fetch data by key as a readable byte stream.
    pub async fn stream_by_key(&self, key: &str) -> ServalResult<ByteStream> {
        let integrity = self.lookup_integrity(key).await?;
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&integrity)
            .send()
            .await?;

        Ok(object.body)
    }

    /// Look up an integrity checksum for a given key. Url-encodes the integrity string.
    /// Really cheap index. Feel free to replace.
    async fn lookup_integrity(&self, key: &str) -> ServalResult<String> {
        let keyfile = format!("{key}.integrity");
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&keyfile)
            .send()
            .await
        {
            Ok(object) => {
                let chunks = object.body.collect().await?;
                let bytes = chunks.into_bytes().to_vec();
                let integrity_string = String::from_utf8(bytes)?;
                let encoded = encode(&integrity_string);
                Ok(encoded.to_string())
            }
            Err(e) => {
                log::info!(
                    "integrity checksum not found for key={key}; keyfile={keyfile}; error={e}"
                );
                Err(ServalError::S3GetError(e))
            }
        }
    }

    /// Store data by key.
    pub async fn store_by_key(&self, key: &str, bytes: &[u8]) -> ServalResult<Integrity> {
        let integrity = Integrity::from(bytes);
        let keyfile = format!("{key}.integrity");
        let keybody = ByteStream::from(integrity.to_string().as_bytes().to_vec());

        if let Err(failure) = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(keyfile)
            .body(keybody)
            .content_type("text/plain")
            .send()
            .await
        {
            return Err(ServalError::StorageError(format!(
                "failed to write integrity key file to S3; error={failure}"
            )));
        }

        match self.store_by_integrity(&integrity, bytes).await {
            Ok(integrity) => Ok(integrity),
            Err(e) => {
                log::info!("Error storing data in s3: {e:?}");

                Err(ServalError::StorageError(format!(
                    "unable to store executable! key={key}; integrity={integrity}; error={e}"
                )))
            }
        }
    }
}

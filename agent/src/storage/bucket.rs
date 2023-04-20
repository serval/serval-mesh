use aws_sdk_s3 as s3;
use s3::error::ProvideErrorMetadata;
use s3::primitives::ByteStream;
use ssri::Integrity;
use utils::errors::{ServalError, ServalResult};
use utils::structs::Manifest;

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

    /// Checks if the given job type is present in our data store, using the fully-qualified name.
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

    /// Fetch a manifest by its fully-qualified name.
    pub async fn manifest(&self, fq_name: &str) -> ServalResult<Manifest> {
        let key = Manifest::make_manifest_key(fq_name);
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        let chunks = object.body.collect().await?;
        let string = String::from_utf8(chunks.into_bytes().to_vec())?;
        let manifest: Manifest = toml::from_str(&string)?;
        Ok(manifest)
    }

    /// Retrieve a list of all Wasm manifests stored in the bucket.
    pub async fn manifest_names(&self) -> ServalResult<Vec<String>> {
        // this is the only complicated one
        todo!()
    }

    /// Store a Wasm manifest in the bucket. Returns the integrity checksum.
    pub async fn store_manifest(&self, manifest: &Manifest) -> ServalResult<Integrity> {
        let toml = toml::to_string(manifest)?;
        let bytes = toml.as_bytes();
        let body = ByteStream::from(bytes.to_vec());
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(manifest.manifest_key())
            .body(body)
            .content_type("application/toml")
            .send()
            .await;

        match result {
            Ok(_) => {
                let sri = Integrity::from(bytes);
                Ok(sri)
            }
            Err(e) => {
                log::info!("Error storing data in s3: {e:?}");

                Err(ServalError::StorageError(format!(
                    "unable to store manifest! manifest={}; error={}",
                    manifest.manifest_key(),
                    e
                )))
            }
        }
    }

    /// Fetch the bytes of the named executable.
    pub async fn executable_as_bytes(&self, name: &str, version: &str) -> ServalResult<Vec<u8>> {
        let key = Manifest::make_executable_key(name, version);
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

    /// Fetch an executable by key as a readable byte stream.
    pub async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> ServalResult<ByteStream> {
        let key = Manifest::make_executable_key(name, version);
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(object.body)
    }

    /// Store an executable in our bucket by its fully-qualified manifest name and a version string.
    pub async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> ServalResult<Integrity> {
        let key = Manifest::make_executable_key(name, version);
        let sri = Integrity::from(bytes); // note that we'll end up using this as the key. but not just yet!
        let body = ByteStream::from(bytes.to_vec());

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .content_type("application/octet-stream")
            .send()
            .await;

        match result {
            Ok(_) => Ok(sri),
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

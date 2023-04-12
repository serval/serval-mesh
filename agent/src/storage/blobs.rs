use async_trait::async_trait;
use cacache::Reader;
use serde::Serialize;
use ssri::Integrity;
use tokio_util::io::ReaderStream;

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use utils::errors::ServalError;
use utils::structs::Manifest;

use super::Storage;

#[derive(Clone, Debug, Serialize)]
pub struct BlobStore {
    location: PathBuf,
}

impl BlobStore {
    /// Create a new blob store, passing in a path to a writeable directory
    pub fn new(location: PathBuf) -> Result<Self, ServalError> {
        if !location.exists() {
            fs::create_dir(&location)?;
        }
        if !location.is_dir() {
            // todo: ErrorKind::NotADirectory would be more appropriate but as of 2023-01-11, that
            // error is still behind an unstable flag "io_error_more". It should theoretically be
            // usable by tomorrow's nightlies, oddly enough -- weird timing.
            // https://github.com/rust-lang/rust/pull/106375
            return Err(ServalError::IoError(ErrorKind::PermissionDenied.into()));
        }
        let md = fs::metadata(&location)?;
        if md.permissions().readonly() {
            return Err(ServalError::IoError(ErrorKind::PermissionDenied.into()));
        }

        Ok(Self { location })
    }
}

#[async_trait]
impl Storage for BlobStore {
    type A = Reader;

    /// Fetch a manifest by its fully-qualified name.
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError> {
        let bytes = cacache::read(&self.location, Manifest::make_manifest_key(fq_name)).await?;
        if let Ok(data) = String::from_utf8(bytes) {
            let manifest: Manifest = toml::from_str(&data)?;
            Ok(manifest)
        } else {
            // TODO: bad data error
            Err(ServalError::ManifestNotFound(fq_name.to_string()))
        }
    }

    /// Store a job type manifest. Returns the integrity checksum.
    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError> {
        let toml = toml::to_string(manifest)?;
        let meta_sri = cacache::write(&self.location, manifest.manifest_key(), &toml).await?;
        Ok(meta_sri)
    }

    /// Store a job with metadata and an executable for later use. Returns the integrity checksums for the pair.
    async fn store_manifest_and_executable(
        &self,
        manifest: &Manifest,
        executable: &[u8],
    ) -> Result<(Integrity, Integrity), ServalError> {
        let toml = toml::to_string(manifest)?;
        let meta_sri = cacache::write(&self.location, manifest.manifest_key(), &toml).await?;
        let exec_sri =
            cacache::write(&self.location, manifest.executable_key(), executable).await?;

        Ok((meta_sri, exec_sri))
    }

    /// Store an executable in our blob store by its fully-qualified manifest name and a version string.
    async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> Result<Integrity, ServalError> {
        let key = Manifest::make_executable_key(name, version);
        let sri = cacache::write(&self.location, key, bytes).await?;
        Ok(sri)
    }

    /// Given a content address, return a read stream for the object stored there.
    /// Responds with an error if no object is found or if the address is invalid.
    async fn executable_by_sri(&self, address: &str) -> Result<ReaderStream<Reader>, ServalError> {
        let integrity: Integrity = address.parse()?;
        let fd = cacache::Reader::open_hash(&self.location, integrity).await?;
        let stream = ReaderStream::new(fd);
        Ok(stream)
    }

    /// Fetch an executable by key as a read stream.
    async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ReaderStream<Reader>, ServalError> {
        let key = Manifest::make_executable_key(name, version);
        let fd = cacache::Reader::open(&self.location, key).await?;
        let stream = ReaderStream::new(fd);
        Ok(stream)
    }

    /// A non-streaming way to retrieve a stored blob. Prefer executable_as_stream() if you can.
    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError> {
        let key = Manifest::make_executable_key(name, version);
        let binary: Vec<u8> = cacache::read(&self.location, key).await?;
        Ok(binary)
    }

    /// Checks if the given blob is in the content store, by its SRI string.
    async fn data_exists_by_hash(&self, address: &str) -> Result<bool, ServalError> {
        let integrity: Integrity = address.parse()?;
        Ok(cacache::exists(&self.location, &integrity).await)
    }

    /// Checks if the given job type is present in our data store, using the fully-qualified name.
    async fn data_exists_by_key(&self, fq_name: &str) -> Result<bool, ServalError> {
        let key = Manifest::make_manifest_key(fq_name);
        match cacache::Reader::open(&self.location, key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // TODO: probably should handle errors more granularly
        }
    }

    async fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        let result: Vec<String> = cacache::list_sync(&self.location)
            .filter(|xs| xs.is_ok())
            .map(|xs| xs.unwrap().key)
            .filter(|xs| xs.contains("manifest"))
            .collect();
        Ok(result)
    }
}

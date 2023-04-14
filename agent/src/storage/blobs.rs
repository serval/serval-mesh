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

use crate::storage::RunnerStorage;

/// This struct manages an agent's local cache of wasm jobs (manifests and executables).
/// This cache uses the cacache crate behind the scenes, but this is an implementation detail
/// we've hidden here. There are three functions that are speculative implementations
/// of some features that cacache allows us to have easily but that we have not yet exposed
/// in the agent's API. Those are marked as lint exceptions.
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

    /// Fetch an executable by key as a read stream.
    pub async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ReaderStream<Reader>, ServalError> {
        let key = Manifest::make_executable_key(name, version);
        let fd = cacache::Reader::open(&self.location, key).await?;
        let stream = ReaderStream::new(fd);
        Ok(stream)
    }

    // Unused; consider removing
    #[allow(dead_code)]
    /// Given a content address, return a read stream for the object stored there.
    /// Responds with an error if no object is found or if the address is invalid.
    pub async fn executable_by_sri(
        &self,
        address: &str,
    ) -> Result<ReaderStream<Reader>, ServalError> {
        let integrity: Integrity = address.parse()?;
        let fd = cacache::Reader::open_hash(&self.location, integrity).await?;
        let stream = ReaderStream::new(fd);
        Ok(stream)
    }

    // Unused; consider removing
    #[allow(dead_code)]
    /// Checks if the given blob is in the content store, by its SRI string.
    pub async fn data_exists_by_hash(&self, address: &str) -> Result<bool, ServalError> {
        let integrity: Integrity = address.parse()?;
        Ok(cacache::exists(&self.location, &integrity).await)
    }

    /// Checks if the given job type is present in our data store, using the fully-qualified name.
    pub async fn data_exists_by_key(&self, fq_name: &str) -> Result<bool, ServalError> {
        let key = Manifest::make_manifest_key(fq_name);
        match cacache::Reader::open(&self.location, key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // TODO: probably should handle errors more granularly
        }
    }
}

#[async_trait]
impl RunnerStorage for &BlobStore {
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

    /// A non-streaming way to retrieve a stored compiled Wasm task. Prefer executable_as_stream() if you do not
    /// need the executable bytes in memory.
    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError> {
        let key = Manifest::make_executable_key(name, version);
        let binary: Vec<u8> = cacache::read(&self.location, key).await?;
        Ok(binary)
    }

    /// Retrieve a list of all Wasm manifests stored on this node.
    async fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        let result: Vec<String> = cacache::list_sync(&self.location)
            .filter(|xs| xs.is_ok())
            .map(|xs| xs.unwrap().key)
            .filter(|xs| xs.contains("manifest"))
            .collect();
        Ok(result)
    }

    /// Store a Wasm manifest. Returns the integrity checksum.
    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError> {
        let toml = toml::to_string(manifest)?;
        let meta_sri = cacache::write(&self.location, manifest.manifest_key(), &toml).await?;
        Ok(meta_sri)
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
}

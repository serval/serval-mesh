use async_trait::async_trait;
use ssri::Integrity;
use tokio_util::io::ReaderStream;

use utils::errors::ServalError;
use utils::structs::Manifest;

pub mod blobs;
pub use blobs::*;

pub mod proxy;
pub use proxy::*;

#[async_trait]
pub trait Storage {
    type A;

    /// Fetch a manifest by its fully-qualified name.
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError>;

    /// Store a job type manifest. Returns the integrity checksum.
    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError>;

    /// Store a job with metadata and an executable for later use. Returns the integrity checksums for the pair.
    async fn store_manifest_and_executable(
        &self,
        manifest: &Manifest,
        executable: &[u8],
    ) -> Result<(Integrity, Integrity), ServalError>;

    /// Store an executable in our blob store by its fully-qualified manifest name and a version string.
    async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> Result<Integrity, ServalError>;

    /// Given a content address, return a read stream for the object stored there.
    /// Responds with an error if no object is found or if the address is invalid.
    async fn executable_by_sri(&self, address: &str) -> Result<ReaderStream<Self::A>, ServalError>;

    /// Fetch an executable by key as a read stream.
    async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ReaderStream<Self::A>, ServalError>;

    /// A non-streaming way to retrieve a stored blob. Prefer executable_as_stream() if you can.
    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError>;

    /// Checks if the given blob is in the content store, by its SRI string.
    async fn data_exists_by_hash(&self, address: &str) -> Result<bool, ServalError>;

    /// Checks if the given job type is present in our data store, using the fully-qualified name.
    async fn data_exists_by_key(&self, fq_name: &str) -> Result<bool, ServalError>;

    /// List all manifests stored in this cache.
    async fn manifest_names(&self) -> Result<Vec<String>, ServalError>;
}

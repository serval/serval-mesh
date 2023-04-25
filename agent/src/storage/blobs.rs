use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::pin::Pin;

use serde::Serialize;
use ssri::Integrity;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;
use utils::errors::{ServalError, ServalResult};

use super::SendableStream;

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
    pub fn new(location: PathBuf) -> ServalResult<Self> {
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

    /// Given a content address, return a read stream for the object stored there.
    /// Responds with an error if no object is found or if the address is invalid.
    pub async fn data_by_sri(
        &self,
        integrity: &Integrity,
    ) -> ServalResult<ReaderStream<SendableStream>> {
        let fd = cacache::Reader::open_hash(&self.location, integrity.clone()).await?;
        log::info!("got a file descriptor");
        let pinned: Pin<Box<dyn AsyncRead + Send + 'static>> = Box::pin(fd);
        let stream = ReaderStream::new(pinned);
        Ok(stream)
    }

    #[allow(dead_code)]
    /// Checks if the given blob is in the content store, by its SRI string.
    pub async fn data_exists_by_sri(&self, integrity: &Integrity) -> ServalResult<bool> {
        Ok(cacache::exists(&self.location, integrity).await)
    }

    /// Checks if the given job type is present in our data store, using the fully-qualified name.
    pub async fn data_exists_by_key(&self, key: &str) -> Result<bool, ServalError> {
        match cacache::Reader::open(&self.location, key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // TODO: probably should handle errors more granularly
        }
    }

    /// A non-streaming way to retrieve a stored data blob.. Prefer stream_by_key() if you do not
    /// need the bytes in memory.
    pub async fn data_by_key(&self, key: &str) -> ServalResult<Vec<u8>> {
        let binary: Vec<u8> = cacache::read(&self.location, key).await?;
        Ok(binary)
    }

    /// Fetch a data blob by key as a read stream.
    pub async fn stream_by_key(&self, key: &str) -> ServalResult<ReaderStream<SendableStream>> {
        let fd = cacache::Reader::open(&self.location, key).await?;
        let pinned: SendableStream = Box::pin(fd);
        let stream = ReaderStream::new(pinned);
        Ok(stream)
    }

    /// Store data in our blob store by key. Returns the integrity checksum.
    pub async fn store_by_key(&self, key: &str, bytes: &[u8]) -> ServalResult<Integrity> {
        let sri = cacache::write(&self.location, key, bytes).await?;
        Ok(sri)
    }
}

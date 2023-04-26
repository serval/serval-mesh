use std::path::PathBuf;
use std::pin::Pin;

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::config::Region;
use axum::body::StreamBody;
use bytes::Bytes;
use once_cell::sync::OnceCell;
use serval_client::ServalApiClient;
use ssri::Integrity;
use tokio::io::AsyncRead;
use tokio_util::io::{ReaderStream, StreamReader};
use utils::errors::{ServalError, ServalResult};
use utils::mesh::ServalRole;
use utils::structs::Manifest;

pub mod blobs;
pub use blobs::*;

pub mod bucket;
pub use bucket::S3Storage;

use crate::structures::MESH;

// A convenient alias for an often-used stream type.
type SendableStream = Pin<Box<dyn AsyncRead + Send + 'static>>;

/// Our fully-configured storage object, with all of its details hidden.
pub static STORAGE: OnceCell<Storage> = OnceCell::new();

/// Initialize our local storage and a proxy option if we have no storage ourselves.
pub async fn initialize(path: Option<PathBuf>) -> ServalResult<()> {
    let local = if let Some(blobpath) = path {
        match BlobStore::new(&blobpath) {
            Ok(v) => Some(v),
            Err(e) => {
                log::warn!(
                    "We requested a cacache store at {} but failed! error={e}",
                    blobpath.display()
                );
                None
            }
        }
    } else {
        None
    };

    let bucket = if let Ok(bucket_name) = std::env::var("STORAGE_BUCKET") {
        let region_provider = RegionProviderChain::first_try(
            std::env::var("AWS_DEFAULT_REGION").ok().map(Region::new),
        )
        .or_default_provider()
        .or_else(Region::new("us-east-2"));
        let config = aws_config::from_env().region(region_provider).load().await;
        let bucket = S3Storage::new(&bucket_name, config)?;
        log::info!("s3 storage bucket enabled at {bucket_name}");
        Some(bucket)
    } else {
        None
    };

    let store = Storage::new(bucket, local);
    STORAGE.set(store).unwrap();
    Ok(())
}

/// This struct holds all the logic for juggling our three different ways of persisting data.
///
/// If it has no storage options configured, it immediately proxies all reads and writes to
/// a freshly-discovered peer that advertises the role. If it has any storage configured,
/// it will never attempt to proxy, lest we proxy in infinite loops.
///
/// If the operation is a read operation, it tries local storage first then falls back to s3 storage
/// if that is available. If it's a write operation, it will always try all configured options.
#[derive(Debug, Clone)]
pub struct Storage {
    bucket: Option<S3Storage>,
    local: Option<BlobStore>,
}

impl Storage {
    pub fn new(bucket: Option<S3Storage>, local: Option<BlobStore>) -> Self {
        Self { bucket, local }
    }

    fn has_storage(&self) -> bool {
        self.bucket.is_some() || self.local.is_some()
    }

    // This implementation is just a bunch of painful by-hand delegation logic.
    // I'd like to golf it down.

    pub async fn data_by_sri(
        &self,
        integrity: Integrity,
    ) -> ServalResult<StreamBody<ReaderStream<SendableStream>>> {
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            let bytes = proxy.data_by_sri(&integrity.to_string()).await?;
            let reader = ReaderStream::new(vec_to_byte_stream(bytes));
            return Ok(StreamBody::new(reader));
        }

        if let Some(local) = &self.local {
            if let Ok(v) = local.data_by_integrity(&integrity).await {
                log::info!("serving from local blobs; {integrity}");
                return Ok(StreamBody::new(v));
            }
        }

        if let Some(bucket) = &self.bucket {
            if let Ok(bytestream) = bucket.data_by_integrity(&integrity).await {
                log::info!("serving from s3 bucket; {integrity}");
                let readable = bytestream.into_async_read();
                let pinned: SendableStream = Box::pin(readable);
                let rs = ReaderStream::new(pinned);
                return Ok(StreamBody::new(rs));
            }
        }

        Err(ServalError::DataNotFound(integrity.to_string()))
    }

    /// Check if the given manifest is present in our store, using the fully-qualified name.
    ///
    /// Never checks a proxy; this is intended to be a local check.
    pub async fn data_exists_by_sri(&self, integrity: &Integrity) -> ServalResult<bool> {
        if let Some(local) = &self.local {
            if let Ok(_v) = local.data_exists_by_integrity(integrity).await {
                return Ok(true);
            }
        }

        if let Some(bucket) = &self.bucket {
            if let Ok(_v) = bucket.data_exists_by_key(&integrity.to_string()).await {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if the given manifest is present in our store, using the fully-qualified name.
    ///
    /// Never checks a proxy; this is intended to be a local check.
    pub async fn data_exists_by_key(&self, fq_name: &str) -> ServalResult<bool> {
        let key = Manifest::make_manifest_key(fq_name);

        // If we make a successful local check, we return only if we found it.
        // We're going to fall back to bucket storage if we have it.
        if let Some(local) = &self.local {
            if let Ok(v) = local.data_exists_by_key(&key).await {
                if v {
                    return Ok(v);
                }
            }
        }

        if let Some(bucket) = &self.bucket {
            if let Ok(v) = bucket.data_exists_by_key(&key).await {
                return Ok(v);
            }
        }

        Ok(false)
    }

    /// Fetch a manifest by its fully-qualified name.
    pub async fn manifest(&self, fq_name: &str) -> ServalResult<Manifest> {
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            return proxy.get_manifest(fq_name).await;
        }

        let key = Manifest::make_manifest_key(fq_name);

        if let Some(local) = &self.local {
            if let Ok(bytes) = local.data_by_key(&key).await {
                if let Ok(data) = String::from_utf8(bytes) {
                    let manifest: Manifest = toml::from_str(&data)?;
                    return Ok(manifest);
                }
            }
        }

        if let Some(bucket) = &self.bucket {
            if let Ok(bytes) = bucket.data_by_key(&key).await {
                if let Ok(data) = String::from_utf8(bytes) {
                    let manifest: Manifest = toml::from_str(&data)?;
                    return Ok(manifest);
                }
            }
        }

        Err(ServalError::ManifestNotFound(fq_name.to_string()))
    }

    /// Store a Wasm manifest. Returns the integrity checksum.
    pub async fn store_manifest(&self, manifest: &Manifest) -> ServalResult<Integrity> {
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            return proxy.store_manifest(manifest).await;
        }

        let toml = toml::to_string(manifest)?;
        let key = manifest.manifest_key();

        let local_result = if let Some(local) = &self.local {
            Some(local.store_by_key(&key, toml.as_bytes()).await)
        } else {
            None
        };

        let bucket_result = if let Some(bucket) = &self.bucket {
            Some(bucket.store_by_key(&key, toml.as_bytes()).await)
        } else {
            None
        };

        // Consider comparing integrity hashes.

        if let Some(result) = local_result {
            result
        } else if let Some(result) = bucket_result {
            result
        } else {
            Err(ServalError::StorageError(format!(
                "all storage attempts failed for manifest {}",
                manifest.fq_name()
            )))
        }
    }

    /// Fetch an executable by key as a read stream.
    pub async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> ServalResult<StreamBody<ReaderStream<SendableStream>>> {
        // Here we do gear changing to shift the disparate types from the various
        // clients into the singular type that the agent callers expect.
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            let bytes = proxy.get_executable(name, version).await?;
            let reader = ReaderStream::new(vec_to_byte_stream(bytes));
            return Ok(StreamBody::new(reader));
        }

        let key = Manifest::make_executable_key(name, version);

        if let Some(local) = &self.local {
            match local.stream_by_key(&key).await {
                Ok(reader) => {
                    let body = StreamBody::new(reader);
                    return Ok(body);
                }
                Err(e) => {
                    log::info!("error reading blob storage; key={name}@{version}; {e:?}");
                }
            }
        }

        if let Some(bucket) = &self.bucket {
            match bucket.stream_by_key(&key).await {
                Ok(bytestream) => {
                    let readable = bytestream.into_async_read();
                    let pinned: SendableStream = Box::pin(readable);
                    let rs = ReaderStream::new(pinned);
                    let body = StreamBody::new(rs);
                    return Ok(body);
                }
                Err(e) => {
                    log::info!("error reading bucket storage; key={name}@{version}; {e:?}");
                }
            }
        }

        Err(ServalError::ExecutableNotFound(format!("{name}@{version}")))
    }

    /// Fetch the bytes of the named executable so we can run it.
    pub async fn executable_as_bytes(&self, name: &str, version: &str) -> ServalResult<Vec<u8>> {
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            return proxy.get_executable(name, version).await;
        }

        let key = Manifest::make_executable_key(name, version);

        if let Some(local) = &self.local {
            if let Ok(v) = local.data_by_key(&key).await {
                return Ok(v);
            }
        }

        if let Some(bucket) = &self.bucket {
            if let Ok(v) = bucket.data_by_key(&key).await {
                return Ok(v);
            }
        }

        Err(ServalError::ExecutableNotFound(format!("{name}@{version}")))
    }

    /// Store an executable in the target node's blob store by its fully-qualified
    /// manifest name and a version string.
    pub async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> ServalResult<Integrity> {
        if !self.has_storage() {
            let proxy = make_proxy_client().await?;
            return proxy.store_executable(name, version, bytes.to_vec()).await;
        }

        let key = Manifest::make_executable_key(name, version);
        let local_result = if let Some(local) = &self.local {
            Some(local.store_by_key(&key, bytes).await)
        } else {
            None
        };

        let bucket_result = if let Some(bucket) = &self.bucket {
            Some(bucket.store_by_key(&key, bytes).await)
        } else {
            None
        };

        if let Some(result) = local_result {
            result
        } else if let Some(result) = bucket_result {
            result
        } else {
            Err(ServalError::StorageError(format!(
                "all storage attempts failed for executable {}@{}",
                name, version
            )))
        }
    }
}

// Convenience function to make a proxy client for a freshly-selected peer.
async fn make_proxy_client() -> ServalResult<ServalApiClient> {
    let mesh = MESH.get().expect("Peer network not initialized!"); // yes, we crash in this case
    let peers = mesh.peers_with_role(&ServalRole::Storage).await;
    let iter = peers.iter();
    for peer in iter {
        if let Some(addr) = peer.http_address() {
            let proxy = ServalApiClient::new_with_version(1, addr.to_string());
            return Ok(proxy);
        }
    }
    // If we get here we have utterly failed and cannot continue, but crashing might not be right.
    Err(ServalError::StorageError(
        "We were unable to find any peers with the storage role on this mesh.".to_string(),
    ))
}

// Convenience function used by executable_as_stream().
fn vec_to_byte_stream(bytes: Vec<u8>) -> SendableStream {
    let stream = futures::stream::iter(
        bytes
            .into_iter()
            .map(|xs| Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&[xs]))),
    );
    let sr = StreamReader::new(stream);

    Box::pin(sr)
}

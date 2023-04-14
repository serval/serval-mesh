use std::path::PathBuf;

use async_trait::async_trait;
use once_cell::sync::OnceCell;

use ssri::Integrity;
use utils::errors::ServalError;
use utils::mesh::ServalRole;
use utils::structs::Manifest;

pub mod blobs;
pub use blobs::*;

pub mod proxy;
pub use proxy::*;

use crate::structures::MESH;

/// If a node has local storage, this cell contains it.
pub static STORAGE: OnceCell<BlobStore> = OnceCell::new();

/// Initialize our local storage.
pub fn initialize(path: PathBuf) -> Result<(), ServalError> {
    let store = BlobStore::new(path)?;
    STORAGE.set(store).unwrap();
    Ok(())
}

/// The job runner calls this to get a storage implementation appropriate for the node.
/// If we have local storage, we use that. If we do not, we discover a peer that does have
/// storage and ask it for the data. We don't cache the peer client because it's both cheap
/// to discover and we would prefer to get one from our most recent idea of who our peers are.
pub async fn get_runner_storage() -> Result<Box<dyn RunnerStorage + Send + Sync>, ServalError> {
    match STORAGE.get() {
        Some(v) => Ok(Box::new(v)),
        None => {
            let mesh = MESH.get().expect("Peer network not initialized!"); // yes, we crash in this case
            let peers = mesh.peers_with_role(&ServalRole::Storage).await;
            let iter = peers.iter();
            for peer in iter {
                if let Some(addr) = peer.http_address() {
                    let storage = StorageProxy::new(1, addr);
                    return Ok(Box::new(storage));
                }
            }
            // If we get here we have utterly failed and cannot continue, but crashing might not be right.
            Err(ServalError::StorageError(
                "We were unable to find any peers with the storage role on this mesh.".to_string(),
            ))
        }
    }
}

/// This trait expresses the duties of a storage implementation that meets the requirments of
/// a Wasm job runner node.
#[async_trait]
pub trait RunnerStorage {
    /// Fetch a manifest by its fully-qualified name.
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError>;

    /// Fetch the bytes of the named executable so we can run it.
    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError>;

    // The following three functions are speculative implementations of things that
    // that runner nodes do not need today, but that were easy to implement just in case.

    /// Store a Wasm manifest. Returns the integrity checksum.
    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError>;

    /// Store an executable in the target node's blob store by its fully-qualified
    /// manifest name and a version string.
    async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> Result<Integrity, ServalError>;

    /// Retrieve a list of all Wasm manifests stored on the target node.
    async fn manifest_names(&self) -> Result<Vec<String>, ServalError>;
}

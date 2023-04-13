use std::path::PathBuf;

use async_trait::async_trait;
use once_cell::sync::OnceCell;

use utils::errors::ServalError;
use utils::mesh::ServalRole;
use utils::structs::Manifest;

pub mod blobs;
pub use blobs::*;

pub mod proxy;
pub use proxy::*;

use crate::structures::MESH;

pub static STORAGE: OnceCell<BlobStore> = OnceCell::new();

pub fn initialize(path: PathBuf) -> Result<(), ServalError> {
    let store = BlobStore::new(path)?;
    STORAGE.set(store).unwrap();
    Ok(())
}

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

#[async_trait]
pub trait RunnerStorage {
    /// Fetch a manifest by its fully-qualified name.
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError>;

    /// A non-streaming way to retrieve a stored blob. Prefer executable_as_stream() if you can.
    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError>;
}

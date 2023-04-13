use std::net::SocketAddr;

// Write to figure out how it should work, then make it vanish.
use async_trait::async_trait;

use serval_client::ServalApiClient;
use ssri::Integrity;
use utils::errors::ServalError;
use utils::structs::Manifest;

use crate::storage::RunnerStorage;

#[derive(Debug, Clone)]
pub struct StorageProxy {
    client: ServalApiClient,
}

impl StorageProxy {
    /// Create a new proxy to a peer at the specified address, using the requested API version.
    pub fn new(version: u8, address: SocketAddr) -> Self {
        Self {
            client: ServalApiClient::new_with_version(version, address.to_string()),
        }
    }
}

#[async_trait]
impl RunnerStorage for StorageProxy {
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError> {
        self.client.get_manifest(fq_name).await
    }

    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError> {
        self.client.get_executable(name, version).await
    }

    // The following functions are speculative implementations of reaching out
    // to a peer for data we do not have locally. We don't use any of these at the
    // moment, but we might need to.

    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError> {
        let integrity = self.client.store_manifest(manifest).await?;
        Ok(integrity)
    }

    async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> Result<Integrity, ServalError> {
        self.client
            .store_executable(name, version, bytes.to_vec())
            .await
    }

    async fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        self.client.list_manifests().await
    }
}

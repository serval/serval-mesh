use std::net::SocketAddr;

// Write to figure out how it should work, then make it vanish.
use async_trait::async_trait;

use serval_client::ServalApiClient;
use utils::errors::ServalError;
use utils::structs::Manifest;

use crate::storage::RunnerStorage;

#[derive(Debug, Clone)]
pub struct StorageProxy {
    client: ServalApiClient,
}

impl StorageProxy {
    pub fn new(version: u8, address: SocketAddr) -> Self {
        Self {
            client: ServalApiClient::new_with_version(version, address.to_string()),
        }
    }
    /*

    pub async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError> {
        let integrity = self.client.store_manifest(manifest).await?;
        Ok(integrity)
    }

    pub async fn store_manifest_and_executable(
        &self,
        manifest: &Manifest,
        executable: &[u8],
    ) -> Result<(Integrity, Integrity), ServalError> {
        let m_integrity = self.client.store_manifest(manifest).await?;
        let e_integrity = self
            .client
            .store_executable(&manifest.fq_name(), manifest.version(), executable.to_vec())
            .await?;

        Ok((m_integrity, e_integrity))
    }

    pub async fn store_executable(
        &self,
        name: &str,
        version: &str,
        bytes: &[u8],
    ) -> Result<Integrity, ServalError> {
        self.client
            .store_executable(name, version, bytes.to_vec())
            .await
    }

    pub async fn executable_by_sri(
        &self,
        _address: &str,
    ) -> Result<Pin<Box<dyn AsyncRead>>, ServalError> {
        // TODO needs to be exposed in the api first
        // This function is NOT USED in api implementation. (consider removing)
        todo!()
    }

    pub async fn data_exists_by_hash(&self, _address: &str) -> Result<bool, ServalError> {
        // This needs to be exposed in an API to be proxied
        // This function is NOT USED in api implementation. (consider removing)
        todo!()
    }

    pub async fn data_exists_by_key(&self, _fq_name: &str) -> Result<bool, ServalError> {
        // This needs to be exposed in an API to be proxied
        // This function IS USED in api implementation.
        todo!()
    }

    pub async fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        self.client.list_manifests().await
    }
    */
}

#[async_trait]
impl RunnerStorage for StorageProxy {
    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError> {
        self.client.get_manifest(fq_name).await
    }

    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError> {
        self.client.get_executable(name, version).await
    }
}

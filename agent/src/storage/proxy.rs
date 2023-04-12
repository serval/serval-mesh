use std::net::SocketAddr;

// Write to figure out how it should work, then make it vanish.
use async_trait::async_trait;
use bytes::Bytes;
use ssri::Integrity;
use tokio_util::io::ReaderStream;

use std::io::Cursor;

use serval_client::ServalApiClient;
use utils::errors::ServalError;
use utils::structs::Manifest;

use super::Storage;

struct StorageProxy {
    client: ServalApiClient,
}

impl StorageProxy {
    fn new(version: u8, address: SocketAddr) -> Self {
        Self {
            client: ServalApiClient::new_with_version(version, address.to_string()),
        }
    }
}

#[async_trait]
impl Storage for StorageProxy {
    type A = Cursor<Bytes>;

    async fn manifest(&self, fq_name: &str) -> Result<Manifest, ServalError> {
        self.client.get_manifest(fq_name).await
    }

    async fn store_manifest(&self, manifest: &Manifest) -> Result<Integrity, ServalError> {
        let integrity = self.client.store_manifest(manifest).await?;
        Ok(integrity)
    }

    async fn store_manifest_and_executable(
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

    async fn executable_by_sri(
        &self,
        _address: &str,
    ) -> Result<ReaderStream<Self::A>, ServalError> {
        // TODO needs to be exposed in the api first

        todo!()
    }

    async fn executable_as_stream(
        &self,
        name: &str,
        version: &str,
    ) -> Result<ReaderStream<Cursor<Bytes>>, ServalError> {
        let reader = self.client.get_executable_as_stream(name, version).await?;

        Ok(reader)
    }

    async fn executable_as_bytes(&self, name: &str, version: &str) -> Result<Vec<u8>, ServalError> {
        self.client.get_executable(name, version).await
    }

    async fn data_exists_by_hash(&self, _address: &str) -> Result<bool, ServalError> {
        // This needs to be exposed in an API to be proxied
        todo!()
    }

    async fn data_exists_by_key(&self, _fq_name: &str) -> Result<bool, ServalError> {
        // This needs to be exposed in an API to be proxied
        todo!()
    }

    async fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        self.client.list_manifests().await
    }
}

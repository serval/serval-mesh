// Write to figure out how it should work, then make it vanish.
use async_trait::async_trait;
use axum::extract::BodyStream;
use ssri::Integrity;
use tokio_util::io::ReaderStream;

use utils::errors::ServalError;
use utils::structs::Manifest;

use super::Storage;

struct StorageProxy {}

/*
super::proxy::relay_request(&mut request, &ServalRole::Storage, &state.instance_id).await
 */

#[async_trait]
impl Storage for StorageProxy {
    type A = BodyStream;

    async fn manifest(&self, _fq_name: &str) -> Result<Manifest, ServalError> {
        todo!()
    }

    async fn store_manifest(&self, _manifest: &Manifest) -> Result<Integrity, ServalError> {
        todo!()
    }

    async fn store_manifest_and_executable(
        &self,
        _manifest: &Manifest,
        _executable: &[u8],
    ) -> Result<(Integrity, Integrity), ServalError> {
        todo!()
    }

    async fn store_executable(
        &self,
        _name: &str,
        _version: &str,
        _bytes: &[u8],
    ) -> Result<String, ServalError> {
        todo!()
    }

    async fn executable_by_sri(
        &self,
        _address: &str,
    ) -> Result<ReaderStream<Self::A>, ServalError> {
        todo!()
    }

    async fn executable_as_stream(
        &self,
        _name: &str,
        _version: &str,
    ) -> Result<ReaderStream<Self::A>, ServalError> {
        todo!()
    }

    async fn executable_as_bytes(
        &self,
        _name: &str,
        _version: &str,
    ) -> Result<Vec<u8>, ServalError> {
        todo!()
    }

    async fn data_exists_by_hash(&self, _address: &str) -> Result<bool, ServalError> {
        todo!()
    }

    async fn data_exists_by_key(&self, _fq_name: &str) -> Result<bool, ServalError> {
        todo!()
    }

    fn manifest_names(&self) -> Result<Vec<String>, ServalError> {
        todo!()
    }
}

#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use std::time::Duration;

use reqwest::{Response, StatusCode};
use ssri::Integrity;
use utils::errors::ServalError;
use utils::mesh::{PeerMetadata, ServalRole};
use utils::structs::Manifest;

type ApiResult<T> = Result<T, ServalError>;
type JsonObject = serde_json::Map<String, serde_json::Value>;

/// A client for the Serval API.
#[derive(Debug, Clone)]
pub struct ServalApiClient {
    version: u8,
    socket_addr: String,
}

impl ServalApiClient {
    /// Create a new client for the peer node pointed to by the address, using the most recent API version.
    pub fn new(socket_addr: String) -> Self {
        Self {
            version: 1, // magic number, yes it is
            socket_addr,
        }
    }

    /// Create a new client for the peer node pointed to by the address, using the specified API version.
    pub fn new_with_version(version: u8, socket_addr: String) -> Self {
        Self {
            version,
            socket_addr,
        }
    }

    /// Ping whichever node we're pointing to.
    pub async fn ping(&self) -> ApiResult<String> {
        // This url is not versioned.
        let url = format!("http://{}/monitor/ping", self.socket_addr);
        let response = reqwest::get(&url).await?;
        let body = response.text().await?;

        Ok(body)
    }

    /// Get monitoring status from whatever node we're pointing to.
    pub async fn monitor_status(&self) -> ApiResult<JsonObject> {
        // This url is not versioned.
        let url = format!("http://{}/monitor/status", self.socket_addr);
        let response = reqwest::get(&url).await?;
        let body: serde_json::Map<String, serde_json::Value> = response.json().await?;

        Ok(body)
    }

    /// List all running jobs.
    pub async fn list_jobs(&self) -> ApiResult<JsonObject> {
        let url = self.build_url("jobs");
        let response = reqwest::get(&url).await?;
        let body: JsonObject = response.json().await?;

        Ok(body)
    }

    /// Run a previously-stored Wasm job by its fully-qualified name. If the job
    /// needs input, send it in as a vec of bytes. Pass a zero-length vec if the
    /// job doesn't need input.
    pub async fn run_job(&self, name: &str, input: Vec<u8>) -> ApiResult<Response> {
        let url = self.build_url(&format!("jobs/{name}/run"));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        // TODO: this is a cop-out for the moment, because the cli does a lot with the response object.
        // We *should* respond with WasmResult.
        let response = client.post(url).body(input).send().await?;
        Ok(response)
    }

    /// Get a list of all peers the node is aware of.
    pub async fn all_peers(&self) -> ApiResult<Vec<PeerMetadata>> {
        let url = self.build_url("mesh/peers");
        let response = reqwest::get(&url).await?;
        let body: Vec<PeerMetadata> = response.json().await?;

        Ok(body)
    }

    /// Get a list of all known peers advertising the given role.
    pub async fn peers_with_role(&self, role: ServalRole) -> ApiResult<Vec<PeerMetadata>> {
        let url = self.build_url(&format!("mesh/peers/{role}"));
        let response = reqwest::get(&url).await?;
        let body: Vec<PeerMetadata> = response.json().await?;

        Ok(body)
    }

    /// Store a Wasm manifest on the node.
    pub async fn store_manifest(&self, manifest: &Manifest) -> ApiResult<Integrity> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        let url = self.build_url("storage/manifests");
        let response = client.post(url).body(manifest.to_string()).send().await?;

        // StatusCode.CREATED  + ssri string
        if response.status().is_success() {
            let body = response.text().await?;
            let integrity: Integrity = body.parse()?;
            Ok(integrity)
        } else {
            Err(ServalError::StorageError(response.text().await?))
        }
    }

    /// Fetch a manifest from the node. The response will be *toml*, not json
    /// as you might expect, because manifests are canonically stored as toml.
    pub async fn get_manifest(&self, name: &str) -> ApiResult<Manifest> {
        let url = self.build_url(&format!("storage/manifests/{name}"));
        let response = reqwest::get(&url).await?;
        if response.status().is_success() {
            let text = response.text().await?;
            let manifest = Manifest::from_string(&text)?;
            Ok(manifest)
        } else {
            Err(ServalError::ManifestNotFound(response.text().await?))
        }
    }

    /// Check if this node has in its local storage the named manifest.
    pub async fn has_manifest(&self, name: &str) -> ApiResult<bool> {
        let url = self.build_url(&format!("storage/manifests/{name}"));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;

        let response = client.head(&url).send().await?;
        let found = matches!(response.status(), StatusCode::OK);
        Ok(found)
    }

    /// Store bytes for a pre-compiled Wasm executable. Note that we're not yet
    /// tracking architecture or anything else; it's YOLO if you built the wasm
    /// for something the target node can't run. (If we were done, we'd ship it.)
    pub async fn store_executable(
        &self,
        name: &str,
        version: &str,
        executable: Vec<u8>,
    ) -> ApiResult<Integrity> {
        let url = self.build_url(&format!("storage/manifests/{name}/executable/{version}"));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        let response = client.put(url).body(executable).send().await?;
        if response.status().is_success() {
            let body = response.text().await?;
            let integrity: Integrity = body.parse()?;
            Ok(integrity)
        } else {
            Err(ServalError::StorageError(response.text().await?))
        }
    }

    /// Fetch the bytes for the named Wasm executable.
    pub async fn get_executable(&self, name: &str, version: &str) -> ApiResult<Vec<u8>> {
        let url = self.build_url(&format!("storage/manifests/{name}/executable/{version}"));
        let response = reqwest::get(&url).await?;
        if response.status().is_success() {
            let executable = response.bytes().await?;
            Ok(executable.to_vec())
        } else {
            Err(ServalError::StorageError(response.text().await?))
        }
    }

    pub async fn stream_by_integrity(&self, address: &str) -> ApiResult<Vec<u8>> {
        let url = self.build_url(&format!("storage/data/{address}"));
        let response = reqwest::get(&url).await?;
        if response.status().is_success() {
            let bytes = response.bytes().await?;
            Ok(bytes.to_vec())
        } else {
            Err(ServalError::StorageError(response.text().await?))
        }
    }

    /// Store a blob of data in the content-addressable store on the targeted peer.
    pub async fn store_by_integrity(&self, bytes: Vec<u8>) -> ApiResult<Integrity> {
        let url = self.build_url("storage/data");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        let response = client.post(url).body(bytes).send().await?;
        if response.status().is_success() {
            let body = response.text().await?;
            let integrity: Integrity = body.parse()?;
            Ok(integrity)
        } else {
            Err(ServalError::StorageError(response.text().await?))
        }
    }

    // Convenience function to build urls repeatably.
    fn build_url(&self, path: &str) -> String {
        format!("http://{}/v{}/{path} ", self.socket_addr, self.version)
    }
}

#[cfg(test)]
mod tests {}

use anyhow::Result;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use http::header::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utils::{blobs::BlobStore, errors::ServalError};
use uuid::Uuid;

use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

pub mod jobs;
pub mod proxy;
pub mod storage;

#[derive(Debug, Clone, Serialize)]
pub struct RunnerState {
    instance_id: Uuid,
    storage: Option<BlobStore>,
    jobs: HashMap<String, JobMetadata>,
    total: usize,
    errors: usize,
}

impl RunnerState {
    pub fn new(instance_id: Uuid, blob_path: Option<PathBuf>) -> Result<Self, ServalError> {
        let storage = match blob_path {
            Some(path) => Some(BlobStore::new(path)?),
            None => None,
        };

        Ok(RunnerState {
            instance_id,
            storage,
            total: 0,
            errors: 0,
            jobs: HashMap::new(),
        })
    }
}

pub type AppState = Arc<Mutex<RunnerState>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobMetadata {
    id: Uuid,
    name: String,
    description: String,
    status_url: String, // for the moment
    result_url: String, // for the moment
}

impl JobMetadata {
    pub fn new(name: String, description: String) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            name,
            description,
            status_url: format!("/jobs/{}/status", id),
            result_url: format!("/jobs/{}/result", id),
        }
    }
}

impl From<Envelope> for JobMetadata {
    fn from(envelope: Envelope) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            name: envelope.name.clone(),
            description: envelope.description,
            status_url: format!("/jobs/{}/status", id),
            result_url: format!("/jobs/{}/result", id),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Envelope {
    name: String,
    description: String,
}

/// Remember what is important.
pub async fn clacks<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let mut response = next.run(req).await;
    if !response.headers().contains_key("X-Clacks-Overhead") {
        response.headers_mut().append(
            "X-Clacks-Overhead",
            HeaderValue::from_static("GNU/Terry Pratchett"),
        );
    }
    Ok(response)
}

/// Respond to ping. Useful for monitoring.
pub async fn ping() -> String {
    "pong".to_string()
}

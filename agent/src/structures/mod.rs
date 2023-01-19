use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utils::{blobs::BlobStore, errors::ServalError};
use uuid::Uuid;

use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

/// Our application state. Fields are public for now but we'll want to fix that.
#[derive(Debug, Clone, Serialize)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub storage: Option<BlobStore>,
    pub jobs: HashMap<String, JobMetadata>,
    pub total: usize,
    pub errors: usize,
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

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
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

impl Default for Envelope {
    fn default() -> Self {
        Self {
            name: "unknown".to_string(),
            description: "unknown job description".to_string(),
        }
    }
}

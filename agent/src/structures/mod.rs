use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;
use utils::{blobs::BlobStore, errors::ServalError, structs::Job};
use uuid::Uuid;

use std::fs;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

/// Our application state. Fields are public for now but we'll want to fix that.
#[derive(Debug, Clone, Serialize)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub extensions: HashMap<String, PathBuf>,
    pub storage: Option<BlobStore>,
    pub jobs: HashMap<String, Job>,
    pub total: usize,
    pub errors: usize,
    pub should_run_jobs: bool,
}

impl RunnerState {
    pub fn new(
        instance_id: Uuid,
        blob_path: Option<PathBuf>,
        extensions_path: Option<PathBuf>,
        should_run_jobs: bool,
    ) -> Result<Self, ServalError> {
        let storage = match blob_path {
            Some(path) => Some(BlobStore::new(path)?),
            None => None,
        };
        let extensions = if let Some(extensions_path) = extensions_path {
            // Read the contents of the directory at the given path and build a HashMap that maps
            // from the module's name (the filename minus the .wasm extension) to its path on disk.

            let mut extensions: HashMap<String, PathBuf> = HashMap::new();
            let dir_entries = fs::read_dir(&extensions_path)?;
            for entry in dir_entries {
                let Ok(entry) = entry else { continue };
                let filename = entry.file_name();
                let filename = filename.to_string_lossy();
                if !filename.to_lowercase().ends_with(".wasm") {
                    continue;
                }
                let module_name = &filename[0..filename.len() - ".wasm".len()];
                extensions.insert(module_name.to_string(), entry.path());
            }
            extensions
        } else {
            HashMap::new()
        };

        Ok(RunnerState {
            instance_id,
            storage,
            extensions,
            total: 0,
            errors: 0,
            jobs: HashMap::new(),
            should_run_jobs,
        })
    }
}

pub type AppState = Arc<Mutex<RunnerState>>;

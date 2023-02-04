use anyhow::Result;
use once_cell::sync::OnceCell;
use serde::Serialize;
use utils::{blobs::BlobStore, errors::ServalError};
use uuid::Uuid;

use std::fs;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

pub static STORAGE: OnceCell<BlobStore> = OnceCell::new();

/// Our application state. Fields are public for now but we'll want to fix that.
#[derive(Debug, Clone, Serialize)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub extensions: HashMap<String, PathBuf>,
    pub should_run_jobs: bool,
    pub has_storage: bool,
}

impl RunnerState {
    pub fn new(
        instance_id: Uuid,
        blob_path: Option<PathBuf>,
        extensions_path: Option<PathBuf>,
        should_run_jobs: bool,
    ) -> Result<Self, ServalError> {
        let has_storage = match blob_path {
            Some(path) => {
                let store = BlobStore::new(path)?;
                STORAGE.set(store).unwrap();
                true
            }
            None => false,
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
            extensions,
            should_run_jobs,
            has_storage,
        })
    }
}

pub type AppState = Arc<RunnerState>;

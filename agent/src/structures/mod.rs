use anyhow::Result;
use engine::extensions::{load_extensions, ServalExtension};
use once_cell::sync::OnceCell;
use utils::errors::ServalError;
use utils::mesh::ServalMesh;
use uuid::Uuid;

use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

pub static MESH: OnceCell<ServalMesh> = OnceCell::new();

pub type ServalRouter = axum::Router<Arc<RunnerState>, hyper::Body>;

/// Our application state. Fields are public for now but we'll want to fix that.
#[derive(Debug, Clone)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub extensions: HashMap<String, ServalExtension>,
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
                crate::storage::initialize(path)?;
                true
            }
            None => false,
        };

        let extensions = extensions_path
            .and_then(|extensions_path| {
                load_extensions(&extensions_path)
                    .map_err(|err| {
                        log::warn!(
                            "Failed to load extensions; path={extensions_path:?}, err={err:?}"
                        );
                        err
                    })
                    .ok()
            })
            .unwrap_or_default();

        Ok(RunnerState {
            instance_id,
            extensions,
            should_run_jobs,
            has_storage,
        })
    }
}

pub type AppState = Arc<RunnerState>;

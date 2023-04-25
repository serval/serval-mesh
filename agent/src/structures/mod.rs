use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use engine::extensions::{load_extensions, ServalExtension};
use once_cell::sync::OnceCell;
use utils::errors::ServalError;
use utils::mesh::ServalMesh;
use uuid::Uuid;

pub static MESH: OnceCell<ServalMesh> = OnceCell::new();

pub type ServalRouter = axum::Router<Arc<RunnerState>, hyper::Body>;

/// Our application state. Fields are public for now but we'll want to fix that.
#[derive(Debug, Clone)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub extensions: HashMap<String, ServalExtension>,
    pub should_run_jobs: bool,
    pub should_run_scheduler: bool,
    pub has_storage: bool,
}

impl RunnerState {
    pub async fn new(
        instance_id: Uuid,
        blob_path: Option<PathBuf>,
        extensions_path: Option<PathBuf>,
        should_run_jobs: bool,
        should_run_scheduler: bool,
    ) -> Result<Self, ServalError> {
        let has_storage = blob_path.is_some();
        crate::storage::initialize(blob_path).await?;

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
            should_run_scheduler,
            has_storage,
        })
    }
}

pub type AppState = Arc<RunnerState>;

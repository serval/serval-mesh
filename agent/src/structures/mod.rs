use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use engine::extensions::{load_extensions, ServalExtension};
use gethostname::gethostname;
use once_cell::sync::OnceCell;
use serde::Serialize;
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
    pub start_timestamp: Instant,
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
            start_timestamp: Instant::now(),
        })
    }
}

pub type AppState = Arc<RunnerState>;

/// Agent metadata.
#[derive(Serialize)]
struct BuildInfo {
    build_timestamp: String,
    build_date: String,
    git_branch: String,
    git_timestamp: String,
    git_date: String,
    git_hash: String,
    git_describe: String,
    rustc_host_triple: String,
    rustc_version: String,
    cargo_target_triple: String,
}

impl BuildInfo {
    fn new() -> BuildInfo {
        BuildInfo {
            build_timestamp: String::from(env!("VERGEN_BUILD_TIMESTAMP")),
            build_date: String::from(env!("VERGEN_BUILD_DATE")),
            git_branch: String::from(env!("VERGEN_GIT_BRANCH")),
            git_timestamp: String::from(env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
            git_date: String::from(env!("VERGEN_GIT_COMMIT_DATE")),
            git_hash: String::from(env!("VERGEN_GIT_SHA")),
            git_describe: String::from(env!("VERGEN_GIT_DESCRIBE")),
            rustc_host_triple: String::from(env!("VERGEN_RUSTC_HOST_TRIPLE")),
            rustc_version: String::from(env!("VERGEN_RUSTC_SEMVER")),
            cargo_target_triple: String::from(env!("VERGEN_CARGO_TARGET_TRIPLE")),
        }
    }
}

/// Agent metadata.
#[derive(Serialize)]
pub struct AgentInfo {
    hostname: String,
    instance_id: Uuid,
    uptime: f64,
    build_info: BuildInfo,
}

impl AgentInfo {
    pub fn new(state: &AppState) -> AgentInfo {
        AgentInfo {
            hostname: gethostname().into_string().expect("Failed to get hostname"),
            instance_id: state.instance_id,
            uptime: state.start_timestamp.elapsed().as_secs_f64(),
            build_info: BuildInfo::new(),
        }
    }
}

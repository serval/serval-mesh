use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use engine::extensions::{load_extensions, ServalExtension};
use once_cell::sync::OnceCell;
use utils::errors::ServalError;
use utils::mesh::{PeerMetadata, ServalMesh};
use utils::structs::JobStatus;
use uuid::Uuid;

pub static MESH: OnceCell<ServalMesh> = OnceCell::new();
pub static JOBS: OnceCell<Arc<Mutex<JobQueue>>> = OnceCell::new();

pub type ServalRouter = axum::Router<Arc<RunnerState>, hyper::Body>;

/// Our application state. Fields are public for now but we'll want to fix that.
// todo: rename this to NodeState or something
#[derive(Debug, Clone)]
pub struct RunnerState {
    pub instance_id: Uuid,
    pub extensions: HashMap<String, ServalExtension>,
    pub should_run_jobs: bool,
    pub should_run_scheduler: bool,
    pub has_storage: bool,
}

impl RunnerState {
    pub fn new(
        instance_id: Uuid,
        blob_path: Option<PathBuf>,
        extensions_path: Option<PathBuf>,
        should_run_jobs: bool,
        should_run_scheduler: bool,
    ) -> Result<Self, ServalError> {
        if should_run_scheduler {
            JOBS.set(Arc::new(Mutex::new(JobQueue::new()))).unwrap();
        }

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
            should_run_scheduler,
            has_storage,
        })
    }
}

pub type AppState = Arc<RunnerState>;

#[derive(Clone, Debug)]
pub struct Job {
    // todo: keep track of when a job was claimed so we can expire and retry
    // todo: keep track of attempts so we can give up if it fails too many time
    id: Uuid,
    status: JobStatus,
    name: String,
    input: Vec<u8>,  // todo should be an owned Bytes, probably
    output: Vec<u8>, // todo should be an owned Bytes, probably
}

impl Job {
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn status(&self) -> &JobStatus {
        &self.status
    }
}

#[derive(Debug)]
pub struct JobQueue {
    jobs: Vec<Job>,
    workers: HashSet<PeerMetadata>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            jobs: vec![],
            workers: HashSet::new(),
        }
    }
}

impl JobQueue {
    pub fn get_job(&self, job_id: Uuid) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == job_id)
    }

    pub fn enqueue(&mut self, name: String, input: Vec<u8>) -> Result<Uuid> {
        let job = Job {
            id: Uuid::new_v4(),
            status: JobStatus::Pending,
            name,
            input,
            output: vec![],
        };
        let job_id = job.id.clone();

        self.jobs.push(job);

        Ok(job_id)
    }

    pub fn complete(&mut self, id: Uuid, status: JobStatus, output: Vec<u8>) -> Result<()> {
        todo!();
        Ok(())
    }

    pub fn claim(&mut self) -> Option<Job> {
        for job in self.jobs.iter_mut() {
            if job.status == JobStatus::Pending {
                job.status = JobStatus::Active;
                return Some(job.clone());
            }
        }

        None
    }

    pub fn tickle(&mut self, id: Uuid) -> Result<()> {
        todo!();
        Ok(())
    }

    pub fn register_worker(&mut self, worker: PeerMetadata) -> Result<()> {
        self.workers.insert(worker);
        Ok(())
    }
}

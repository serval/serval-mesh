pub mod v1;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Mutex;

use crate::api::v1::AxumState;
use crate::queue::JobQueue;

/// Initialize an HTTP service. Set up all routes.
pub async fn init_http(host: &str, port: u16, job_queue_filename: PathBuf) -> anyhow::Result<()> {
    let state = AxumState {
        job_queue: Arc::new(Mutex::new(JobQueue::new(Some(job_queue_filename)))),
    };

    let app = Router::new()
        .route("/ping", get(v1::ping))
        .route("/jobs/create", post(v1::create))
        .route("/jobs/claim", post(v1::claim))
        .route("/jobs/:job_id", get(v1::get))
        .route("/jobs/:job_id/tickle", post(v1::tickle))
        .route("/jobs/:job_id/complete", post(v1::complete))
        .with_state(state);

    let addr = format!("{host}:{port}");
    log::info!("Job queue service about to listen on http://{addr}");
    let addr: SocketAddr = addr.parse()?;
    match axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}

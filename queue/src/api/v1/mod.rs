use std::sync::Arc;

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::queue::{Job, JobQueue};

// follow this pattern for endpoint groups
// pub mod <filename>;
// pub use <filename>::*;

/// Respond to ping. Useful for liveness checks.
pub async fn ping() -> &'static str {
    "pong"
}

#[derive(Debug, Deserialize)]
pub struct JobsCreateRequest {
    binary_addr: String,
    input_addr: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct JobsCreateResponse {
    job_id: String,
}

#[derive(Clone)]
pub struct AxumState {
    pub job_queue: Arc<Mutex<JobQueue>>,
}

type JsonHandlerResult<T> = Result<Json<T>, Response>;

pub async fn create(
    State(state): State<AxumState>,
    Json(payload): Json<JobsCreateRequest>,
) -> JsonHandlerResult<JobsCreateResponse> {
    let mut job_queue = state.job_queue.lock().await;

    // TODO: validate that payload.binary_addr and payload.input_addr are valid-looking addresses.
    // Ideally this would happen at the Serde level but I don't see a way to implement constraints
    // on string fields. My best idea so far would be to make these fields into [u8;40] and tell
    // Serde to represent them as hex strings when in JSON format. This seems not ideal.
    log::info!("Enqueueing job {payload:?}");
    let Ok(job_id) = job_queue.enqueue_job(payload.binary_addr, payload.input_addr) else {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, String::from("Failed to enqueue job")).into_response());
    };

    Ok(Json(JobsCreateResponse {
        job_id: job_id.to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct JobsClaimRequest {
    runner_id: Uuid,
}

pub async fn claim(
    State(state): State<AxumState>,
    Json(payload): Json<JobsClaimRequest>,
) -> JsonHandlerResult<Job> {
    let mut job_queue = state.job_queue.lock().await;
    job_queue.detect_abandoned_jobs();

    let Some(job) = job_queue.claim_job(&payload.runner_id) else {
        return Err((StatusCode::NO_CONTENT, String::from("No pending jobs")).into_response());
    };

    Ok(Json(job))
}

pub async fn tickle(
    Path(job_id): Path<Uuid>,
    State(state): State<AxumState>,
) -> JsonHandlerResult<Value> {
    let mut job_queue = state.job_queue.lock().await;
    job_queue.detect_abandoned_jobs();

    let Ok(()) = job_queue.tickle_job(&job_id) else {
        return Err((StatusCode::BAD_REQUEST, String::from("Job does not exist or is not active")).into_response());
    };

    Ok(Json(json!({})))
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub enum JobCompletionStatus {
    Completed,
    Failed,
}
#[derive(Debug, Deserialize)]
pub struct JobsCompleteRequest {
    status: JobCompletionStatus,
    output_addr: Option<String>,
}
pub async fn complete(
    State(state): State<AxumState>,
    Path(job_id): Path<Uuid>,
    Json(payload): Json<JobsCompleteRequest>,
) -> JsonHandlerResult<Value> {
    let mut job_queue = state.job_queue.lock().await;
    let res = match payload.status {
        JobCompletionStatus::Failed => job_queue.fail_job(&job_id, &payload.output_addr),
        JobCompletionStatus::Completed => job_queue.complete_job(&job_id, &payload.output_addr),
    };
    let Ok(()) = res else {
        return Err((StatusCode::BAD_REQUEST, String::from("Job does not exist or is not active")).into_response());
    };

    Ok(Json(json!({})))
}

pub async fn get(
    Path(job_id): Path<Uuid>,
    State(state): State<AxumState>,
) -> JsonHandlerResult<Job> {
    let mut job_queue = state.job_queue.lock().await;
    job_queue.detect_abandoned_jobs();

    let Ok(job) = job_queue.get_job(&job_id) else {
        return Err((StatusCode::NOT_FOUND, String::from("Job does not exist")).into_response());
    };

    Ok(Json(job))
}

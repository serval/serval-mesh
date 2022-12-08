use axum::{
    extract::{Json, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::queue::{claim_job, complete_job, enqueue_job, fail_job, get_job, tickle_job, Job};

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

type JsonHandlerResult<T> = Result<Json<T>, Response>;

pub async fn create(
    Json(payload): Json<JobsCreateRequest>,
) -> JsonHandlerResult<JobsCreateResponse> {
    // TODO: validate that payload.binary_addr and payload.input_addr are valid-looking addresses.
    // Ideally this would happen at the Serde level but I don't see a way to implement constraints
    // on string fields. My best idea so far would be to make these fields into [u8;40] and tell
    // Serde to represent them as hex strings when in JSON format. This seems not ideal.
    log::info!("Enqueueing job {payload:?}");
    let Ok(job_id) = enqueue_job(payload.binary_addr, payload.input_addr) else {
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

#[derive(Debug, Serialize)]
pub struct JobsClaimResponse {
    job: Job,
}

pub async fn claim(Json(payload): Json<JobsClaimRequest>) -> JsonHandlerResult<JobsClaimResponse> {
    let Some(job) = claim_job(&payload.runner_id) else {
        return Err((StatusCode::NO_CONTENT, String::from("No pending jobs")).into_response());
    };

    Ok(Json(JobsClaimResponse { job }))
}

pub async fn tickle(Path(job_id): Path<Uuid>) -> JsonHandlerResult<Value> {
    let Ok(()) = tickle_job(&job_id) else {
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
    Path(job_id): Path<Uuid>,
    Json(payload): Json<JobsCompleteRequest>,
) -> JsonHandlerResult<Value> {
    let res = match payload.status {
        JobCompletionStatus::Failed => fail_job(&job_id, &payload.output_addr),
        JobCompletionStatus::Completed => complete_job(&job_id, &payload.output_addr),
    };
    let Ok(()) = res else {
        return Err((StatusCode::BAD_REQUEST, String::from("Job does not exist or is not active")).into_response());
    };

    Ok(Json(json!({})))
}

pub async fn get(Path(job_id): Path<Uuid>) -> JsonHandlerResult<Job> {
    let Ok(job) = get_job(&job_id) else {
        return Err((StatusCode::NOT_FOUND, String::from("Job does not exist")).into_response());
    };

    Ok(Json(job))
}

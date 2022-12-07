use axum::{
    extract::Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::queue::{claim_job, enqueue_job, Job};

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

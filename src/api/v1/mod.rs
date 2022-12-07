use axum::extract::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

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
pub async fn create(Json(payload): Json<JobsCreateRequest>) -> Json<JobsCreateResponse> {
    log::info!("Enqueueing job {payload:?}");
    Json(JobsCreateResponse {
        job_id: String::from("1234"),
    })
}

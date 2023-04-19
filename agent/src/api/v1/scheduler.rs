use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::Json;
use utils::mesh::ServalRole;
use utils::structs::api::{SchedulerEnqueueJobResponse, SchedulerJobStatusResponse};
use uuid::Uuid;

use crate::structures::*;

/// Mount all jobs endpoint handlers onto the passed-in router.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/scheduler/enqueue/:name", post(enqueue_job))
        .route("/v1/scheduler/claim", post(claim_job))
        .route("/v1/scheduler/:job_id/tickle", post(tickle_job))
        .route("/v1/scheduler/:job_id/status", get(job_status))
}

/// Mount a handler that relays all job-running requests to another node.
pub fn mount_proxy(router: ServalRouter) -> ServalRouter {
    router.route("/v1/scheduler/*rest", any(proxy))
}

/// Relay all scheduler requests to a node that can handle them.
async fn proxy(State(state): State<AppState>, mut request: Request<Body>) -> impl IntoResponse {
    let path = request.uri().path();
    log::info!("relaying a scheduler request; path={path}");
    metrics::increment_counter!("proxy:scheduler:{path}");

    if let Ok(resp) =
        super::proxy::relay_request(&mut request, &ServalRole::Scheduler, &state.instance_id).await
    {
        resp
    } else {
        // Welp, not much we can do
        metrics::increment_counter!("proxy:error");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Peer with the job runner role not available",
        )
            .into_response()
    }
}

/// This is the main scheduler endpoint. It accepts incoming jobs and holds them until they can be
/// claimed by an appropriate runner.
async fn enqueue_job(
    Path(name): Path<String>,
    state: State<AppState>,
    input: Bytes,
) -> Result<Json<SchedulerEnqueueJobResponse>, impl IntoResponse> {
    let mut queue = JOBS
        .get()
        .expect("Job queue not initialized")
        .lock()
        .unwrap();
    let Ok(job_id) = queue.enqueue(name, input.to_vec()) else {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, String::from("Failed to enqueue job")).into_response());
    };

    Ok(Json(SchedulerEnqueueJobResponse { job_id }))
}

async fn claim_job(_state: State<AppState>) -> impl IntoResponse {
    StatusCode::NOT_FOUND
}

async fn tickle_job(Path(job_id): Path<Uuid>, _state: State<AppState>) -> impl IntoResponse {
    StatusCode::OK
}

async fn job_status(
    Path(job_id): Path<Uuid>,
    _state: State<AppState>,
) -> Result<Json<SchedulerJobStatusResponse>, impl IntoResponse> {
    let queue = JOBS
        .get()
        .expect("Job queue not initialized")
        .lock()
        .unwrap();
    println!("want job status");

    let Some(job) = queue.get_job(job_id) else {
        println!("404");
        return Err(StatusCode::NOT_FOUND);
    };

    println!("ok job status");

    Ok(Json(SchedulerJobStatusResponse {
        status: job.status().to_owned(),
        output: job.output().to_owned(),
    }))
}

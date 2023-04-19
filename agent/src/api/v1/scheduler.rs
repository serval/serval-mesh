use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use engine::errors::ServalEngineError;
use engine::ServalEngine;
use utils::mesh::ServalRole;
use utils::structs::Job;

use crate::structures::*;

/// Mount all jobs endpoint handlers onto the passed-in router.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/scheduler/enqueue", post(enqueue_job))
        .route("/v1/scheduler/claim", post(claim_job))
        .route("/v1/scheduler/tickle/:job_id", post(tickle_job))
}

/// Mount a handler that relays all job-running requests to another node.
pub fn mount_proxy(router: ServalRouter) -> ServalRouter {
    router.route("/v1/jobs/*rest", any(proxy))
}

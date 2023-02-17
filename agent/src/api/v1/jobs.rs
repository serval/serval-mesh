use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{Response, IntoResponse},
    routing::{get, post },
};
use engine::{errors::ServalEngineError, ServalEngine};
use utils::structs::Job;

use crate::structures::*;

/// Mount all jobs endpoint handlers onto the passed-in router.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
    .route("/v1/jobs", get(running)) // TODO
    .route("/v1/jobs/:name/run", post(run_job)) // has an input payload; TODO options (needs design)
}
/// Instead of mounting routes, mount middleware that relays all job-running requests to another node.
pub async fn proxy(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next<Body>,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    if path.starts_with("/v1/jobs/") {
        log::info!("relaying a job-runner request; path={path}");
        if let Ok(resp) =
            super::proxy::relay_request(&mut req, SERVAL_SERVICE_RUNNER, &state.instance_id).await
        {
            Ok(resp)
        } else {
            // Welp, not much we can do
            Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                format!("{SERVAL_SERVICE_RUNNER} not available"),
            )
                .into_response())
        }
    } else {
        Ok(next.run(req).await)
    }
}

/// Get running jobs
async fn running(_state: State<AppState>) -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

/// This is the main worker endpoint. It accepts incoming jobs and runs them.
async fn run_job(
    Path(name): Path<String>,
    state: State<AppState>,
    input: Bytes,
) -> impl IntoResponse {
    let storage = STORAGE.get().expect("Storage not initialized!");
    let Ok(manifest) = storage.manifest(&name).await else {
        return (StatusCode::NOT_FOUND, "no manifest of that name found").into_response();
    };

    let Ok(executable) = storage.executable_as_bytes(&name, manifest.version()).await else {
        return (StatusCode::NOT_FOUND, "no executable found for manifest; key={key}").into_response();
    };

    let job = Job::new(manifest, executable, input.to_vec());
    log::info!(
        "received WASM job; name={}; executable length={}; input length={}; id={}",
        &job.manifest().fq_name(),
        &job.executable().len(),
        input.len(),
        job.id()
    );

    let start = std::time::Instant::now();

    // What we'll do later is accept this job for processing and send it to a thread or something.
    // But for now we do it right here, in our handler.
    // The correct response by design is a 202 Accepted plus the metadata object.
    // TODO: SER-38 - capture exit code for failed jobs
    log::info!(
        "about to run job name=TODO; id={}; executable size={}",
        job.id(),
        job.executable().len()
    );

    let extensions = state.extensions.clone();

    let Ok(mut engine) = ServalEngine::new(extensions) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "unable to create wasm engine").into_response();
    };
    let result = engine.execute(job.executable(), job.input());

    match result {
        Ok(result) => {
            // We're not doing anything with stderr here.
            log::info!(
                "job completed; job={}; code={}; elapsed_ms={}",
                job.id(),
                result.code,
                start.elapsed().as_millis()
            );
            if result.code == 0 {
                // Zero exit status code is a success.
                (StatusCode::OK, result.stdout).into_response()
            } else {
                (StatusCode::OK, result.stderr).into_response()
            }
        }
        Err(ServalEngineError::ExecutionError {
            stdout: _,
            error: _,
            stderr,
        }) => {
            // Now the fun part of http error signaling: the request was successful, but the
            // result of the operation was bad from the user's point of view. Our behavior here
            // is yet to be defined but I'm sending back stderr just to show we can.
            (StatusCode::OK, stderr).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}


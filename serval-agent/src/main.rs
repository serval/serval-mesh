#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dotenvy::dotenv;
use engine::ServalEngine;
use http::header::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utils::{mdns::advertise_service, structs::WasmResult};
use uuid::Uuid;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone, Serialize)]
struct RunnerState {
    jobs: HashMap<String, JobMetadata>,
    total: usize,
    errors: usize,
}

impl RunnerState {
    pub fn new() -> Self {
        RunnerState {
            total: 0,
            errors: 0,
            jobs: HashMap::new(),
        }
    }
}

type AppState = Arc<Mutex<RunnerState>>;

#[derive(Clone, Serialize, Deserialize)]
struct JobMetadata {
    id: Uuid,
    name: String,
    description: String,
    status_url: String, // for the moment
    result_url: String, // for the moment
}

impl From<Envelope> for JobMetadata {
    fn from(envelope: Envelope) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            name: envelope.name.clone(),
            description: envelope.description,
            status_url: format!("/jobs/{}/status", id),
            result_url: format!("/jobs/{}/result", id),
        }
    }
}

/// Remember what is important.
async fn clacks<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let mut response = next.run(req).await;
    response.headers_mut().append(
        "X-Clacks-Overhead",
        HeaderValue::from_static("GNU/Terry Pratchett"),
    );
    Ok(response)
}

/// Respond to ping. Useful for monitoring.
async fn ping() -> String {
    "pong".to_string()
}

#[derive(Clone, Debug, Deserialize)]
struct Envelope {
    name: String,
    description: String,
}

/// This is the main worker endpoint. It accepts incoming jobs and runs them.
async fn incoming(state: State<AppState>, mut multipart: Multipart) -> Response {
    let mut envelope: Option<Envelope> = None;
    let mut binary: Option<Vec<u8>> = None;
    let mut input: Option<Vec<u8>> = None;

    // chomp up form input here
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.unwrap();
        match name.as_str() {
            "envelope" => {
                let data = data.to_vec();
                let Ok(parsed) = serde_json::from_slice(&data) else {
                    // this is not good enough
                    return (StatusCode::BAD_REQUEST, "job envelope is invalid".to_string()).into_response();
                };
                envelope = Some(parsed);
            }
            "input" => {
                input = Some(data.to_vec());
            }
            "executable" => {
                binary = Some(data.to_vec());
            }
            _ => {
                log::info!("ignoring unknown field `{name}`");
            }
        }
    }

    if binary.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "no wasm executable data provided!".to_string(),
        )
            .into_response();
    }

    let envelope = envelope.unwrap_or(Envelope {
        name: "unknown".to_string(),
        description: "unknown job description".to_string(),
    });
    let metadata: JobMetadata = JobMetadata::from(envelope);
    let binary = binary.unwrap();
    log::info!(
        "received WASM job; name={}; executable length={}; input length={}",
        metadata.name,
        binary.len(),
        input.as_ref().map(|input| input.len()).unwrap_or_else(|| 0),
    );

    // Poor human's history tracking here. We'll need to do better at some point.
    // E.g., handle overflows. That would be some nice uptime.
    let mut state = state.lock().await;
    state.total += 1;
    state.jobs.insert(metadata.id.to_string(), metadata.clone());

    let start = std::time::Instant::now();

    // What we'll do later is accept this job for processing and send it to a thread or something.
    // But for now we do it right here, in our handler.
    // The correct response by design is a 202 Accepted plus the metadata object.
    // TODO: SER-38 - capture exit code for failed jobs
    match execute_job(&metadata, binary, input).await {
        Ok(result) => {
            // We're not doing anything with stderr here.
            log::info!(
                "job completed; job={}; code={}; elapsed_ms={}",
                metadata.id,
                result.code,
                start.elapsed().as_millis()
            );
            if result.code == 0 {
                // Zero exit status code is a success.
                (StatusCode::OK, result.stdout).into_response()
            } else {
                // Now the fun part of http error signalling: the request was successful, but the
                // result of the operation was bad from the user's point of view. Our behavior here
                // is yet to be defined but I'm sending back stderr just to show we can.
                (StatusCode::OK, result.stderr).into_response()
            }
        }
        Err(e) => {
            state.errors += 1;
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Run a job in the wasm engine.
async fn execute_job(
    metadata: &JobMetadata,
    executable: Vec<u8>,
    input: Option<Vec<u8>>,
) -> Result<WasmResult> {
    log::info!(
        "about to run job name={}; id={}",
        metadata.name,
        metadata.id
    );

    let stdin = input.unwrap_or_default();

    let mut engine = ServalEngine::new()?;
    let result = engine.execute(&executable, &stdin)?;

    Ok(result)
}

async fn monitor_history(state: State<AppState>) -> Json<RunnerState> {
    let state = state.lock().await;
    Json(state.clone())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8100".to_string())
        .parse()?;

    let state = Arc::new(Mutex::new(RunnerState::new()));

    const MAX_BODY_SIZE_BYTES: usize = 100 * 1024 * 1024;
    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/history", get(monitor_history))
        .route("/jobs", post(incoming))
        .route_layer(middleware::from_fn(clacks))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_BYTES))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    log::info!("serval agent daemon listening on {}", &addr);

    advertise_service("serval_daemon", port, None)?;

    let addr: SocketAddr = addr.parse().unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}

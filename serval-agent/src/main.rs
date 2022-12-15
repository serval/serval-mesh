use anyhow::Result;
use axum::{
    extract::{Multipart, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use dotenvy::dotenv;
use engine::ServalEngine;
use http::header::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utils::mdns::advertise_service;
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

/// Unimplemented main worker endpoint.
async fn incoming(state: State<AppState>, mut multipart: Multipart) -> (StatusCode, String) {
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
                    return (StatusCode::BAD_REQUEST, "job envelope is invalid".to_string());
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
        );
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

    // What we'll do later is accept this job for processing and send it to a thread or something.
    // But for now we do it right here, in our handler.
    // The correct response by design is a 202 Accepted plus the metadata object.
    match execute_job(&metadata, binary).await {
        Ok(v) => (StatusCode::OK, v),
        Err(e) => {
            state.errors += 1;
            (StatusCode::BAD_REQUEST, e.to_string())
        }
    }
}

/// Run a job in the wasm engine.
async fn execute_job(metadata: &JobMetadata, executable: Vec<u8>) -> anyhow::Result<String> {
    log::info!(
        "about to run job name={}; id={}",
        metadata.name,
        metadata.id
    );
    let mut engine = ServalEngine::new()?;
    let bytes = engine.execute(&executable, &vec![])?;
    let contents = String::from_utf8(bytes)?;

    Ok(contents)
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

    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/history", get(monitor_history))
        .route("/jobs", post(incoming))
        .route_layer(middleware::from_fn(clacks))
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

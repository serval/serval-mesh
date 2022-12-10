use anyhow::Result;
use axum::{
    extract::{Extension, Multipart},
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

use std::collections::HashMap;
use std::ops::Deref;
use std::{net::SocketAddr, sync::Arc};

#[derive(Clone)]
struct RunnerState {
    engine: ServalEngine,
    history: RunnerHistory,
}

#[derive(Clone, Serialize)]
struct RunnerHistory {
    jobs: HashMap<String, JobMetadata>,
    total: usize,
}

impl RunnerState {
    pub fn new() -> Result<Self> {
        let engine = ServalEngine::new()?;
        let history = RunnerHistory {
            total: 0,
            jobs: HashMap::new(),
        };
        Ok(Self { engine, history })
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct JobMetadata {
    id: uuid::Uuid,
    name: String,
    description: String,
    update_url: String, // for the moment
    result_url: String, // for the moment
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

/// Unimplemented main worker endpoint.
async fn incoming(
    Extension(mut state): Extension<Arc<RunnerState>>,
    mut multipart: Multipart,
) -> (StatusCode, String) {
    let Some(mut state) = Arc::get_mut(&mut state) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to read app state".to_string());
    };

    state.history.total += 1;

    // chomp up form input here

    (StatusCode::ACCEPTED, "acccepted".to_string())
}

async fn monitor_history(Extension(state): Extension<Arc<RunnerState>>) -> Json<RunnerHistory> {
    Json(state.deref().history.clone())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());

    let state = RunnerState::new()?;

    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/history", get(monitor_history))
        .route("/jobs", post(incoming))
        .route_layer(middleware::from_fn(clacks))
        .with_state(Arc::new(state));

    let addr = format!("{}:{}", host, port);
    log::info!("wait-for-it daemon listening on {}", &addr);

    let addr: SocketAddr = addr.parse().unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}

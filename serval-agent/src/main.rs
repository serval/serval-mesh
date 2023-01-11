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
    extract::DefaultBodyLimit,
    middleware::{self},
    routing::{get, post, put},
    Router,
};
use dotenvy::dotenv;
use tokio::sync::Mutex;
use utils::mdns::advertise_service;

use std::net::SocketAddr;
use std::sync::Arc;

mod api;
use crate::api::*;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8100".to_string())
        .parse()?;
    let blob_path = std::env::var("BLOB_STORE").unwrap_or_else(|_| "/tmp".to_string());

    log::info!("serval agent blob store mounted; path={blob_path}");
    let state = Arc::new(Mutex::new(RunnerState::new(blob_path)));

    const MAX_BODY_SIZE_BYTES: usize = 100 * 1024 * 1024;
    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/history", get(jobs::monitor_history))
        .route("/jobs", post(jobs::incoming))
        .route("/run/:addr", get(jobs::run_stored_job))
        .route("/blobs", put(storage::store_blob))
        .route("/blobs/:addr", get(storage::get_blob))
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

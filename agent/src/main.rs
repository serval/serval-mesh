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
    routing::{get, head, post, put},
    Router,
};
use dotenvy::dotenv;
use tokio::sync::Mutex;
use utils::{mdns::advertise_service, networking::find_nearest_port};
use uuid::Uuid;

use std::net::SocketAddr;
use std::{path::PathBuf, sync::Arc};

mod api;
use crate::api::*;

#[derive(Debug, Clone)]
enum StorageRoleConfig {
    Auto,
    Never,
    Always,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .map(Ok)
        .unwrap_or_else(|_| find_nearest_port(8100).map(|port| port.to_string()))?
        .parse()?;
    let storage_role = match &std::env::var("STORAGE_ROLE").unwrap_or_else(|_| "auto".to_string())[..]
    {
        "always" => StorageRoleConfig::Always,
        "auto" => StorageRoleConfig::Auto,
        "never" => StorageRoleConfig::Never,
        _ => {
            log::warn!(
                "Invalid value for STORAGE_ROLE environment variable; defaulting to 'never'"
            );
            StorageRoleConfig::Never
        }
    };
    let blob_path = match storage_role {
        StorageRoleConfig::Never => None,
        _ => Some(
            std::env::var("BLOB_STORE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir().join("serval_storage")),
        ),
    };

    let instance_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(RunnerState::new(
        instance_id,
        blob_path.clone(),
    )?));

    const MAX_BODY_SIZE_BYTES: usize = 100 * 1024 * 1024;
    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/history", get(jobs::monitor_history))
        .route("/jobs", post(jobs::incoming))
        .route("/run/:addr", get(jobs::run_stored_job))
        // begin optional endpoints; these requests will be pre-empted by our
        // proxy_unavailable_services middleware if they aren't implemented by this instance.
        .route("/storage/blobs", put(storage::store_blob))
        .route("/storage/blobs/:addr", get(storage::get_blob))
        .route("/storage/blobs/:addr", head(storage::has_blob))
        // end optional endpoints
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            proxy::proxy_unavailable_services,
        ))
        .route_layer(middleware::from_fn(clacks))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_BYTES))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    log::info!("serval agent daemon listening on {}", &addr);

    advertise_service("serval_daemon", port, &instance_id, None)?;

    if blob_path.is_some() {
        log::info!("serval agent blob store mounted; path={blob_path:?}");
        advertise_service("serval_storage", port, &instance_id, None)?;
    }

    let addr: SocketAddr = addr.parse().unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}

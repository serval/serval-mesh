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
    Router, Server,
};
use dotenvy::dotenv;
use tokio::sync::Mutex;
use utils::{mdns::advertise_service, networking::find_nearest_port};
use uuid::Uuid;

use std::{net::SocketAddr, process};
use std::{path::PathBuf, sync::Arc};

mod api;
use crate::api::*;

mod structures;
use crate::structures::RunnerState;

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
        .route("/v1/monitor/history", get(v1::jobs::monitor_history))
        .route("/v1/jobs", post(v1::jobs::incoming))
        .route("/v1/run/:addr", get(v1::jobs::run_stored_job))
        // begin optional endpoints; these requests will be pre-empted by our
        // proxy_unavailable_services middleware if they aren't implemented by this instance.
        .route("/v1/storage/blobs", put(v1::storage::store_blob))
        .route("/v1/storage/blobs/:addr", get(v1::storage::get_blob))
        .route("/v1/storage/blobs/:addr", head(v1::storage::has_blob))
        // end optional endpoints
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            v1::proxy::proxy_unavailable_services,
        ))
        .route_layer(middleware::from_fn(clacks))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_BYTES))
        .with_state(state);

    let predefined_port: Option<u16> = match std::env::var("PORT") {
        Ok(port_str) => port_str.parse::<u16>().ok(),
        Err(_) => None,
    };

    // Start the Axum server; this is in a loop so we can try binding more than once in case our
    // randomly-selected port number ends up conflicting with something else due to a race condition.
    let mut port: u16;
    let server: Server<_, _> = loop {
        port = predefined_port.unwrap_or_else(|| find_nearest_port(8100).unwrap());
        let addr: SocketAddr = format!("{host}:{port}").parse().unwrap();
        match axum::Server::try_bind(&addr) {
            Ok(builder) => break builder.serve(app.into_make_service()),
            Err(_) => {
                // Port number in use already, presumably
                if predefined_port.is_some() {
                    log::error!("Specified port number ({port}) is already in use; aborting");
                    process::exit(1);
                }
            }
        }
    };

    log::info!("serval agent daemon listening on {host}:{port}");
    advertise_service("serval_daemon", port, &instance_id, None)?;

    if blob_path.is_some() {
        log::info!("serval agent blob store mounted; path={blob_path:?}");
        advertise_service("serval_storage", port, &instance_id, None)?;
    }

    server.await.unwrap();
    Ok(())
}

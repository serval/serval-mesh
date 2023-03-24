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
    body::*,
    extract::DefaultBodyLimit,
    middleware::{self},
    routing::get,
    Router, Server,
};
use dotenvy::dotenv;
use engine::ServalEngine;
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalMesh, ServalRole};
use utils::networking::find_nearest_port;
use uuid::Uuid;

// TODO: should switch on feature.
use metrics_exporter_tcp::TcpBuilder;

use std::{net::SocketAddr, process};
use std::{path::PathBuf, sync::Arc};

mod api;
use crate::api::*;

mod structures;
use crate::structures::*;

#[tokio::main]
async fn main() -> Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    // TODO: metrics sink initialization based on env vars or config
    let metrics_port = std::env::var("METRICS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{metrics_port}").parse().unwrap();
    let builder = TcpBuilder::new().listen_address(addr);

    if let Err(err) = builder.install() {
        log::warn!("failed to install TCP recorder: {err:?}");
    };
    metrics::increment_counter!("process:start", "component" => "agent");

    let storage_role = match &std::env::var("STORAGE_ROLE").unwrap_or_else(|_| "auto".to_string())[..]
    {
        "always" => true,
        "auto" => {
            // todo: add some sort of heuristic to determine whether we should be a storage node
            // for now, don't be a storage node unless explicitly asked to be; this should change
            // once we have distributed storage rather than a single-node temporary hack.
            false
        }
        "never" => false,
        _ => {
            log::warn!(
                "Invalid value for STORAGE_ROLE environment variable; defaulting to 'never'"
            );
            false
        }
    };
    let blob_path = if storage_role {
        Some(
            std::env::var("BLOB_STORE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir().join("serval_storage")),
        )
    } else {
        None
    };
    let should_run_jobs = match &std::env::var("RUNNER_ROLE").unwrap_or_else(|_| "auto".to_string())
        [..]
    {
        "always" => {
            if !ServalEngine::is_available() {
                log::error!("RUNNER_ROLE environment variable is set to 'always', but this platform is not supported by our Wasm engine.");
                process::exit(1)
            }
            true
        }
        "auto" => ServalEngine::is_available(),
        "never" => false,
        _ => {
            log::warn!("Invalid value for RUNNER_ROLE environment variable; defaulting to 'never'");
            false
        }
    };

    let extensions_path = std::env::var("EXTENSIONS_PATH").ok().map(PathBuf::from);

    let instance_id = Uuid::new_v4();
    let state = Arc::new(RunnerState::new(
        instance_id,
        blob_path.clone(),
        extensions_path.clone(),
        should_run_jobs,
    )?);
    log::info!(
        "agent configured with storage={} and run-jobs={}",
        state.has_storage,
        state.should_run_jobs
    );

    const MAX_BODY_SIZE_BYTES: usize = 100 * 1024 * 1024;

    let mut router: Router<Arc<RunnerState>, Body> = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/status", get(monitor_status));

    // NOTE: We have two of these now. If we develop a third, generalize this pattern.
    router = if state.has_storage {
        v1::storage::mount(router)
    } else {
        v1::storage::mount_proxy(router)
    };

    router = if state.should_run_jobs {
        v1::jobs::mount(router)
    } else {
        v1::jobs::mount_proxy(router)
    };

    let app = router
        .route_layer(middleware::from_fn(clacks))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_BYTES))
        .with_state(state.clone());

    let predefined_port: Option<u16> = match std::env::var("PORT") {
        Ok(port_str) => port_str.parse::<u16>().ok(),
        Err(_) => None,
    };

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

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

    log::info!("serval agent http will listen on {host}:{port}");

    if let Some(extensions_path) = extensions_path {
        let extensions = &state.extensions;
        log::info!(
            "Found {} extensions at {extensions_path:?}: {:?}",
            extensions.len(),
            extensions.keys(),
        );
    }

    let mut roles: Vec<ServalRole> = Vec::new();
    if blob_path.is_some() {
        log::info!("serval agent blob store mounted; path={blob_path:?}");
        roles.push(ServalRole::Storage);
    }
    if should_run_jobs {
        log::info!("job running enabled");
        roles.push(ServalRole::Runner);
    } else {
        log::info!("job running not enabled (or not supported)");
    }

    let mesh_port: u16 = match std::env::var("MESH_PORT") {
        Ok(port_str) => port_str.parse::<u16>().unwrap_or(8181),
        Err(_) => 8181,
    };

    let metadata = PeerMetadata::new(
        Uuid::new_v4().to_string(),
        format!("http://{}:{}", host, port),
        roles,
        None,
    );
    let mut mesh = ServalMesh::new(metadata, mesh_port, None).await?;
    mesh.start().await?;
    MESH.set(mesh).unwrap();

    // And finally, listen on HTTP.
    server.await.unwrap();
    Ok(())
}

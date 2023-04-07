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
use utils::mesh::{mesh_interface_and_port, KaboodleMesh, PeerMetadata, ServalMesh, ServalRole};
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

    let config = init_config();
    init_metrics();

    log::info!("instance id {}", config.instance_id);
    let state = Arc::new(RunnerState::new(
        config.instance_id,
        config.blob_path.clone(),
        config.extensions_path.clone(),
        config.should_run_jobs,
    )?);
    log::info!(
        "agent configured with storage={} and run-jobs={}",
        state.has_storage,
        state.should_run_jobs
    );

    let app = init_router(&state);

    // Start the Axum server; this is in a loop so we can try binding more than once in case our
    // randomly-selected port number ends up conflicting with something else due to a race condition.
    let mut http_addr: SocketAddr;
    let server: Server<_, _> = loop {
        let host = std::env::var("HOST").unwrap_or_else(|_| "[::]".to_string());
        let predefined_port = std::env::var("PORT")
            .ok()
            .and_then(|port_str| port_str.parse::<u16>().ok());
        let port = predefined_port.unwrap_or_else(|| find_nearest_port(8100).unwrap());
        http_addr = format!("{host}:{port}").parse().unwrap();
        let Ok(builder) = axum::Server::try_bind(&http_addr) else {
            // Port number in use already, presumably
            if predefined_port.is_some() {
                log::error!("Specified port number ({port}) is already in use; aborting");
                process::exit(1);
            }
            continue;
        };
        break builder.serve(app.into_make_service());
    };

    log::info!("serval agent http will listen on {http_addr}");

    if let Some(extensions_path) = config.extensions_path {
        let extensions = &state.extensions;
        log::info!(
            "Found {} extensions at {extensions_path:?}: {:?}",
            extensions.len(),
            extensions.keys(),
        );
    }

    let mut roles: Vec<ServalRole> = Vec::new();
    if let Some(storage_path) = config.blob_path {
        log::info!(
            "serval agent blob store mounted; path={}",
            storage_path.display()
        );
        roles.push(ServalRole::Storage);
    }
    if config.should_run_jobs {
        log::info!("job running enabled");
        roles.push(ServalRole::Runner);
    } else {
        log::info!("job running not enabled (or not supported)");
    }

    let (mesh_interface, mesh_port) = mesh_interface_and_port();
    let metadata = PeerMetadata::new(
        Uuid::new_v4().to_string(),
        Some(http_addr.port()),
        roles,
        mesh_interface.ip(),
    );
    let mut mesh = ServalMesh::new(metadata, mesh_port, Some(mesh_interface)).await?;
    mesh.start().await?;
    MESH.set(mesh).unwrap();

    // And finally, listen on HTTP.
    server.await.unwrap();
    Ok(())
}

struct Config {
    instance_id: Uuid,
    extensions_path: Option<PathBuf>,
    should_run_jobs: bool,
    blob_path: Option<PathBuf>,
}
fn init_config() -> Config {
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

    Config {
        instance_id,
        extensions_path,
        should_run_jobs,
        blob_path,
    }
}

fn init_metrics() {
    // TODO: This should switch on which set of metrics features we're building with.
    let metrics_addr = std::env::var("METRICS_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".to_string());
    let addr: SocketAddr = metrics_addr.parse().unwrap();
    let builder = TcpBuilder::new().listen_address(addr);

    if let Err(err) = builder.install() {
        log::warn!("failed to install TCP recorder: {err:?}");
    };
    metrics::increment_counter!("process:start", "component" => "agent");
}

fn init_router(state: &Arc<RunnerState>) -> Router {
    const MAX_BODY_SIZE_BYTES: usize = 100 * 1024 * 1024;

    let mut router: Router<Arc<RunnerState>, Body> = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/monitor/status", get(monitor_status));
    router = v1::mesh::mount(router);

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

    router
        .route_layer(middleware::from_fn(clacks))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE_BYTES))
        .with_state(state.clone())
}

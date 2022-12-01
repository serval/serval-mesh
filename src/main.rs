use std::{env, fs, net::SocketAddr, path::PathBuf};

use anyhow::anyhow;
use axum::{
    body::{Bytes, StreamBody},
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, put},
    Router,
};
use clap::Parser;
use dotenv::dotenv;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    // Where to store data; defaults to a one-off temporary directory if not specified
    path: Option<PathBuf>,
}

#[derive(Clone)]
struct AxumState {
    storage_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    // Figure out where we're storing data
    let args = Args::parse();
    let storage_path = args.path.unwrap_or_else(|| {
        log::warn!("No --path specified; defaulting to $TMPDIR/castaway");
        env::temp_dir().join("castaway")
    });
    if !storage_path.exists() {
        fs::create_dir(&storage_path)?;
    }

    log::info!("Storage path: {}", &storage_path.display());

    // Actually do the thing
    init_http(storage_path).await?;

    Ok(())
}

fn is_valid_blob_addr(addr: &String) -> bool {
    // this does not seem worth adding a regexp crate for
    let valid_chars = String::from("0123456789abcdef");
    addr.len() == 40
        && addr
            .to_lowercase()
            .chars()
            .all(|ch| valid_chars.contains(ch))
}

async fn get_blob(
    Path(blob_addr): Path<String>,
    State(state): State<AxumState>,
) -> impl IntoResponse {
    if !is_valid_blob_addr(&blob_addr) {
        log::warn!("Request for an invalid address; addr={}", blob_addr);
        return StatusCode::BAD_REQUEST.into_response();
    }
    let filename = state.storage_path.join(&blob_addr);
    if !filename.exists() {
        log::warn!("Blob not found; addr={}", blob_addr);
        return (
            StatusCode::NOT_FOUND,
            format!("Blob {} not found", &blob_addr),
        )
            .into_response();
    }

    let file = match tokio::fs::File::open(filename).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let stream = ReaderStream::new(file);
    let body = StreamBody::new(stream);
    let headers = [(
        header::CONTENT_TYPE,
        String::from("application/octet-stream"),
    )];

    log::info!("Serving blob; addr={}", &blob_addr);
    (headers, body).into_response()
}

async fn store_blob(State(state): State<AxumState>, body: Bytes) -> impl IntoResponse {
    let mut hasher = Sha1::new();
    hasher.update(&body);
    let blob_addr = hex::encode(hasher.finalize());
    let filename = state.storage_path.join(&blob_addr);
    if filename.exists() {
        log::info!("Blob that already existed; addr={}", &blob_addr);
        return (StatusCode::OK, blob_addr).into_response();
    }

    let mut file = match tokio::fs::File::create(filename).await {
        Ok(file) => file,
        Err(err) => {
            log::error!("Error opening destination file for writing, err={:?}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match file.write(&body).await {
        Ok(_) => {
            log::info!("Stored blob; addr={} size={}", &blob_addr, body.len());
            (StatusCode::CREATED, blob_addr).into_response()
        }
        Err(err) => {
            log::error!("Error writing blob, err={:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn init_http(storage_path: PathBuf) -> anyhow::Result<()> {
    let state = AxumState { storage_path };
    let app = Router::new()
        .route("/blob", put(store_blob))
        .route("/:addr", get(get_blob))
        .with_state(state);

    let addr: SocketAddr = "0.0.0.0:7475".parse()?;
    log::info!("API service about to listen on http://{addr}");
    match axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}

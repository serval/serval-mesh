use std::{net::SocketAddr, path::PathBuf};

use anyhow::anyhow;
use axum::{
    body::{Bytes, StreamBody},
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, head, put},
    Router,
};

use utils::blobs::BlobStore;
use utils::errors::ServalError;

#[derive(Clone)]
struct AxumState {
    storage: BlobStore,
}

async fn get_blob(
    Path(blob_addr): Path<String>,
    State(state): State<AxumState>,
) -> impl IntoResponse {
    match state.storage.get_stream(&blob_addr).await {
        Ok(stream) => {
            let body = StreamBody::new(stream);
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/octet-stream"),
            )];

            log::info!("Serving blob; addr={}", &blob_addr);
            (headers, body).into_response()
        }
        Err(e) => match e {
            ServalError::BlobAddressInvalid(_) => {
                log::warn!("Request for an invalid address; addr={}", blob_addr);
                StatusCode::BAD_REQUEST.into_response()
            }
            ServalError::BlobAddressNotFound(_) => {
                log::warn!("Blob not found; addr={blob_addr}");
                (
                    StatusCode::NOT_FOUND,
                    format!("Blob {} not found", &blob_addr),
                )
                    .into_response()
            }
            ServalError::IoError(_) => {
                log::warn!("i/o error reading blob; addr={blob_addr}; {:?}", e);
                (
                    StatusCode::NOT_FOUND,
                    format!("Blob {} not found", &blob_addr),
                )
                    .into_response()
            }
            _ => {
                log::warn!("unexpected error case; addr={blob_addr}; {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
    }
}

async fn has_blob(Path(blob_addr): Path<String>, State(state): State<AxumState>) -> StatusCode {
    match state.storage.has_blob(&blob_addr).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(ServalError::BlobAddressInvalid(_)) => StatusCode::BAD_REQUEST,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn store_blob(State(state): State<AxumState>, body: Bytes) -> impl IntoResponse {
    match state.storage.store(&body).await {
        Ok((new, address)) => {
            log::info!("Stored blob; addr={} size={}", &address, body.len());
            if new {
                (StatusCode::CREATED, address).into_response()
            } else {
                (StatusCode::OK, address).into_response()
            }
        }
        Err(_e) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn init_http(host: &str, port: u16, storage_path: PathBuf) -> anyhow::Result<()> {
    let storage = BlobStore::new(storage_path);
    let state = AxumState { storage };
    let app = Router::new()
        .route("/blob", put(store_blob))
        .route("/blob/:addr", get(get_blob))
        .route("/blob/:addr", head(has_blob))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    let addr: SocketAddr = addr.parse()?;
    log::info!("API service about to listen on http://{addr}");
    match axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}

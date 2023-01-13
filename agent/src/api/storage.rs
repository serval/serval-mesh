use axum::{
    body::{Bytes, StreamBody},
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
};

use utils::errors::ServalError;

use super::*;

pub async fn get_blob(
    Path(blob_addr): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Yeah, I don't like this.
    let state = state.lock().await;
    let storage = state.storage.as_ref().unwrap();

    match storage.get_stream(&blob_addr).await {
        Ok(stream) => {
            let body = StreamBody::new(stream);
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/octet-stream"),
            )];

            log::info!("Serving blob; addr={}", &blob_addr);
            (headers, body).into_response()
        }
        Err(e) => {
            log::warn!("error reading blob; addr={}; error={}", blob_addr, e);
            e.into_response()
        }
    }
}

pub async fn store_blob(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    // Yeah, I don't like this.
    let state = state.lock().await;
    let storage = state.storage.as_ref().unwrap();

    match storage.store(&body).await {
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

pub async fn has_blob(Path(blob_addr): Path<String>, State(state): State<AppState>) -> StatusCode {
    // Yeah, I don't like this.
    let state = state.lock().await;
    let storage = state.storage.as_ref().unwrap();

    match storage.has_blob(&blob_addr).await {
        Ok(exists) => {
            log::info!("Has blob?; exists={exists} addr={blob_addr}");
            if exists {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        }
        Err(ServalError::BlobAddressInvalid(_)) => StatusCode::BAD_REQUEST,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

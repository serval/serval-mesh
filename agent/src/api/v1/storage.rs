use axum::{
    body::{Bytes, StreamBody},
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};

use utils::errors::ServalError;
use utils::structs::Manifest;

use crate::structures::*;

/// Fetch an executable by fully-qualified manifest name.
pub async fn get_executable(
    Path((name, version)): Path<(String, String)>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    match storage.executable_as_stream(&name, &version).await {
        Ok(stream) => {
            let body = StreamBody::new(stream);
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/octet-stream"),
            )];

            log::info!("Serving job binary; name={}", &name);
            (headers, body).into_response()
        }
        Err(e) => {
            log::warn!("error reading job binary; name={}; error={}", name, e);
            e.into_response()
        }
    }
}

/// Fetch task manifest by name. The manifest is returned as json.
pub async fn get_manifest(
    Path(name): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    match storage.manifest(&name).await {
        Ok(v) => {
            log::info!("Serving job manifest; name={}", &name);
            let stringified = v.to_string();
            let headers = [(header::CONTENT_TYPE, String::from("application/toml"))];
            (headers, stringified).into_response()
        }
        Err(e) => {
            log::warn!("error reading job metadata; name={}; error={}", &name, e);
            e.into_response()
        }
    }
}

/// Store a job with its metadata.
pub async fn store_executable(
    State(_state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
    body: Bytes,
) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    let Ok(manifest) = storage.manifest(&name).await else {
        return (StatusCode::NOT_FOUND, format!("no manifest of that name found; name={name}")).into_response();
    };

    let bytes = body.to_vec();

    match storage.store_executable(&name, &version, &bytes).await {
        Ok(integrity) => {
            log::info!(
                "Stored new executable; name={}@{}; executable_hash={}; size={}",
                manifest.fq_name(),
                version,
                integrity,
                bytes.len()
            );
            (StatusCode::CREATED, integrity).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// Returns true if this node has access to the given task type, specified by fully-qualified name.
pub async fn has_manifest(Path(name): Path<String>, State(_state): State<AppState>) -> StatusCode {
    let storage = STORAGE.get().unwrap();

    match storage.data_exists_by_key(&name).await {
        Ok(exists) => {
            log::info!("Has manifest?; exists={exists} addr={name}");
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

pub async fn list_manifests(State(_state): State<AppState>) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    match storage.manifest_names() {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn store_manifest(State(_state): State<AppState>, body: String) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    match Manifest::from_string(&body) {
        Ok(manifest) => {
            log::info!("storing manifest for job={}", manifest.fq_name());
            match storage.store_manifest(&manifest).await {
                Ok(integrity) => {
                    log::info!(
                        "Stored new manifest; name={}; manifest_hash={}",
                        manifest.fq_name(),
                        integrity.to_string(),
                    );
                    (StatusCode::CREATED, integrity.to_string()).into_response()
                }
                Err(e) => e.into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

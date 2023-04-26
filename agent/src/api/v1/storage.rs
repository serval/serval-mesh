use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, head, post, put};
use ssri::Integrity;
use utils::errors::ServalError;
use utils::mesh::ServalRole;
use utils::structs::Manifest;

use crate::storage::STORAGE;
use crate::structures::*;

/// Mount all storage endpoint handlers onto the passed-in router.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/storage/manifests", post(store_manifest))
        .route("/v1/storage/manifests/:name", get(get_manifest))
        .route("/v1/storage/manifests/:name", head(has_manifest))
        .route(
            "/v1/storage/manifests/:name/executable/:version",
            put(store_executable),
        )
        .route(
            "/v1/storage/manifests/:name/executable/:version",
            get(get_executable),
        )
        .route("/v1/storage/data/*address", get(get_by_content_address))
        .route("/v1/storage/data/*address", head(has_content_address))
}

/// Mount a handler for all storage routes that relays requests to a node that can handle them.
pub fn mount_proxy(router: ServalRouter) -> ServalRouter {
    router.route("/v1/storage/*rest", any(proxy))
}

/// Relay all storage requests to a node that can handle them.
async fn proxy(State(state): State<AppState>, mut request: Request<Body>) -> impl IntoResponse {
    let path = request.uri().path();
    metrics::increment_counter!("storage:proxy");
    log::info!("relaying a storage request; path={path}");

    if let Ok(resp) =
        super::proxy::relay_request(&mut request, &ServalRole::Storage, &state.instance_id).await
    {
        resp
    } else {
        // Welp, not much we can do
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Peer with the storage role not available",
        )
            .into_response()
    }
}

async fn get_by_content_address(Path(address): Path<String>) -> impl IntoResponse {
    metrics::increment_counter!("storage:cas:get");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

    let Ok(integrity) = address.parse::<Integrity>() else {
        let e = ServalError::BlobAddressInvalid(format!("{} is not a valid sub-resource integrity string", address));
        return e.into_response()
    };

    match storage.data_by_sri(integrity).await {
        Ok(stream) => {
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/octet-stream"),
            )];

            log::info!("Serving CAS data; address={}", &address);
            (headers, stream).into_response()
        },
        Err(ServalError::DataNotFound(s)) => (StatusCode::NOT_FOUND, s).into_response(),
        Err(e) => {
            log::info!("Error serving CAS data; address={}; error={}", &address, e);
            e.into_response()
        }
    }
}

async fn has_content_address(Path(address): Path<String>) -> impl IntoResponse {
    metrics::increment_counter!("storage:cas:head");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

    let Ok(integrity) = address.parse::<Integrity>() else {
        let e = ServalError::BlobAddressInvalid(format!("{} is not a valid sub-resource integrity string", address));
        return e.into_response()
    };

    match storage.data_exists_by_sri(&integrity).await {
        Ok(exists) => {
            if exists {
                StatusCode::OK.into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        },
        Err(ServalError::DataNotFound(s)) => (StatusCode::NOT_FOUND, s).into_response(),
        Err(e) => {
            log::info!("Error serving CAS data head; address={}; error={}", &address, e);
            e.into_response()
        }
    }
}

/// Fetch an executable by fully-qualified manifest name.
async fn get_executable(
    Path((name, version)): Path<(String, String)>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    metrics::increment_counter!("storage:executable:get");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

    match storage.executable_as_stream(&name, &version).await {
        Ok(stream) => {
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/octet-stream"),
            )];

            log::info!("Serving job binary; name={}", &name);
            (headers, stream).into_response()
        }
        Err(e) => {
            log::warn!("error reading job binary; name={}; error={}", name, e);
            e.into_response()
        }
    }
}

/// Fetch task manifest by name. The manifest is returned as json.
async fn get_manifest(
    Path(name): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    metrics::increment_counter!("storage:manifest:get");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

    match storage.manifest(&name).await {
        Ok(manifest) => {
            let headers = [(
                header::CONTENT_TYPE,
                String::from("application/toml"),
            )];
            (headers, manifest.to_string()).into_response()
        }
        Err(ServalError::DataNotFound(s)) => (StatusCode::NOT_FOUND, s).into_response(),
        Err(e) => {
            log::warn!("error reading manifest; name={}; error={}", &name, e);
            e.into_response()
        }
    }
}

/// Store a job with its metadata.
async fn store_executable(
    State(_state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
    body: Bytes,
) -> impl IntoResponse {
    metrics::increment_counter!("storage:executable:put");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

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
            (StatusCode::CREATED, integrity.to_string()).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// Returns true if this node has access to the given task type, specified by fully-qualified name.
async fn has_manifest(Path(name): Path<String>, State(_state): State<AppState>) -> StatusCode {
    metrics::increment_counter!("storage:manifest:head");
    let Some(storage) = STORAGE.get() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };

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

async fn store_manifest(State(_state): State<AppState>, body: String) -> impl IntoResponse {
    metrics::increment_counter!("storage:manifest:post");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "storage uninitialized; programmer error".to_string()).into_response();
    };

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

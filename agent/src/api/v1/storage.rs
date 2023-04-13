use axum::{
    body::{Body, Bytes, StreamBody},
    extract::{Path, State},
    http::{header, Request, StatusCode},
    response::IntoResponse,
    routing::{any, get, head, post, put},
    Json,
};

use utils::structs::Manifest;
use utils::{errors::ServalError, mesh::ServalRole};

use crate::{
    storage::{RunnerStorage, STORAGE},
    structures::*,
};

/// Mount all storage endpoint handlers onto the passed-in router.
pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/storage/manifests", get(list_manifests))
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

/// Fetch an executable by fully-qualified manifest name.
async fn get_executable(
    Path((name, version)): Path<(String, String)>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    metrics::increment_counter!("storage:executable:get");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "unable to locate a storage node on the mesh".to_string()).into_response();
    };

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
async fn get_manifest(
    Path(name): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    metrics::increment_counter!("storage:manifest:get");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "unable to locate a storage node on the mesh".to_string()).into_response();
    };

    match storage.manifest(&name).await {
        Ok(v) => {
            log::info!("Serving job manifest; name={}", &name);
            (StatusCode::OK, Json(v)).into_response()
        }
        Err(e) => {
            log::warn!("error reading job metadata; name={}; error={}", &name, e);
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
        return (StatusCode::SERVICE_UNAVAILABLE, "unable to locate a storage node on the mesh".to_string()).into_response();
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

async fn list_manifests(State(_state): State<AppState>) -> impl IntoResponse {
    metrics::increment_counter!("storage:manifest:list");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "unable to locate a storage node on the mesh".to_string()).into_response();
    };

    match storage.manifest_names().await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn store_manifest(State(_state): State<AppState>, body: String) -> impl IntoResponse {
    metrics::increment_counter!("storage:manifest:post");
    let Some(storage) = STORAGE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "unable to locate a storage node on the mesh".to_string()).into_response();
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

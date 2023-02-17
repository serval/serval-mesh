use axum::{
    body::{Body, Bytes, StreamBody},
    extract::{Path, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{Response, IntoResponse},
    routing::{get, head, post, put},
    Json,
};

use utils::errors::ServalError;
use utils::structs::Manifest;

use crate::structures::*;

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

/// Relay all storage requests to a node that can handle them.
pub async fn proxy(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next<Body>,
) -> Result<Response, StatusCode> {
    // TODO: I kinda don't like this as middleware, but maybe it's the cleanest way to do it.

    let path = req.uri().path();
    if path.starts_with("/v1/storage/") {
        log::info!("relaying a storage request; path={path}");
        if let Ok(resp) =
            super::proxy::relay_request(&mut req, SERVAL_SERVICE_STORAGE, &state.instance_id).await
        {
            Ok(resp)
        } else {
            // Welp, not much we can do
            Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                format!("{SERVAL_SERVICE_STORAGE} not available"),
            )
                .into_response())
        }
    } else {
        Ok(next.run(req).await)
    }
}

/// Fetch an executable by fully-qualified manifest name.
async fn get_executable(
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
async fn get_manifest(
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
async fn store_executable(
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
async fn has_manifest(Path(name): Path<String>, State(_state): State<AppState>) -> StatusCode {
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

async fn list_manifests(State(_state): State<AppState>) -> impl IntoResponse {
    let storage = STORAGE.get().unwrap();

    match storage.manifest_names() {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn store_manifest(State(_state): State<AppState>, body: String) -> impl IntoResponse {
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

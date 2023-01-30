use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use engine::ServalEngine;
use utils::structs::WasmResult;

use crate::structures::*;

/// This is the main worker endpoint. It accepts incoming jobs and runs them.
pub async fn incoming(state: State<AppState>, mut multipart: Multipart) -> Response {
    let mut envelope: Option<Envelope> = None;
    let mut binary: Option<Vec<u8>> = None;
    let mut input: Option<Vec<u8>> = None;

    // chomp up form input here
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.unwrap();
        match name.as_str() {
            "envelope" => {
                let data = data.to_vec();
                let Ok(parsed) = serde_json::from_slice(&data) else {
                    // this is not good enough
                    return (StatusCode::BAD_REQUEST, "job envelope is invalid".to_string()).into_response();
                };
                envelope = Some(parsed);
            }
            "input" => {
                input = Some(data.to_vec());
            }
            "executable" => {
                binary = Some(data.to_vec());
            }
            _ => {
                log::info!("ignoring unknown field `{name}`");
            }
        }
    }

    let Some(binary) = binary else {
        return (
            StatusCode::BAD_REQUEST,
            "no wasm executable data provided!".to_string(),
        )
            .into_response();
    };

    let envelope = envelope.unwrap_or_default();
    let metadata: JobMetadata = JobMetadata::from(envelope);
    log::info!(
        "received WASM job; name={}; executable length={}; input length={}",
        metadata.name(),
        binary.len(),
        input.as_ref().map(|input| input.len()).unwrap_or_else(|| 0),
    );

    run_job_inner(state, metadata, binary, input).await
}

async fn run_job_inner(
    state: State<AppState>,
    metadata: JobMetadata,
    binary: Vec<u8>,
    input: Option<Vec<u8>>,
) -> Response {
    let mut state = state.lock().await;

    // Poor human's history tracking here. We'll need to do better at some point.
    // E.g., handle overflows. That would be some nice uptime.
    state.total += 1;
    state
        .jobs
        .insert(metadata.id().to_string(), metadata.clone());

    let start = std::time::Instant::now();

    // What we'll do later is accept this job for processing and send it to a thread or something.
    // But for now we do it right here, in our handler.
    // The correct response by design is a 202 Accepted plus the metadata object.
    // TODO: SER-38 - capture exit code for failed jobs
    log::info!(
        "about to run job name={}; id={}; executable size={}",
        metadata.name(),
        metadata.id(),
        binary.len()
    );
    match execute_job(binary, input, &state.extensions).await {
        Ok(result) => {
            // We're not doing anything with stderr here.
            log::info!(
                "job completed; job={}; code={}; elapsed_ms={}",
                metadata.id(),
                result.code,
                start.elapsed().as_millis()
            );
            if result.code == 0 {
                // Zero exit status code is a success.
                (StatusCode::OK, result.stdout).into_response()
            } else {
                // Now the fun part of http error signaling: the request was successful, but the
                // result of the operation was bad from the user's point of view. Our behavior here
                // is yet to be defined but I'm sending back stderr just to show we can.
                (StatusCode::OK, result.stderr).into_response()
            }
        }
        Err(e) => {
            state.errors += 1;
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Run a job in the wasm engine.
// Probably can vanish because there's only one caller.
async fn execute_job(
    executable: Vec<u8>,
    input: Option<Vec<u8>>,
    extensions: &HashMap<String, PathBuf>,
) -> Result<WasmResult> {
    let stdin = input.unwrap_or_default();

    let mut engine = ServalEngine::new(extensions.clone())?;
    let result = engine.execute(&executable, &stdin)?;

    Ok(result)
}

/// Run a previously-stored job by address. Fast hack. Feel free to improve with an input feature.
pub async fn run_stored_job(
    state: State<AppState>,
    Path(blob_addr): Path<String>,
) -> impl IntoResponse {
    let locked = state.lock().await;

    let Some(storage) = locked.storage.as_ref() else {
        // todo: in this case, we should proxy this request to another node that is advertising the serval_storage role
        return (StatusCode::SERVICE_UNAVAILABLE, "Storage is not available").into_response();
    };

    let Ok(binary) = storage.get_bytes(&blob_addr).await else {
        return (
            StatusCode::NOT_FOUND,
            format!("Blob {} not found", &blob_addr),
        )
            .into_response();
    };

    let metadata = JobMetadata::new(blob_addr.clone(), "stored binary".to_string());
    drop(locked);
    run_job_inner(state, metadata, binary, None).await
}

pub async fn monitor_history(state: State<AppState>) -> Json<RunnerState> {
    let state = state.lock().await;
    Json(state.clone())
}

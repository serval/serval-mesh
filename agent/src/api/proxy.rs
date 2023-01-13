use anyhow::Result;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::*;

pub async fn proxy_unavailable_services<B>(
    State(state): State<AppState>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    if req.uri().path().starts_with("/storage/") {
        let state = state.lock().await;
        if state.storage.is_none() {
            log::info!(
                "proxy_unavailable_services intecepting request; path={}",
                req.uri().path()
            );
            // todo: In this case, we should proxy this request to another node that is advertising
            // the serval_storage role. For now, let's just barf.
            return Ok((StatusCode::SERVICE_UNAVAILABLE, "Storage not available").into_response());
        }
    }

    let response = next.run(req).await;
    Ok(response)
}

use anyhow::Result;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::header::{CONTENT_LENGTH, EXPECT, HOST};
use mdns_sd::ServiceInfo;
use utils::{errors::ServalError, mdns::discover_service};

use super::*;

pub async fn proxy_unavailable_services<B>(
    State(state): State<AppState>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    if path.starts_with("/storage/") {
        let state = state.lock().await;
        if state.storage.is_none() {
            log::info!(
                "proxy_unavailable_services intercepting request; path={}",
                path,
            );

            let Ok(resp) = proxy_request_to_service(&req, "_serval_storage").await else {
                // Welp, not much we can do
                return Ok((StatusCode::SERVICE_UNAVAILABLE, "Storage not available").into_response());
            };

            return Ok(resp);
        }
    }

    let response = next.run(req).await;
    Ok(response)
}

// proxies the given request to to the first node that we discover that is advertising the given
// service. in the future, we may keep a list of known nodes for a given service so we can avoid
// running the discovery process for every proxy request.
async fn proxy_request_to_service<B>(
    req: &Request<B>,
    service_name: &str,
) -> Result<Response, ServalError> {
    let node_info = discover_service(service_name).await.map_err(|err| {
        log::warn!("proxy_unavailable_services failed to find a node offering the service; service={service_name}; err={err:?}");
        err
    })?;

    let result = proxy_request_to_other_node(req, &node_info).await;
    result
}

async fn proxy_request_to_other_node<B>(
    req: &Request<B>,
    info: &ServiceInfo,
) -> Result<Response, ServalError> {
    let host = info.get_addresses().iter().next().unwrap(); // unwrap is safe because discover_service will never return a service without addresses
    let port = info.get_port();
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|qs| format!("?{qs}"))
        .unwrap_or_else(|| "".to_string());
    let url = format!("http://{host}:{port}{path}{query}");
    let mut inner_req = reqwest::Client::new().request(req.method().clone(), url);

    // Copy over the headers, modulo a few that are only relevant to the original request
    for (k, v) in req.headers().iter() {
        if k == CONTENT_LENGTH || k == EXPECT || k == HOST {
            continue;
        }
        inner_req = inner_req.header(k, v);
    }
    inner_req = inner_req.header(
        "Serval-Proxied-For",
        "<todo: put instance_id here once it exists in AppState>",
    );

    // Actually send the request
    inner_req
        .send()
        .await
        .map(reqwest_response_to_axum_response)?
        .await
        .map(|mut resp| {
            resp.headers_mut().append(
                "Serval-Proxied-From",
                "<todo: put instance_id from `info`'s properties here>"
                    .parse()
                    .unwrap(),
            );
            resp
        })
        .map_err(|err| {
            log::warn!("Failed to proxy request to other node; node={host}:{port}; err={err:?}");
            err
        })
}

async fn reqwest_response_to_axum_response(
    reqwest_resp: reqwest::Response,
) -> Result<Response, ServalError> {
    let inner_status = reqwest_resp.status();
    let inner_headers = reqwest_resp.headers().to_owned();
    let addr = reqwest_resp.remote_addr();
    let inner_body = reqwest_resp.bytes().await.map_err(|err| {
        log::warn!("Failed to read response from proxy node; addr={addr:?}; err={err:?}");
        err
    })?;
    let mut axum_resp = (inner_status, inner_body).into_response();

    // Remove any headers that axum hallucinated into the response if the reqwest response has them;
    // in particular, it will set a content-type of application/octet-stream, which we don't need if
    // the reqwest_resp has a content-type header of its own.
    let headers = axum_resp.headers_mut();
    for k in inner_headers.keys() {
        if inner_headers.contains_key(k) {
            headers.remove(k);
        }
    }

    for (k, v) in inner_headers.iter() {
        headers.append(k, v.clone());
    }

    Ok(axum_resp)
}

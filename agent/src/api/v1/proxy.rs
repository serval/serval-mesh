use anyhow::Result;
use axum::{
    body::{Body, HttpBody},
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::header::{CONTENT_LENGTH, EXPECT, HOST};
use mdns_sd::ServiceInfo;
use utils::{
    errors::ServalError,
    mdns::{discover_service, get_service_instance_id},
};

use super::*;
use crate::structures::AppState;

pub async fn proxy_unavailable_services(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next<Body>,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    if path.starts_with("/v1/storage/") {
        let state = state.lock().await;
        if state.storage.is_none() {
            log::info!(
                "proxy_unavailable_services intercepting request; path={}",
                path,
            );
            let Ok(resp) = proxy_request_to_service(&mut req, "_serval_storage", &state.instance_id).await else {
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
async fn proxy_request_to_service(
    req: &mut Request<Body>,
    service_name: &str,
    source_instance_id: &Uuid,
) -> Result<Response, ServalError> {
    let node_info = discover_service(service_name).await.map_err(|err| {
        log::warn!("proxy_unavailable_services failed to find a node offering the service; service={service_name}; err={err:?}");
        err
    })?;

    let result = proxy_request_to_other_node(req, &node_info, source_instance_id).await;
    result.map_err(|err| {
        log::warn!("Failed to proxy request to other node; node={node_info:?}; err={err:?}");
        err
    })
}

async fn proxy_request_to_other_node(
    req: &mut Request<Body>,
    info: &ServiceInfo,
    source_instance_id: &Uuid,
) -> Result<Response, ServalError> {
    let target_instance_id = get_service_instance_id(info)?;
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
        HeaderValue::from_str(&source_instance_id.to_string()).map_err(anyhow::Error::from)?,
    );

    // Copy the body over
    if let Some(req_body_bytes_res) = req.body_mut().data().await {
        if let Ok(req_body_bytes) = req_body_bytes_res {
            inner_req = inner_req.body(req_body_bytes);
        } else {
            log::warn!("Failed to copy body bytes over; aborting this request");
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to copy body bytes",
            )
                .into_response());
        }
    }

    // Actually send the request
    let inner_req_res = inner_req.send().await?;
    let mut resp = reqwest_response_to_axum_response(inner_req_res).await?;

    resp.headers_mut().append(
        "Serval-Proxied-From",
        HeaderValue::from_str(&target_instance_id.to_string()).map_err(anyhow::Error::from)?,
    );

    Ok(resp)
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

#[cfg(test)]
mod tests {

    use anyhow::anyhow;
    use axum::body::Bytes;
    use http::response::Builder;
    use reqwest::Response;
    use utils::futures::get_future_sync;

    use super::*;

    fn get_axum_body_as_bytes(resp: axum::response::Response) -> Result<Bytes> {
        let body = resp.into_body();
        let Some(body_bytes) = get_future_sync(hyper::body::to_bytes(body)).ok() else {
            return Err(anyhow!("Could not get body bytes"));
        };

        Ok(body_bytes)
    }

    #[test]
    fn test_reqwest_response_to_axum_response() {
        let mut reqwest_resp = Response::from(
            Builder::new()
                .status(418)
                .body("<whistling noises intensify>")
                .unwrap(),
        );
        reqwest_resp
            .headers_mut()
            .append("foo", HeaderValue::from_str("bar").unwrap());

        // Make sure the Reqwest response matches expectations
        assert_eq!(reqwest_resp.status(), 418);

        // Make sure the conversion works
        let result = get_future_sync(reqwest_response_to_axum_response(reqwest_resp));
        assert!(result.is_ok());
        let axum_resp = result.unwrap();
        assert_eq!(418, axum_resp.status());
        assert_eq!(
            HeaderValue::from_str("bar").unwrap(),
            axum_resp.headers().get("foo").unwrap()
        );
        let body_bytes: Vec<u8> = get_axum_body_as_bytes(axum_resp).unwrap().into();
        assert_eq!(
            "<whistling noises intensify>",
            String::from_utf8_lossy(&body_bytes)
        );
    }
}

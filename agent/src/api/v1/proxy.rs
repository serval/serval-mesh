use axum::body::{Body, HttpBody};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use http::header::{CONTENT_LENGTH, EXPECT, HOST};
use http::HeaderValue;
use utils::errors::ServalError;
use utils::mesh::{PeerMetadata, ServalRole};
use uuid::Uuid;

use crate::structures::MESH;

// Relay the given request to to the first node that we discover that is advertising the given
// service. in the future, we may keep a list of known nodes for a given service so we can avoid
// running the discovery process for every proxy request.
pub async fn relay_request(
    req: &mut Request<Body>,
    role: &ServalRole,
    source_instance_id: &Uuid,
) -> Result<Response, ServalError> {
    let mesh = MESH.get().expect("Peer network not initialized!");

    let candidates = mesh.peers_with_role(role).await;
    let Some(peer) = candidates.first() else {
        log::warn!("proxy_unavailable_services failed to find a node offering the service; service={role}");
        metrics::increment_counter!("proxy:no_service");
        return Err(ServalError::ServiceNotFound);
    };

    let result = proxy_request_to_other_node(req, peer, source_instance_id).await;
    result.map_err(|err| {
        log::warn!("Failed to proxy request to peer; peer={peer:?}; err={err:?}");
        metrics::increment_counter!("proxy:failure");
        err
    })
}

async fn proxy_request_to_other_node(
    req: &mut Request<Body>,
    peer: &PeerMetadata,
    source_instance_id: &Uuid,
) -> Result<Response, ServalError> {
    let target_instance_id = peer.instance_id();
    let http_address = peer.http_address();

    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|qs| format!("?{qs}"))
        .unwrap_or_else(|| "".to_string());
    // We know that we are only ever handed a candidate with a http_address.
    let url = format!("http://{}{path}{query}", http_address.unwrap());
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
        HeaderValue::from_str(target_instance_id).map_err(anyhow::Error::from)?,
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

    use anyhow::{anyhow, Result};
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

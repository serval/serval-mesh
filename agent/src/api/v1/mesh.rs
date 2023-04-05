use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use axum::{
    body::Body,
    extract::{Path, State},
    http::Request,
    response::IntoResponse,
    routing::get,
    Json,
};
use http::StatusCode;
use serde::Serialize;
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalRole};

use crate::structures::*;

pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/mesh/all", get(mesh_summary))
        .route("/v1/mesh/roles/:role", get(mesh_members_by_role))
}

#[derive(Serialize)]
struct MeshPeerInfo {
    http_address: SocketAddr,
    roles: Vec<ServalRole>,
}

impl MeshPeerInfo {
    fn new(peer: PeerMetadata) -> Self {
        MeshPeerInfo {
            http_address: peer.http_address().unwrap(),
            roles: peer.roles(),
        }
    }
}

#[derive(Serialize)]
struct MeshSummaryResponse {
    fingerprint: String,
    members: HashMap<String, MeshPeerInfo>,
}

async fn mesh_summary(
    State(_state): State<AppState>,
    _request: Request<Body>,
) -> Json<MeshSummaryResponse> {
    let mesh = MESH.get().expect("Peer network not initialized!");
    let peers = mesh.peers().await;

    let members = peers
        .iter()
        // todo: remove http_address check once it's no longer an Option
        .filter(|peer| peer.http_address().is_some())
        .map(|peer| {
            (
                peer.instance_id().to_owned(),
                MeshPeerInfo::new(peer.clone()),
            )
        })
        .collect();

    Json(MeshSummaryResponse {
        fingerprint: format!("{:08x}", mesh.fingerprint().await),
        members,
    })
}

type MeshMembersResponse = HashMap<String, MeshPeerInfo>;

async fn mesh_members_by_role(
    State(_state): State<AppState>,
    Path(role): Path<String>,
    _request: Request<Body>,
) -> Result<Json<MeshMembersResponse>, impl IntoResponse> {
    let mesh = MESH.get().expect("Peer network not initialized!");
    let peers = mesh.peers().await;

    let Ok(role) = ServalRole::from_str(&role) else {
    return Err((StatusCode::BAD_REQUEST, "Invalid role").into_response());
};

    let members = peers
        .iter()
        .filter_map(|peer| {
            // todo: remove http_address check once it's no longer an Option
            if peer.http_address().is_some() && peer.roles().contains(&role) {
                Some((
                    peer.instance_id().to_owned(),
                    MeshPeerInfo::new(peer.clone()),
                ))
            } else {
                None
            }
        })
        .collect();

    Ok(Json(members))
}

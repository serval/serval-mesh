use std::{collections::HashMap, net::SocketAddr};

use axum::{
    body::Body,
    extract::{Path, State},
    http::Request,
    routing::get,
    Json,
};
use serde::Serialize;
use utils::mesh::{KaboodleMesh, PeerMetadata, ServalRole};

use crate::structures::*;

pub fn mount(router: ServalRouter) -> ServalRouter {
    router
        .route("/v1/mesh", get(mesh_summary))
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
    Path(role): Path<ServalRole>,
    _request: Request<Body>,
) -> Json<MeshMembersResponse> {
    let mesh = MESH.get().expect("Peer network not initialized!");
    let peers = mesh.peers().await;

    let members = peers
        .iter()
        .filter_map(|peer| {
            if peer.roles().contains(&role) {
                Some((
                    peer.instance_id().to_owned(),
                    MeshPeerInfo::new(peer.clone()),
                ))
            } else {
                None
            }
        })
        .collect();

    Json(members)
}
